use anyhow::{Context, Result};
use deadpool_sqlite::Pool;
use tracing::{info, warn};

use crate::routes::notes::flatten_interact;

/// Generate audio for an episode — full TTS pipeline
pub async fn generate_episode_audio(
    client: &reqwest::Client,
    pool: &Pool,
    episode_id: i64,
) -> Result<()> {
    info!(episode_id, "Starting audio generation");

    // 1. Fetch episode from DB
    let pool2 = pool.clone();
    let episode = flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| crate::db::queries::get_audio_episode(conn, episode_id))
            .await,
    )?;

    // 2. Update status to processing
    let pool3 = pool.clone();
    flatten_interact(
        pool3
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                crate::db::queries::update_episode_status(
                    conn,
                    episode_id,
                    "processing",
                    None,
                    None,
                    None,
                    None,
                )
            })
            .await,
    )?;

    // 3. Fetch all linked notes
    let note_ids = episode.note_ids.clone();
    let notes = flatten_interact(
        pool2
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| crate::db::queries::get_notes_by_ids(conn, &note_ids))
            .await,
    )?;

    if notes.is_empty() {
        anyhow::bail!("No notes found for episode {}", episode_id);
    }

    // 4. Determine TTS provider and voice
    let provider = TtsProvider::from_setting(&episode.tts_provider);
    let voice = episode
        .tts_voice
        .as_deref()
        .unwrap_or(provider.default_voice());

    // 5. Prepare text for each note
    let mut all_audio_chunks: Vec<Vec<u8>> = Vec::new();

    for (i, note) in notes.iter().enumerate() {
        let text = if episode.content_mode == "summary" {
            // Use existing summary if available, otherwise use full content
            note.summary.as_deref().unwrap_or(&note.content)
        } else {
            &note.content
        };

        if text.is_empty() || text == "[Voice memo - transcribing...]" {
            warn!(
                note_id = note.id,
                "Skipping note with empty or pending content"
            );
            continue;
        }

        // Add note separator for multi-note episodes
        let text_with_context = if notes.len() > 1 && i > 0 {
            let title = note.title.as_deref().unwrap_or("Next note");
            format!("{}. {}", title, text)
        } else {
            text.to_string()
        };

        // Chunk text for API limits
        let chunks = chunk_text(&text_with_context, provider.chunk_limit());

        for chunk in chunks {
            let audio_bytes = synthesize(client, &provider, voice, &chunk).await?;
            all_audio_chunks.push(audio_bytes);
        }
    }

    if all_audio_chunks.is_empty() {
        anyhow::bail!("No audio generated — all notes were empty or pending");
    }

    // 6. Save audio file(s) and concatenate if needed
    let db_path = std::env::var("NOTE_DB").unwrap_or_else(|_| "note.db".to_string());
    let db_dir = std::path::Path::new(&db_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let audio_dir = db_dir.join("audio");
    tokio::fs::create_dir_all(&audio_dir).await?;

    let audio_filename = audio_output_filename(&episode.title, episode_id);
    let final_path = audio_dir.join(&audio_filename);

    if all_audio_chunks.len() == 1 {
        // Single chunk — write directly
        tokio::fs::write(&final_path, &all_audio_chunks[0]).await?;
    } else {
        // Multiple chunks — concatenate with ffmpeg
        let mut temp_paths = Vec::new();
        for (i, chunk) in all_audio_chunks.iter().enumerate() {
            let temp_path = audio_dir.join(format!(
                "{}-chunk-{}.mp3",
                audio_filename.trim_end_matches(".mp3"),
                i
            ));
            tokio::fs::write(&temp_path, chunk).await?;
            temp_paths.push(temp_path);
        }

        concat_mp3_files(&temp_paths, &final_path).await?;

        // Clean up temp chunk files
        for temp_path in &temp_paths {
            tokio::fs::remove_file(temp_path).await.ok();
        }
    }

    // 7. Measure duration
    let duration_path = final_path.clone();
    let duration = tokio::task::spawn_blocking(move || {
        mp3_duration::from_path(&duration_path)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    })
    .await
    .unwrap_or(0.0);

    let file_size = tokio::fs::metadata(&final_path)
        .await
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    let audio_path_str = format!("audio/{audio_filename}");

    // 8. Update DB with completion
    let pool4 = pool.clone();
    let path_clone = audio_path_str.clone();
    flatten_interact(
        pool4
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                crate::db::queries::update_episode_status(
                    conn,
                    episode_id,
                    "complete",
                    Some(&path_clone),
                    Some(file_size),
                    Some(duration),
                    None,
                )
            })
            .await,
    )?;

    // 9. Track API usage
    let char_count = notes.iter().map(|n| n.content.len() as i64).sum::<i64>();
    let service = episode.tts_provider.clone();
    flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                let cost = match service.as_str() {
                    "openai" => char_count as f64 * 0.000015, // $15/1M chars
                    "gemini" => char_count as f64 * 0.000010, // ~$10/1M tokens
                    "elevenlabs" => char_count as f64 * 0.000030, // approximate blended ElevenLabs TTS cost
                    _ => 0.0,
                };
                crate::db::queries::record_usage(
                    conn, &service, "tts", None, char_count, 0, cost, None,
                )
            })
            .await,
    )?;

    info!(
        episode_id,
        duration_seconds = duration,
        file_size_bytes = file_size,
        audio_path = audio_path_str,
        "Audio generation complete"
    );

    Ok(())
}

/// Mark an episode as failed
pub async fn mark_episode_failed(pool: &Pool, episode_id: i64, error: &str) -> Result<()> {
    let error = error.to_string();
    flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                crate::db::queries::update_episode_status(
                    conn,
                    episode_id,
                    "failed",
                    None,
                    None,
                    None,
                    Some(&error),
                )
            })
            .await,
    )?;
    Ok(())
}

// --- TTS Provider Implementations ---

enum TtsProvider {
    OpenAi,
    Gemini,
    ElevenLabs,
}

impl TtsProvider {
    fn from_setting(provider: &str) -> Self {
        match provider {
            "gemini" => Self::Gemini,
            "elevenlabs" => Self::ElevenLabs,
            _ => Self::OpenAi,
        }
    }

    fn default_voice(&self) -> &'static str {
        match self {
            Self::OpenAi => "alloy",
            Self::Gemini => "Kore",
            Self::ElevenLabs => "N2lVS1wzUtoSnaSjtS9X",
        }
    }

    fn chunk_limit(&self) -> usize {
        match self {
            Self::ElevenLabs => 5000,
            _ => 4096,
        }
    }
}

async fn synthesize(
    client: &reqwest::Client,
    provider: &TtsProvider,
    voice: &str,
    text: &str,
) -> Result<Vec<u8>> {
    match provider {
        TtsProvider::OpenAi => openai_tts(client, text, voice).await,
        TtsProvider::Gemini => gemini_tts(client, text, voice).await,
        TtsProvider::ElevenLabs => elevenlabs_tts(client, text, voice).await,
    }
}

async fn openai_tts(client: &reqwest::Client, text: &str, voice: &str) -> Result<Vec<u8>> {
    let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not set")?;

    let response = client
        .post("https://api.openai.com/v1/audio/speech")
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "model": "tts-1",
            "input": text,
            "voice": voice,
            "response_format": "mp3"
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let msg = classify_api_error("OpenAI", status.as_u16(), &body);
        anyhow::bail!(msg);
    }

    Ok(response.bytes().await?.to_vec())
}

async fn gemini_tts(client: &reqwest::Client, text: &str, voice: &str) -> Result<Vec<u8>> {
    let api_key = std::env::var("GEMINI_API_KEY").context("GEMINI_API_KEY not set")?;

    let model = "gemini-2.5-flash-preview-tts";
    let url =
        format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");

    let response = client
        .post(&url)
        .header("x-goog-api-key", &api_key)
        .json(&serde_json::json!({
            "contents": [{
                "parts": [{ "text": text }]
            }],
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "speechConfig": {
                    "voiceConfig": {
                        "prebuiltVoiceConfig": {
                            "voiceName": voice
                        }
                    }
                }
            }
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let msg = classify_api_error("Gemini", status.as_u16(), &body);
        anyhow::bail!(msg);
    }

    // Parse JSON response with base64-encoded PCM audio
    let json: serde_json::Value = response.json().await?;
    let audio_data = json["candidates"][0]["content"]["parts"][0]["inlineData"]["data"]
        .as_str()
        .context("No audio data in Gemini response")?;

    let pcm_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, audio_data)?;

    // Convert PCM to MP3 via ffmpeg
    pcm_to_mp3(&pcm_bytes).await
}

fn elevenlabs_request_body(text: &str, voice: &str) -> serde_json::Value {
    serde_json::json!({
        "text": text,
        "model_id": std::env::var("ELEVENLABS_TTS_MODEL").unwrap_or_else(|_| "eleven_v3".to_string()),
        "voice_id": voice,
        "voice_settings": {
            "stability": 0.65,
            "similarity_boost": 0.75,
            "style": 0.0,
            "use_speaker_boost": true
        }
    })
}

async fn elevenlabs_tts(client: &reqwest::Client, text: &str, voice: &str) -> Result<Vec<u8>> {
    let api_key = std::env::var("ELEVENLABS_API_KEY").context("ELEVENLABS_API_KEY not set")?;
    let url =
        format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format=mp3_44100_128");

    let response = client
        .post(&url)
        .header("xi-api-key", api_key)
        .header("Accept", "audio/mpeg")
        .json(&elevenlabs_request_body(text, voice))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let msg = classify_api_error("ElevenLabs", status.as_u16(), &body);
        anyhow::bail!(msg);
    }

    Ok(response.bytes().await?.to_vec())
}

async fn pcm_to_mp3(pcm_bytes: &[u8]) -> Result<Vec<u8>> {
    let tmp_dir = std::env::temp_dir();
    let pcm_path = tmp_dir.join(format!("tts_pcm_{}.raw", std::process::id()));
    let mp3_path = tmp_dir.join(format!("tts_mp3_{}.mp3", std::process::id()));

    tokio::fs::write(&pcm_path, pcm_bytes).await?;

    let result = tokio::process::Command::new("/opt/homebrew/bin/ffmpeg")
        .args(["-f", "s16le", "-ar", "24000", "-ac", "1", "-i"])
        .arg(&pcm_path)
        .args(["-b:a", "128k", "-y"])
        .arg(&mp3_path)
        .output()
        .await
        .context("Failed to run ffmpeg for PCM to MP3 conversion")?;

    // Clean up PCM temp file
    tokio::fs::remove_file(&pcm_path).await.ok();

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        anyhow::bail!("ffmpeg PCM→MP3 failed: {stderr}");
    }

    let mp3_bytes = tokio::fs::read(&mp3_path).await?;
    tokio::fs::remove_file(&mp3_path).await.ok();

    Ok(mp3_bytes)
}

// --- Text Chunking ---

/// Split text into chunks of ≤ max_chars, breaking on sentence boundaries.
fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.len() <= max_chars {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    // Split on sentence-ending punctuation followed by whitespace
    for sentence in split_sentences(text) {
        if current.len() + sentence.len() > max_chars {
            if !current.is_empty() {
                chunks.push(current.trim().to_string());
                current = String::new();
            }
            // If a single sentence exceeds max_chars, split it further
            if sentence.len() > max_chars {
                for sub in split_on_clauses(&sentence, max_chars) {
                    chunks.push(sub);
                }
                continue;
            }
        }
        current.push_str(&sentence);
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if ch == '.' || ch == '!' || ch == '?' {
            // Look ahead for whitespace to confirm sentence boundary
            sentences.push(current.clone());
            current.clear();
        }
    }

    if !current.is_empty() {
        sentences.push(current);
    }

    sentences
}

fn split_on_clauses(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for part in text.split_inclusive([',', ';', ':']) {
        if current.len() + part.len() > max_chars && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current = String::new();
        }
        current.push_str(part);
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    // Last resort: hard-split if still too long
    let mut final_chunks = Vec::new();
    for chunk in chunks {
        if chunk.len() > max_chars {
            for i in (0..chunk.len()).step_by(max_chars) {
                let end = (i + max_chars).min(chunk.len());
                final_chunks.push(chunk[i..end].to_string());
            }
        } else {
            final_chunks.push(chunk);
        }
    }

    final_chunks
}

// --- Audio File Naming ---

fn audio_output_filename(title: &str, episode_id: i64) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in title.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        return format!("episode-{episode_id}.mp3");
    }

    let suffix = format!("-{episode_id}.mp3");
    let max_total_len = 96usize;
    let max_slug_len = max_total_len.saturating_sub(suffix.len());
    let mut slug = slug.chars().take(max_slug_len).collect::<String>();
    slug = slug.trim_matches('-').to_string();

    if slug.is_empty() {
        format!("episode-{episode_id}.mp3")
    } else {
        format!("{slug}{suffix}")
    }
}

// --- Audio Concatenation ---

async fn concat_mp3_files(paths: &[std::path::PathBuf], output: &std::path::Path) -> Result<()> {
    if paths.is_empty() {
        anyhow::bail!("No audio files to concatenate");
    }

    if paths.len() == 1 {
        tokio::fs::copy(&paths[0], output).await?;
        return Ok(());
    }

    // Create ffmpeg concat file list
    let tmp_dir = std::env::temp_dir();
    let list_path = tmp_dir.join(format!("concat_list_{}.txt", std::process::id()));
    let list_content: String = paths
        .iter()
        .map(|p| format!("file '{}'", p.display()))
        .collect::<Vec<_>>()
        .join("\n");

    tokio::fs::write(&list_path, &list_content).await?;

    let result = tokio::process::Command::new("/opt/homebrew/bin/ffmpeg")
        .args(["-f", "concat", "-safe", "0", "-i"])
        .arg(&list_path)
        .args(["-c", "copy", "-y"])
        .arg(output)
        .output()
        .await
        .context("Failed to run ffmpeg for audio concatenation")?;

    tokio::fs::remove_file(&list_path).await.ok();

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        anyhow::bail!("ffmpeg concat failed: {stderr}");
    }

    Ok(())
}

/// Classify HTTP error status codes into user-friendly messages
fn classify_api_error(provider: &str, status: u16, body: &str) -> String {
    match status {
        401 | 403 => {
            format!("{provider} API key is invalid or expired. Check your API key configuration.")
        }
        402 => format!(
            "{provider} account has no credits remaining. Add billing to your {provider} account."
        ),
        429 => format!("{provider} rate limit exceeded. Try again in a few minutes."),
        _ => format!("{provider} TTS failed ({status}): {body}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_from_setting_supports_elevenlabs() {
        assert!(matches!(
            TtsProvider::from_setting("elevenlabs"),
            TtsProvider::ElevenLabs
        ));
    }

    #[test]
    fn default_voice_for_elevenlabs_is_financial_strategist() {
        assert_eq!(
            TtsProvider::ElevenLabs.default_voice(),
            "N2lVS1wzUtoSnaSjtS9X"
        );
    }

    #[test]
    fn elevenlabs_uses_larger_chunk_limit() {
        assert_eq!(TtsProvider::ElevenLabs.chunk_limit(), 5000);
    }

    #[test]
    fn elevenlabs_request_body_uses_business_audio_defaults() {
        let body = elevenlabs_request_body("Quarterly EBITDA update", "N2lVS1wzUtoSnaSjtS9X");

        assert_eq!(body["model_id"], "eleven_v3");
        assert_eq!(body["text"], "Quarterly EBITDA update");
        assert_eq!(body["voice_settings"]["stability"], 0.65);
        assert_eq!(body["voice_settings"]["style"], 0.0);
    }

    #[test]
    fn audio_output_filename_uses_descriptive_kebab_case_title_and_id() {
        assert_eq!(
            audio_output_filename("Q4 Market Analysis: EBITDA & QE", 42),
            "q4-market-analysis-ebitda-qe-42.mp3"
        );
    }

    #[test]
    fn audio_output_filename_falls_back_to_episode_id_when_title_has_no_words() {
        assert_eq!(audio_output_filename("!!!", 42), "episode-42.mp3");
    }

    #[test]
    fn audio_output_filename_truncates_long_titles_without_trailing_dash() {
        let filename = audio_output_filename(&"market ".repeat(30), 42);

        assert!(filename.ends_with("-42.mp3"));
        assert!(!filename.contains("--"));
        assert!(filename.len() <= 96);
    }

    #[test]
    fn chunk_text_short() {
        let chunks = chunk_text("Hello world.", 4096);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world.");
    }

    #[test]
    fn chunk_text_splits_on_sentences() {
        let text = "First sentence. Second sentence. Third sentence.";
        let chunks = chunk_text(text, 30);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= 30, "Chunk too long: {} chars", chunk.len());
        }
    }

    #[test]
    fn chunk_text_empty() {
        let chunks = chunk_text("", 4096);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn chunk_text_exactly_at_limit() {
        let text = "a".repeat(4096);
        let chunks = chunk_text(&text, 4096);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn chunk_text_long_single_sentence() {
        let text = "a".repeat(8192);
        let chunks = chunk_text(&text, 4096);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= 4096);
        }
    }

    #[test]
    fn chunk_text_exactly_at_limit_preserves_content() {
        let text = "a".repeat(4096);
        let chunks = chunk_text(&text, 4096);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 4096);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn chunk_text_with_exclamation_marks() {
        let text = "Wow! Amazing! Great work.";
        let chunks = chunk_text(text, 10);
        assert!(
            chunks.len() >= 2,
            "Exclamation marks should split sentences"
        );
    }

    #[test]
    fn chunk_text_trailing_content_without_period() {
        let text = "First sentence. Some trailing content without period";
        let chunks = chunk_text(text, 30);
        let all_text: String = chunks.join("");
        assert!(
            all_text.contains("trailing"),
            "Trailing content must not be lost"
        );
    }

    #[test]
    fn chunk_text_no_content_lost() {
        let text = "One sentence. Two sentence. Three sentence. Four sentence.";
        let chunks = chunk_text(text, 20);
        let reassembled: String = chunks.join("");
        // All words must survive chunking
        for word in ["One", "Two", "Three", "Four"] {
            assert!(reassembled.contains(word), "Lost word: {}", word);
        }
    }

    #[test]
    fn chunk_text_clause_splitting() {
        // Single long "sentence" with clauses — forces clause-level splitting
        let text = format!(
            "{},{},{},{}.",
            "a".repeat(30),
            "b".repeat(30),
            "c".repeat(30),
            "d".repeat(30)
        );
        let chunks = chunk_text(&text, 40);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(
                chunk.len() <= 40,
                "Chunk exceeded limit: {} chars",
                chunk.len()
            );
        }
    }
}

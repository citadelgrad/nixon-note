use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use tracing::{info, warn};

use crate::AppState;
use crate::db::queries;
use crate::routes::notes::{AppError, flatten_interact};

// --- Request/Response Types ---

#[derive(Deserialize)]
pub struct GenerateAudioRequest {
    pub note_ids: Vec<i64>,
    #[serde(default = "default_episode_type")]
    pub episode_type: String,
    #[serde(default = "default_content_mode")]
    pub content_mode: String,
    pub title: Option<String>,
}

fn default_episode_type() -> String {
    "single".to_string()
}

fn default_content_mode() -> String {
    "full".to_string()
}

#[derive(Serialize)]
pub struct GenerateAudioResponse {
    pub episode_id: i64,
    pub status: String,
}

// --- Handlers ---

/// POST /api/audio/generate
pub async fn generate_audio(
    State(state): State<AppState>,
    Json(body): Json<GenerateAudioRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate note_ids
    if body.note_ids.is_empty() && body.episode_type != "digest" {
        return Err(AppError::BadRequest("note_ids must not be empty".into()));
    }

    // Validate episode_type
    if !["single", "batch", "digest"].contains(&body.episode_type.as_str()) {
        return Err(AppError::BadRequest(
            "episode_type must be 'single', 'batch', or 'digest'".into(),
        ));
    }

    // Validate content_mode
    if !["full", "summary"].contains(&body.content_mode.as_str()) {
        return Err(AppError::BadRequest(
            "content_mode must be 'full' or 'summary'".into(),
        ));
    }

    // For single, must have exactly 1 note
    if matches!(body.episode_type.as_str(), "single") && body.note_ids.len() != 1 {
        return Err(AppError::BadRequest(
            "single episode_type requires exactly 1 note_id".into(),
        ));
    }

    // Determine note_ids (for digest, auto-collect today's notes)
    let note_ids = if matches!(body.episode_type.as_str(), "digest") {
        let pool = state.pool.clone();
        flatten_interact(
            pool.get()
                .await
                .map_err(anyhow::Error::from)?
                .interact(|conn| {
                    let mut stmt = conn.prepare(
                        "SELECT id FROM notes WHERE date(created_at) = date('now') ORDER BY id ASC",
                    )?;
                    let ids: Vec<i64> = stmt
                        .query_map([], |row| row.get(0))?
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok::<Vec<i64>, anyhow::Error>(ids)
                })
                .await,
        )?
    } else {
        body.note_ids.clone()
    };

    if note_ids.is_empty() {
        return Err(AppError::BadRequest("No notes found to convert".into()));
    }

    // Check for pending transcription notes
    let ids_clone = note_ids.clone();
    let pool = state.pool.clone();
    let notes = flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_notes_by_ids(conn, &ids_clone))
            .await,
    )?;

    for note in &notes {
        if note.content == "[Voice memo - transcribing...]" {
            return Err(AppError::BadRequest(format!(
                "Note {} is still being transcribed",
                note.id
            )));
        }
    }

    // Get TTS settings
    let pool = state.pool.clone();
    let settings = flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(|conn| queries::get_all_settings(conn))
            .await,
    )?;

    let tts_provider = settings
        .get("tts_provider")
        .cloned()
        .unwrap_or_else(|| "openai".to_string());
    let tts_voice = settings
        .get(&format!("tts_voice_{}", tts_provider))
        .cloned()
        .unwrap_or_else(|| match tts_provider.as_str() {
            "gemini" => "Kore".to_string(),
            "elevenlabs" => "N2lVS1wzUtoSnaSjtS9X".to_string(),
            _ => "alloy".to_string(),
        });

    // Auto-generate title if not provided. Notes may not have gone through
    // background auto-organization yet, so fall back to content-derived titles
    // rather than creating useless "Untitled Note" audio episodes.
    let title = body
        .title
        .as_deref()
        .and_then(clean_title)
        .unwrap_or_else(|| derive_episode_title(&body.episode_type, &notes));

    // Fail fast: check that the selected TTS provider's API key is configured
    match tts_provider.as_str() {
        "openai"
            if std::env::var("OPENAI_API_KEY")
                .unwrap_or_default()
                .is_empty() =>
        {
            return Err(AppError::BadRequest(
                "OpenAI API key is not configured. Add OPENAI_API_KEY to enable TTS.".into(),
            ));
        }
        "gemini"
            if std::env::var("GEMINI_API_KEY")
                .unwrap_or_default()
                .is_empty() =>
        {
            return Err(AppError::BadRequest(
                "Gemini API key is not configured. Add GEMINI_API_KEY to enable TTS.".into(),
            ));
        }
        "elevenlabs"
            if std::env::var("ELEVENLABS_API_KEY")
                .unwrap_or_default()
                .is_empty() =>
        {
            return Err(AppError::BadRequest(
                "ElevenLabs API key is not configured. Add ELEVENLABS_API_KEY to enable TTS."
                    .into(),
            ));
        }
        _ => {}
    }

    // Create episode in DB
    let etype = body.episode_type.clone();
    let ids = note_ids.clone();
    let pool = state.pool.clone();
    let episode_id = flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                queries::create_audio_episode(
                    conn,
                    &title,
                    &etype,
                    &body.content_mode,
                    &tts_provider,
                    &tts_voice,
                    &ids,
                )
            })
            .await,
    )?;

    // Enqueue for background processing
    if let Err(e) = state.background.enqueue_audio(episode_id).await {
        warn!(episode_id, error = ?e, "Failed to enqueue audio generation");
    }

    info!(
        episode_id,
        note_count = note_ids.len(),
        episode_type = body.episode_type,
        "Audio generation enqueued"
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(GenerateAudioResponse {
            episode_id,
            status: "pending".to_string(),
        }),
    ))
}

/// GET /api/audio
pub async fn list_episodes(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let episodes = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(|conn| queries::list_audio_episodes(conn))
            .await,
    )?;

    Ok(Json(serde_json::json!({ "episodes": episodes })))
}

/// GET /api/audio/:id
pub async fn get_episode(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let episode = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_audio_episode(conn, id))
            .await,
    )?;

    Ok(Json(episode))
}

/// DELETE /api/audio/:id
pub async fn delete_episode(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let audio_path = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::delete_audio_episode(conn, id))
            .await,
    )?;

    // Delete audio file if it exists
    if let Some(path) = audio_path {
        let db_path = std::env::var("NOTE_DB").unwrap_or_else(|_| "note.db".to_string());
        let db_dir = std::path::Path::new(&db_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let full_path = db_dir.join(&path);
        tokio::fs::remove_file(&full_path).await.ok();
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/audio/:id/file
pub async fn serve_audio_file(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    req_headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let episode = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_audio_episode(conn, id))
            .await,
    )?;

    if episode.status != "complete" {
        return Err(AppError::BadRequest(format!(
            "Episode {} is not complete (status: {})",
            id, episode.status
        )));
    }

    let audio_path = episode
        .audio_path
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("No audio path for episode {}", id))?;

    let db_path = std::env::var("NOTE_DB").unwrap_or_else(|_| "note.db".to_string());
    let db_dir = std::path::Path::new(&db_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let full_path = db_dir.join(audio_path);

    let mut file = tokio::fs::File::open(&full_path)
        .await
        .map_err(|_| anyhow::anyhow!("Audio file not found: {}", full_path.display()))?;

    let metadata = file
        .metadata()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read file metadata: {e}"))?;

    let file_size = metadata.len();

    // Parse Range header for seeking support
    let range = req_headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("bytes="))
        .and_then(|s| {
            let mut parts = s.splitn(2, '-');
            let start: u64 = parts.next()?.parse().ok()?;
            let end: u64 = parts
                .next()
                .and_then(|e| if e.is_empty() { None } else { e.parse().ok() })
                .unwrap_or(file_size - 1);
            Some((start, end))
        });

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "audio/mpeg".parse().unwrap());
    headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    headers.insert(
        header::CACHE_CONTROL,
        "public, max-age=86400".parse().unwrap(),
    );

    if let Some((start, end)) = range {
        let end = end.min(file_size - 1);
        let length = end - start + 1;

        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to seek: {e}"))?;

        let stream = ReaderStream::new(file.take(length));
        let body = Body::from_stream(stream);

        headers.insert(header::CONTENT_LENGTH, length.to_string().parse().unwrap());
        headers.insert(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{file_size}").parse().unwrap(),
        );

        Ok((StatusCode::PARTIAL_CONTENT, headers, body).into_response())
    } else {
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);

        headers.insert(
            header::CONTENT_LENGTH,
            file_size.to_string().parse().unwrap(),
        );

        Ok((headers, body).into_response())
    }
}

/// GET /api/podcast/feed.xml
pub async fn podcast_feed(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let episodes = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(|conn| queries::list_audio_episodes(conn))
            .await,
    )?;

    let complete_episodes: Vec<_> = episodes
        .into_iter()
        .filter(|e| e.status == "complete")
        .collect();

    let base_url = std::env::var("NOTE_BASE_URL").unwrap_or_else(|_| {
        let port = std::env::var("NOTE_PORT").unwrap_or_else(|_| "8999".to_string());
        format!("http://localhost:{port}")
    });

    let xml = build_podcast_feed(&base_url, &complete_episodes);

    Ok((
        [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
        xml,
    ))
}

fn build_podcast_feed(base_url: &str, episodes: &[queries::AudioEpisode]) -> String {
    use rss::extension::itunes::{
        ITunesCategoryBuilder, ITunesChannelExtensionBuilder, ITunesItemExtensionBuilder,
        ITunesOwnerBuilder,
    };
    use rss::{ChannelBuilder, EnclosureBuilder, GuidBuilder, ItemBuilder};

    let itunes_channel = ITunesChannelExtensionBuilder::default()
        .author(Some("NixonNote".to_string()))
        .subtitle(Some("Your notes as a podcast".to_string()))
        .summary(Some(
            "Auto-generated podcast from NixonNote entries".to_string(),
        ))
        .explicit(Some("No".to_string()))
        .r#type(Some("episodic".to_string()))
        .owner(Some(
            ITunesOwnerBuilder::default()
                .name(Some("NixonNote".to_string()))
                .email(Some("podcast@nixonnote.local".to_string()))
                .build(),
        ))
        .categories(vec![
            ITunesCategoryBuilder::default().text("Technology").build(),
        ])
        .build();

    let items: Vec<rss::Item> = episodes
        .iter()
        .enumerate()
        .map(|(i, ep)| {
            let itunes_item = ITunesItemExtensionBuilder::default()
                .duration(ep.duration_seconds.map(|d| format_duration(d as u64)))
                .episode(Some((i + 1).to_string()))
                .episode_type(Some("full".to_string()))
                .explicit(Some("No".to_string()))
                .build();

            let file_size = ep.file_size_bytes.unwrap_or(0).to_string();
            let enclosure = EnclosureBuilder::default()
                .url(format!("{base_url}/api/audio/{}/file", ep.id))
                .length(file_size)
                .mime_type("audio/mpeg".to_string())
                .build();

            let guid = GuidBuilder::default()
                .value(format!("nixonnote-episode-{}", ep.id))
                .permalink(false)
                .build();

            ItemBuilder::default()
                .title(Some(ep.title.clone()))
                .description(Some(format!("{} episode", ep.episode_type)))
                .pub_date(Some(ep.created_at.clone()))
                .enclosure(Some(enclosure))
                .guid(Some(guid))
                .itunes_ext(Some(itunes_item))
                .build()
        })
        .collect();

    let channel = ChannelBuilder::default()
        .title("NixonNote Podcast".to_string())
        .link(format!("{base_url}/api/podcast/feed.xml"))
        .description("Your notes, read aloud".to_string())
        .language(Some("en".to_string()))
        .items(items)
        .itunes_ext(Some(itunes_channel))
        .build();

    channel.to_string()
}

fn format_duration(total_secs: u64) -> String {
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn derive_episode_title(episode_type: &str, notes: &[queries::Note]) -> String {
    match episode_type {
        "digest" => {
            let today = chrono_today();
            format!("{today} Daily Digest")
        }
        "batch" => {
            let first_title = notes
                .first()
                .and_then(note_title_or_content_title)
                .unwrap_or_else(|| "Notes".to_string());
            format!("{} + {} more", first_title, notes.len().saturating_sub(1))
        }
        _ => notes
            .first()
            .and_then(note_title_or_content_title)
            .unwrap_or_else(|| "Untitled Note".to_string()),
    }
}

fn note_title_or_content_title(note: &queries::Note) -> Option<String> {
    note.title
        .as_deref()
        .and_then(clean_title)
        .or_else(|| title_from_content(&note.content))
}

fn clean_title(title: &str) -> Option<String> {
    let title = title.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn title_from_content(content: &str) -> Option<String> {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "[Voice memo - transcribing...]")?;

    let line = line
        .trim_start_matches('#')
        .trim_start_matches(['-', '*', '•'])
        .trim();

    let sentence_end = line
        .char_indices()
        .find_map(|(idx, ch)| matches!(ch, '.' | '!' | '?').then_some(idx));
    let candidate = match sentence_end {
        Some(idx) => &line[..idx],
        None => line,
    }
    .trim_matches([':', '-', '—', ' ']);

    let title = truncate_title_words(candidate, 10);
    clean_title(&title)
}

fn truncate_title_words(text: &str, max_words: usize) -> String {
    text.split_whitespace()
        .take(max_words)
        .collect::<Vec<_>>()
        .join(" ")
}

fn chrono_today() -> String {
    // Simple date formatting without chrono dependency
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = now / 86400;
    // Approximate — good enough for display titles
    let year = 1970 + days / 365;
    let remaining_days = days % 365;
    let month = remaining_days / 30 + 1;
    let day = remaining_days % 30 + 1;
    format!("{year}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(title: Option<&str>, content: &str) -> queries::Note {
        queries::Note {
            id: 1,
            content: content.to_string(),
            content_type: "text".to_string(),
            source_type: "web".to_string(),
            source_url: None,
            title: title.map(str::to_string),
            summary: None,
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: "2026-01-01 00:00:00".to_string(),
            tags: vec![],
        }
    }

    #[test]
    fn derive_episode_title_prefers_existing_note_title() {
        let notes = vec![note(Some("Existing AI Title"), "Raw note content")];

        assert_eq!(derive_episode_title("single", &notes), "Existing AI Title");
    }

    #[test]
    fn derive_episode_title_falls_back_to_note_content() {
        let notes = vec![note(
            None,
            "Quarterly EBITDA update: margin expansion and cash flow notes. More detail here.",
        )];

        assert_eq!(
            derive_episode_title("single", &notes),
            "Quarterly EBITDA update: margin expansion and cash flow notes"
        );
    }

    #[test]
    fn derive_episode_title_treats_blank_note_title_as_missing() {
        let notes = vec![note(
            Some("   "),
            "# Product launch checklist\n- Draft copy",
        )];

        assert_eq!(
            derive_episode_title("single", &notes),
            "Product launch checklist"
        );
    }

    #[test]
    fn derive_batch_episode_title_uses_content_fallback_for_first_note() {
        let notes = vec![
            note(None, "First useful note. Body"),
            note(Some("Second"), "Second body"),
        ];

        assert_eq!(
            derive_episode_title("batch", &notes),
            "First useful note + 1 more"
        );
    }

    #[test]
    fn clean_title_rejects_blank_request_titles() {
        assert_eq!(clean_title("  \t  "), None);
        assert_eq!(
            clean_title("  Custom Episode  "),
            Some("Custom Episode".to_string())
        );
    }
}

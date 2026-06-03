use axum::{
    Json,
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
};
use tracing::{info, warn};

use crate::AppState;
use crate::db::queries;
use crate::routes::notes::{AppError, flatten_interact};

// simple_transcribe_rs will auto-download models to ~/.cache/whisper/

// Whisper configuration via environment variables:
// - WHISPER_MODEL_DIR: Directory to store/download models (default: ~/.cache/whisper)
// - WHISPER_MODEL_SIZE: Model size - tiny, base, small, medium, large (default: medium)

/// POST /api/voice
/// Accepts audio file, transcribes with local Whisper, saves as note
pub async fn transcribe_voice(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    // Extract audio file from multipart
    let mut audio_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read multipart field: {e}"))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "file" || field_name == "audio" {
            filename = field.file_name().map(|s| s.to_string());
            audio_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to read audio bytes: {e}"))?
                    .to_vec(),
            );
        }
    }

    let audio_bytes = audio_data.ok_or_else(|| anyhow::anyhow!("No audio file provided"))?;

    info!(
        size_bytes = audio_bytes.len(),
        filename = ?filename,
        "Received audio for transcription"
    );

    // Transcribe locally with Whisper
    let (transcription, audio_saved) =
        match transcribe_with_whisper(&audio_bytes, filename.as_deref()).await {
            Ok(text) => (text, false),
            Err(e) => {
                warn!(error = ?e, "Local Whisper transcription failed, will retry in background");
                // Graceful degradation: save note with pending status
                ("[Voice memo - transcribing...]".to_string(), true)
            }
        };

    // Save as note
    let pool = state.pool.clone();
    let content = transcription.clone();
    let id = flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                queries::insert_note(
                    conn,
                    &content,
                    "voice_transcript",
                    "voice",
                    None, // source_url
                )
            })
            .await,
    )?;

    // If transcription failed, save audio for retry
    if audio_saved && let Err(e) = save_audio_for_retry(id, &audio_bytes).await {
        warn!(note_id = id, error = ?e, "Failed to save audio for retry");
    }

    let note = flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_note(conn, id))
            .await,
    )?;

    // Enqueue for background processing (embedding + auto-tagging)
    if let Err(e) = state.background.enqueue(id).await {
        warn!(note_id = id, error = ?e, "Failed to enqueue note for processing");
    }

    info!(note_id = id, "Voice note created and enqueued");

    Ok((StatusCode::CREATED, Json(note)))
}

fn audio_pending_dir() -> std::path::PathBuf {
    let db_path = std::env::var("NOTE_DB").unwrap_or_else(|_| "note.db".to_string());
    let parent = std::path::Path::new(&db_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    parent.join("audio_pending")
}

async fn save_audio_for_retry(note_id: i64, audio_bytes: &[u8]) -> anyhow::Result<()> {
    use tokio::fs;

    let audio_dir = audio_pending_dir();
    fs::create_dir_all(&audio_dir).await?;

    // Save audio as {note_id}.webm
    let audio_path = audio_dir.join(format!("{}.webm", note_id));
    fs::write(&audio_path, audio_bytes).await?;

    info!(note_id, path = ?audio_path, "Saved audio for retry");
    Ok(())
}

pub async fn retry_pending_transcription(
    _client: &reqwest::Client,
    pool: &deadpool_sqlite::Pool,
    note_id: i64,
) -> anyhow::Result<()> {
    use tokio::fs;

    let audio_path = audio_pending_dir().join(format!("{}.webm", note_id));

    if !audio_path.exists() {
        return Ok(()); // No pending audio, skip
    }

    // Read audio
    let audio_bytes = fs::read(&audio_path).await?;

    // Try transcription
    match transcribe_with_whisper(&audio_bytes, Some("recording.webm")).await {
        Ok(transcription) => {
            let conn = pool.get().await?;
            conn.interact(move |conn| {
                let mut stmt = conn.prepare(
                    "UPDATE notes SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
                )?;
                stmt.execute(rusqlite::params![transcription, note_id])?;
                Ok::<(), rusqlite::Error>(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("Pool interaction error: {e}"))??;

            // Delete audio file
            fs::remove_file(&audio_path).await?;

            info!(note_id, "Successfully transcribed pending audio");
            Ok(())
        }
        Err(e) => {
            // Local Whisper still unavailable, keep audio for next retry
            Err(e)
        }
    }
}

async fn transcribe_with_whisper(
    audio_bytes: &[u8],
    _filename: Option<&str>,
) -> anyhow::Result<String> {
    use simple_transcribe_rs::{model_handler, transcriber};

    // Run transcription in async context (ModelHandler::new is async)
    let audio_bytes = audio_bytes.to_vec();
    // Save audio to temp file and convert WebM to WAV for whisper compatibility
    let tmp_dir = std::env::temp_dir();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp_filename = format!("nixonnote_voice_{}_{}.webm", std::process::id(), nonce);
    let tmp_webm_path = tmp_dir.join(&tmp_filename);

    tokio::fs::write(&tmp_webm_path, &audio_bytes)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to write temp audio file: {}", e))?;

    info!("Audio saved to temp file: {:?}", tmp_webm_path);

    // Convert WebM to WAV using ffmpeg (whisper requires 16kHz mono WAV)
    let tmp_wav_path = tmp_dir.join(format!(
        "nixonnote_voice_{}_{}.wav",
        std::process::id(),
        nonce
    ));

    let ffmpeg_result = tokio::process::Command::new("/opt/homebrew/bin/ffmpeg")
        .arg("-i")
        .arg(&tmp_webm_path)
        .arg("-ar")
        .arg("16000") // 16kHz sample rate
        .arg("-ac")
        .arg("1") // mono
        .arg("-f")
        .arg("wav")
        .arg(&tmp_wav_path)
        .arg("-y") // overwrite
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run ffmpeg: {}. Is ffmpeg installed?", e))?;

    if !ffmpeg_result.status.success() {
        let stderr = String::from_utf8_lossy(&ffmpeg_result.stderr);
        anyhow::bail!("ffmpeg conversion failed: {}", stderr);
    }

    info!("Converted WebM to WAV: {:?}", tmp_wav_path);

    // Use WAV path for transcription
    let tmp_path = tmp_wav_path;

    // Get model storage directory from env or use default
    // WHISPER_MODEL_DIR can be set in com.scott.note.plist
    let model_dir = std::env::var("WHISPER_MODEL_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/.cache/whisper", home)
    });

    info!("Using whisper model directory: {}", model_dir);

    // Initialize model handler (auto-downloads model if needed)
    // Model size: "tiny", "base", "small", "medium", or "large"
    // Using "medium" for high accuracy (~1.5GB)
    let model_size = std::env::var("WHISPER_MODEL_SIZE").unwrap_or_else(|_| "medium".to_string());

    let model_handler = model_handler::ModelHandler::new(&model_size, &model_dir).await;

    info!("Model handler initialized with {} model", model_size);

    // Create transcriber and run transcription (CPU-intensive)
    let trans = transcriber::Transcriber::new(model_handler);

    // Clone path for cleanup after transcription
    let tmp_path_for_cleanup = tmp_path.clone();

    let result = tokio::task::spawn_blocking(move || {
        trans
            .transcribe(&tmp_path.to_string_lossy(), None)
            .map_err(|e| anyhow::anyhow!("Transcription failed: {}", e))
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))??;

    info!("Transcription complete");

    // Extract text from result
    let text = result.get_text().to_string();

    // Clean up temp files
    tokio::fs::remove_file(&tmp_path_for_cleanup).await.ok();
    tokio::fs::remove_file(&tmp_webm_path).await.ok();

    Ok(text)
}

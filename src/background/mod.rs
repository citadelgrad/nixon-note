pub mod auto_org;
pub mod embed;
pub mod tts;

use anyhow::Result;
use deadpool_sqlite::Pool;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

#[derive(Debug)]
pub enum BackgroundJob {
    ProcessNote(i64),
    GenerateAudio { episode_id: i64 },
}

#[derive(Clone)]
pub struct BackgroundProcessor {
    sender: mpsc::Sender<BackgroundJob>,
}

impl BackgroundProcessor {
    pub fn new(pool: Pool, client: reqwest::Client) -> Self {
        let (tx, mut rx) = mpsc::channel::<BackgroundJob>(100);

        // Spawn background task
        tokio::spawn(async move {
            info!("Background processor started");

            while let Some(job) = rx.recv().await {
                match job {
                    BackgroundJob::ProcessNote(note_id) => {
                        if let Err(e) = process_note(&client, &pool, note_id).await {
                            error!(note_id, error = ?e, "Failed to process note");
                        }
                    }
                    BackgroundJob::GenerateAudio { episode_id } => {
                        if let Err(e) =
                            tts::generate_episode_audio(&client, &pool, episode_id).await
                        {
                            error!(episode_id, error = ?e, "Failed to generate audio");
                            if let Err(e2) =
                                tts::mark_episode_failed(&pool, episode_id, &e.to_string()).await
                            {
                                error!(episode_id, error = ?e2, "Failed to mark episode as failed");
                            }
                        }
                    }
                }
            }

            info!("Background processor stopped");
        });

        Self { sender: tx }
    }

    pub async fn enqueue(&self, note_id: i64) -> Result<()> {
        self.sender
            .send(BackgroundJob::ProcessNote(note_id))
            .await?;
        Ok(())
    }

    pub async fn enqueue_audio(&self, episode_id: i64) -> Result<()> {
        self.sender
            .send(BackgroundJob::GenerateAudio { episode_id })
            .await?;
        Ok(())
    }

    /// On startup, re-derive the queue from unprocessed notes
    pub async fn rederive_queue(&self, pool: &Pool) -> Result<()> {
        let conn = pool.get().await?;
        let needing_embedding = conn
            .interact(|conn| crate::db::queries::notes_needing_embedding(conn, 1000))
            .await
            .map_err(|e| anyhow::anyhow!("Pool interaction error: {e}"))??;

        info!(count = needing_embedding.len(), "Re-deriving queue");

        for note_id in needing_embedding {
            self.enqueue(note_id).await?;
        }

        Ok(())
    }
}

async fn process_note(client: &reqwest::Client, pool: &Pool, note_id: i64) -> Result<()> {
    info!(note_id, "Processing note");

    // Step 0 - Retry pending transcription if exists
    if let Err(e) = crate::routes::voice::retry_pending_transcription(client, pool, note_id).await {
        warn!(
            note_id,
            error = ?e,
            "Failed to retry transcription (Osaurus may be unavailable)"
        );
        // Continue to other processing steps
    }

    // Step 1 - Embed (Ollama)
    if let Err(e) = embed::embed_note(client, pool, note_id).await {
        warn!(note_id, error = ?e, "Failed to embed note");
        // Continue to auto-tagging even if embedding fails
    }

    // Step 2 - Auto-org (Claude)
    if let Err(e) = auto_org::auto_org_note(client, pool, note_id).await {
        warn!(
            note_id,
            error = ?e,
            "Failed to auto-organize note (Claude API may be unavailable)"
        );
        // Note is still captured and searchable
    }

    Ok(())
}

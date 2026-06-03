use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::db::queries;

#[derive(Deserialize)]
pub struct CreateNote {
    pub content: String,
    #[serde(default = "default_source")]
    pub source_type: String,
    pub source_url: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
pub struct BatchCreateRequest {
    pub notes: Vec<CreateNote>,
}

#[derive(Serialize)]
pub struct BatchCreateResponse {
    pub note_ids: Vec<i64>,
    pub failed_count: usize,
}

#[derive(Deserialize)]
pub struct UpdateNote {
    pub content: String,
}

fn default_source() -> String {
    "web".to_string()
}

#[derive(Deserialize)]
pub struct NotesQuery {
    pub q: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub exclude_tag: Option<String>,
}

fn default_limit() -> i64 {
    20
}

#[derive(Serialize)]
pub struct NotesResponse {
    pub notes: Vec<queries::Note>,
    pub query: Option<String>,
    pub count: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Helper to flatten deadpool interact results into anyhow::Result
pub fn flatten_interact<T>(
    result: Result<anyhow::Result<T>, deadpool_sqlite::InteractError>,
) -> anyhow::Result<T> {
    match result {
        Ok(inner) => inner,
        Err(e) => Err(anyhow::anyhow!("Database interaction error: {e}")),
    }
}

/// Helper to populate tags for notes
async fn populate_tags(
    pool: &deadpool_sqlite::Pool,
    notes: Vec<queries::Note>,
) -> anyhow::Result<Vec<queries::Note>> {
    if notes.is_empty() {
        return Ok(notes);
    }

    flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::populate_note_tags(conn, notes))
            .await,
    )
}

/// Helper to add tags to a note
pub async fn add_tags_to_note(
    pool: &deadpool_sqlite::Pool,
    note_id: i64,
    tags: Vec<String>,
) -> anyhow::Result<()> {
    if tags.is_empty() {
        return Ok(());
    }

    flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                for tag_name in tags {
                    let tag_id = queries::upsert_tag(conn, &tag_name)?;
                    queries::add_note_tag(conn, note_id, tag_id, 1.0, "manual")?;
                }
                Ok(())
            })
            .await,
    )
}

/// Helper to filter notes by excluding a specific tag
fn filter_notes_by_excluded_tag(
    notes: Vec<queries::Note>,
    exclude_tag: &str,
) -> Vec<queries::Note> {
    notes
        .into_iter()
        .filter(|note| !note.tags.iter().any(|t| t == exclude_tag))
        .collect()
}

pub async fn create_note(
    State(state): State<AppState>,
    Json(body): Json<CreateNote>,
) -> Result<impl IntoResponse, AppError> {
    if body.content.trim().is_empty() {
        return Err(AppError::BadRequest("Content cannot be empty".into()));
    }

    let CreateNote {
        content,
        source_type: source,
        source_url,
        tags,
    } = body;
    let id = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                queries::insert_note(conn, &content, "text", &source, source_url.as_deref())
            })
            .await,
    )?;

    add_tags_to_note(&state.pool, id, tags).await?;

    let note = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_note(conn, id))
            .await,
    )?;

    // Enqueue for background processing (embed + auto-tag)
    if let Err(e) = state.background.enqueue(id).await {
        // Log but don't fail the request - note is captured
        tracing::warn!(note_id = id, error = ?e, "Failed to enqueue note for processing");
    }

    Ok((StatusCode::CREATED, Json(note)))
}

pub async fn create_notes_batch(
    State(state): State<AppState>,
    Json(body): Json<BatchCreateRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.notes.is_empty() {
        return Err(AppError::BadRequest("Empty notes array".into()));
    }

    if body.notes.len() > 1000 {
        return Err(AppError::BadRequest("Batch size exceeds 1000".into()));
    }

    let notes = body.notes;
    let (note_ids, failed_count) = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                let tx = conn.transaction()?;
                let mut note_ids = Vec::new();
                let mut failed = 0;

                for note in notes {
                    if note.content.trim().is_empty() {
                        failed += 1;
                        continue;
                    }

                    match queries::insert_note(
                        &tx,
                        &note.content,
                        "text",
                        &note.source_type,
                        note.source_url.as_deref(),
                    ) {
                        Ok(id) => {
                            // Add tags within the same transaction
                            for tag_name in &note.tags {
                                if let Ok(tag_id) = queries::upsert_tag(&tx, tag_name) {
                                    let _ = queries::add_note_tag(&tx, id, tag_id, 1.0, "manual");
                                }
                            }
                            note_ids.push(id);
                        }
                        Err(e) => {
                            tracing::warn!(error = ?e, "Failed to insert note in batch");
                            failed += 1;
                        }
                    }
                }

                tx.commit()?;
                Ok((note_ids, failed))
            })
            .await,
    )?;

    // Enqueue all for background processing
    for note_id in &note_ids {
        if let Err(e) = state.background.enqueue(*note_id).await {
            tracing::warn!(note_id = note_id, error = ?e, "Failed to enqueue note for processing");
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(BatchCreateResponse {
            note_ids,
            failed_count,
        }),
    ))
}

pub async fn get_notes(
    State(state): State<AppState>,
    Query(params): Query<NotesQuery>,
) -> Result<Json<NotesResponse>, AppError> {
    let limit = params.limit;
    let offset = params.offset;
    let exclude_tag = params.exclude_tag.clone();

    if let Some(ref q) = params.q {
        if q.trim().is_empty() {
            let mut notes = flatten_interact(
                state
                    .pool
                    .get()
                    .await
                    .map_err(anyhow::Error::from)?
                    .interact(move |conn| queries::list_notes(conn, limit, offset))
                    .await,
            )?;
            notes = populate_tags(&state.pool, notes).await?;

            // Apply tag filter if specified
            if let Some(ref tag) = exclude_tag {
                notes = filter_notes_by_excluded_tag(notes, tag);
            }

            let count = notes.len() as i64;
            return Ok(Json(NotesResponse {
                notes,
                query: None,
                count,
                limit,
                offset,
            }));
        }
        // Hybrid search: combine FTS and vector similarity
        let mut notes = hybrid_search(&state, q, limit).await?;
        notes = populate_tags(&state.pool, notes).await?;

        // Apply tag filter if specified
        if let Some(ref tag) = exclude_tag {
            notes = filter_notes_by_excluded_tag(notes, tag);
        }

        let count = notes.len() as i64;
        Ok(Json(NotesResponse {
            notes,
            query: params.q,
            count,
            limit,
            offset,
        }))
    } else {
        let mut notes = flatten_interact(
            state
                .pool
                .get()
                .await
                .map_err(anyhow::Error::from)?
                .interact(move |conn| queries::list_notes(conn, limit, offset))
                .await,
        )?;
        notes = populate_tags(&state.pool, notes).await?;

        // Apply tag filter if specified
        if let Some(ref tag) = exclude_tag {
            notes = filter_notes_by_excluded_tag(notes, tag);
        }

        let total = flatten_interact(
            state
                .pool
                .get()
                .await
                .map_err(anyhow::Error::from)?
                .interact(|conn| queries::count_notes(conn))
                .await,
        )?;
        Ok(Json(NotesResponse {
            notes,
            query: None,
            count: total,
            limit,
            offset,
        }))
    }
}

pub async fn get_note(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<queries::Note>, AppError> {
    let note = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_note(conn, id))
            .await,
    )?;
    Ok(Json(note))
}

pub async fn update_note(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateNote>,
) -> Result<Json<queries::Note>, AppError> {
    if body.content.trim().is_empty() {
        return Err(AppError::BadRequest("Content cannot be empty".into()));
    }

    let content = body.content;
    flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::update_note(conn, id, &content))
            .await,
    )?;

    let note = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_note(conn, id))
            .await,
    )?;

    // Re-enqueue for background processing (re-embed + re-tag with new content)
    if let Err(e) = state.background.enqueue(id).await {
        tracing::warn!(note_id = id, error = ?e, "Failed to enqueue updated note for processing");
    }

    Ok(Json(note))
}

pub async fn delete_note(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::delete_note(conn, id))
            .await,
    )?;

    Ok(StatusCode::NO_CONTENT)
}

/// Hybrid search: combines FTS5 text search and vector similarity search
async fn hybrid_search(
    state: &AppState,
    query_text: &str,
    limit: i64,
) -> Result<Vec<queries::Note>, AppError> {
    use std::collections::HashSet;
    use tracing::warn;

    // 1. Run FTS search (graceful fallback if query causes FTS error)
    let query_clone = query_text.to_string();
    let fts_ids: HashSet<i64> = match flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::search_fts(conn, &query_clone, limit))
            .await,
    ) {
        Ok(fts_notes) => fts_notes.iter().map(|n| n.id).collect(),
        Err(e) => {
            warn!(error = ?e, "FTS search failed, falling back to vector search only");
            HashSet::new()
        }
    };

    // 2. Generate embedding for query and run vector search
    let vector_ids: HashSet<i64> =
        match crate::background::embed::generate_embedding(&state.client, query_text).await {
            Ok(embedding) => {
                // Run vector similarity search
                let similar = flatten_interact(
                    state
                        .pool
                        .get()
                        .await
                        .map_err(anyhow::Error::from)?
                        .interact(move |conn| queries::search_similar(conn, &embedding, limit))
                        .await,
                )?;

                similar
                    .into_iter()
                    .map(|(note_id, _distance)| note_id)
                    .collect()
            }
            Err(e) => {
                // If Ollama is down, log warning and continue with FTS results only
                warn!(error = ?e, "Vector search unavailable, using FTS results only");
                HashSet::new()
            }
        };

    // 3. Merge: combine FTS IDs and vector IDs, deduplicate
    let mut merged_ids: Vec<i64> = fts_ids.union(&vector_ids).copied().collect();
    merged_ids.sort_unstable();
    merged_ids.reverse(); // Most recent first

    // Limit to requested count
    merged_ids.truncate(limit as usize);

    // 4. Get full Note objects for merged IDs
    if merged_ids.is_empty() {
        return Ok(vec![]);
    }

    let notes = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_notes_by_ids(conn, &merged_ids))
            .await,
    )?;

    Ok(notes)
}

/// Random notes endpoint for rediscovery widget
pub async fn random_notes(
    State(state): State<AppState>,
    Query(params): Query<NotesQuery>,
) -> Result<Json<NotesResponse>, AppError> {
    let limit = params.limit.min(5);
    let exclude_tag = params.exclude_tag.clone();

    let mut notes = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::random_notes(conn, limit, exclude_tag.as_deref()))
            .await,
    )?;
    notes = populate_tags(&state.pool, notes).await?;

    let count = notes.len() as i64;
    Ok(Json(NotesResponse {
        notes,
        query: Some("random".to_string()),
        count,
        limit,
        offset: 0,
    }))
}

/// Search for relevant notes using vector similarity with FTS fallback
pub async fn search_relevant_notes(
    state: &AppState,
    query: &str,
    limit: i64,
) -> Result<Vec<queries::Note>, AppError> {
    let embedding = match crate::background::embed::generate_embedding(&state.client, query).await {
        Ok(emb) => emb,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to generate embedding, falling back to FTS");
            let query_clone = query.to_string();
            let notes = flatten_interact(
                state
                    .pool
                    .get()
                    .await
                    .map_err(anyhow::Error::from)?
                    .interact(move |conn| queries::search_fts(conn, &query_clone, limit))
                    .await,
            )?;
            return Ok(notes);
        }
    };

    let similar = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::search_similar(conn, &embedding, limit))
            .await,
    )?;

    let note_ids: Vec<i64> = similar.into_iter().map(|(id, _)| id).collect();

    if note_ids.is_empty() {
        return Ok(vec![]);
    }

    flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_notes_by_ids(conn, &note_ids))
            .await,
    )
    .map_err(AppError::from)
}

// Error type that converts to HTTP responses
#[derive(Debug)]
pub enum AppError {
    Internal(anyhow::Error),
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            AppError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}

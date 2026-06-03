use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::db::queries;
use crate::routes::notes::{AppError, flatten_interact};

#[derive(Serialize)]
pub struct TagsResponse {
    pub tags: Vec<queries::TagWithCount>,
}

#[derive(Deserialize)]
pub struct TagFilterQuery {
    pub tag: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

pub async fn list_tags(State(state): State<AppState>) -> Result<Json<TagsResponse>, AppError> {
    let tags = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(|conn| queries::list_tags(conn))
            .await,
    )?;

    Ok(Json(TagsResponse { tags }))
}

pub async fn notes_by_tag(
    State(state): State<AppState>,
    Query(params): Query<TagFilterQuery>,
) -> Result<Json<crate::routes::notes::NotesResponse>, AppError> {
    let tag = params.tag;
    let limit = params.limit;
    let query_str = format!("tag:{}", tag);

    let notes = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::notes_by_tag(conn, &tag, limit))
            .await,
    )?;

    let count = notes.len() as i64;

    Ok(Json(crate::routes::notes::NotesResponse {
        notes,
        query: Some(query_str),
        count,
        limit,
        offset: 0,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use axum::extract::{Query, State};

    async fn setup_test_db() -> (AppState, String) {
        let path = format!(
            "/tmp/test_tags_{}_{:?}.db",
            std::process::id(),
            std::thread::current().id()
        );

        let conn = db::open_and_migrate(&path).unwrap();

        // Insert test notes with tags
        let note1_id =
            db::queries::insert_note(&conn, "Rust is awesome", "text", "test", None).unwrap();
        let note2_id =
            db::queries::insert_note(&conn, "SQLite vector search", "text", "test", None).unwrap();
        let note3_id =
            db::queries::insert_note(&conn, "More Rust content", "text", "test", None).unwrap();

        // Add tags
        let rust_tag = db::queries::upsert_tag(&conn, "rust").unwrap();
        let sqlite_tag = db::queries::upsert_tag(&conn, "sqlite").unwrap();
        let db_tag = db::queries::upsert_tag(&conn, "database").unwrap();

        db::queries::add_note_tag(&conn, note1_id, rust_tag, 1.0, "test").unwrap();
        db::queries::add_note_tag(&conn, note2_id, sqlite_tag, 1.0, "test").unwrap();
        db::queries::add_note_tag(&conn, note2_id, db_tag, 1.0, "test").unwrap();
        db::queries::add_note_tag(&conn, note3_id, rust_tag, 1.0, "test").unwrap();

        drop(conn);

        let pool = db::create_pool(&path).unwrap();
        let client = reqwest::Client::new();
        let background = crate::background::BackgroundProcessor::new(pool.clone(), client.clone());

        let state = AppState {
            pool,
            background,
            client,
        };

        (state, path)
    }

    #[tokio::test]
    async fn test_list_tags() {
        let (state, db_path) = setup_test_db().await;

        let result = list_tags(State(state)).await;
        assert!(result.is_ok());

        let tags = result.unwrap().0.tags;

        // Should have 3 tags
        assert_eq!(tags.len(), 3);

        // Tags should be ordered by count DESC
        // rust: 2 notes, sqlite: 1 note, database: 1 note
        assert_eq!(tags[0].name, "rust");
        assert_eq!(tags[0].count, 2);

        // Cleanup
        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn test_notes_by_tag() {
        let (state, db_path) = setup_test_db().await;

        let query = TagFilterQuery {
            tag: "rust".to_string(),
            limit: 20,
        };

        let result = notes_by_tag(State(state), Query(query)).await;
        assert!(result.is_ok());

        let notes_response = result.unwrap().0;

        // Should have 2 notes tagged with "rust"
        assert_eq!(notes_response.count, 2);
        assert_eq!(notes_response.notes.len(), 2);

        // Query should indicate tag filter
        assert_eq!(notes_response.query, Some("tag:rust".to_string()));

        // Cleanup
        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn test_notes_by_tag_not_found() {
        let (state, db_path) = setup_test_db().await;

        let query = TagFilterQuery {
            tag: "nonexistent".to_string(),
            limit: 20,
        };

        let result = notes_by_tag(State(state), Query(query)).await;
        assert!(result.is_ok());

        let notes_response = result.unwrap().0;

        // Should return empty results
        assert_eq!(notes_response.count, 0);
        assert_eq!(notes_response.notes.len(), 0);

        // Cleanup
        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn test_notes_by_tag_respects_limit() {
        let (state, db_path) = setup_test_db().await;

        let query = TagFilterQuery {
            tag: "rust".to_string(),
            limit: 1, // Limit to 1 result
        };

        let result = notes_by_tag(State(state), Query(query)).await;
        assert!(result.is_ok());

        let notes_response = result.unwrap().0;

        // Should respect limit
        assert_eq!(notes_response.notes.len(), 1);
        assert_eq!(notes_response.limit, 1);

        // Cleanup
        std::fs::remove_file(&db_path).ok();
    }
}

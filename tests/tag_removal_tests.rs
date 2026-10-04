use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use note::db;
use serde_json::json;
use tower::util::ServiceExt;

mod common;

#[test]
fn latest_schema_has_no_tag_tables() {
    let conn = db::open_and_migrate(":memory:").unwrap();

    for table in ["tags", "note_tags"] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "legacy tag table {table} still exists");
    }
}

#[tokio::test]
async fn notes_api_omits_tags_and_filters_imports_by_source() {
    let app = common::setup_test_app().await;

    for (content, source_type) in [("ordinary", "web"), ("package", "homebrew")] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/notes")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer test-token")
                    .body(Body::from(
                        json!({"content": content, "source_type": source_type}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/notes?limit=1&offset=0&hide_imported=true")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let notes = payload["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(payload["count"], 1);
    assert_eq!(notes[0]["source_type"], "web");
    assert!(notes[0].get("tags").is_none());
}

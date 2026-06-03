use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::util::ServiceExt;

mod common;
use common::setup_test_app;

/// Test that bookmarks import requires authentication
#[tokio::test]
async fn test_bookmarks_import_requires_auth() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/bookmarks")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"bookmarks": []}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test that bookmarks import accepts valid data
#[tokio::test]
async fn test_bookmarks_import_success() {
    let app = setup_test_app().await;

    let bookmarks = json!({
        "bookmarks": [
            {
                "title": "Rust Programming Language",
                "url": "https://www.rust-lang.org",
                "notes": "Official Rust website",
                "tags": ["programming", "rust"]
            },
            {
                "title": "GitHub",
                "url": "https://github.com"
            }
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/bookmarks")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(bookmarks.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Parse response
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(response["imported"].as_u64().unwrap(), 2);
    assert_eq!(response["failed"].as_u64().unwrap(), 0);
    assert!(response["note_ids"].is_array());
    assert_eq!(response["note_ids"].as_array().unwrap().len(), 2);
}

/// Test that bookmarks import handles empty array
#[tokio::test]
async fn test_bookmarks_import_empty_array() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/bookmarks")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"bookmarks": []}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(response["imported"].as_u64().unwrap(), 0);
    assert_eq!(response["failed"].as_u64().unwrap(), 0);
}

/// Test that YouTube ingestion requires authentication
#[tokio::test]
async fn test_youtube_ingest_requires_auth() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/youtube")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test that YouTube ingestion rejects invalid URLs
#[tokio::test]
async fn test_youtube_ingest_invalid_url() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/youtube")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"url": "https://not-youtube.com/video"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Could not extract video ID"));
}

/// Test that YouTube ingest degrades gracefully when GEMINI_API_KEY is not set
/// (should still create a note, just without summary)
#[tokio::test]
#[ignore] // Requires network access to YouTube
async fn test_youtube_ingest_without_gemini_key() {
    unsafe {
        std::env::remove_var("GEMINI_API_KEY");
    }

    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/youtube")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(
                    r#"{"url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should succeed even without Gemini — creates note without summary
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(resp["note_id"].as_i64().unwrap() > 0);
}

/// Test that URL ingest requires authentication
#[tokio::test]
async fn test_url_ingest_requires_auth() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/url")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url": "https://example.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test that URL ingest rejects invalid URL format
#[tokio::test]
async fn test_url_ingest_invalid_url_format() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/url")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"url": "not-a-valid-url"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("http"));
}

/// Test that URL ingest rejects malformed JSON
#[tokio::test]
async fn test_url_ingest_malformed_json() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/url")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"not_url": "missing"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 422 for deserialization failures
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got: {}",
        response.status()
    );
}

/// Test that YouTube ingest rejects malformed JSON
#[tokio::test]
async fn test_youtube_ingest_malformed_json() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/youtube")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"not_url": "missing"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got: {}",
        response.status()
    );
}

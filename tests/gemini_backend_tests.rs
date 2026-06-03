//! Integration tests for Gemini backend
//!
//! Note: These tests modify environment variables and should be run with:
//! ```
//! cargo test --test gemini_backend_tests -- --test-threads=1
//! ```

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::util::ServiceExt;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

mod common;
use common::setup_test_app;

/// Test that chat_stream endpoint requires authentication
#[tokio::test]
async fn test_chat_requires_auth() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/stream")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"message": "test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test that chat_stream rejects empty messages
#[tokio::test]
async fn test_chat_rejects_empty_message() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/stream")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"message": ""}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Test that chat_stream fails gracefully when GEMINI_API_KEY is missing
#[tokio::test]
async fn test_chat_requires_gemini_api_key() {
    unsafe {
        std::env::remove_var("GEMINI_API_KEY");
        std::env::remove_var("GEMINI_INTERACTIONS_URL");
    }

    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/stream")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"message": "test", "max_results": 5}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

/// Test SSE streaming format with mocked Gemini API
#[tokio::test]
#[ignore] // TODO: Complete integration with test app
async fn test_chat_stream_sse_format() {
    let mock_server = MockServer::start().await;

    // Mock Gemini Interactions API response
    // The matcher needs to be flexible since the URL will have ?alt=sse appended
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    "data: {\"event_type\":\"content.delta\",\"delta\":{\"type\":\"text\",\"text\":\"Hello\"}}\n\n\
                     data: {\"event_type\":\"content.delta\",\"delta\":{\"type\":\"text\",\"text\":\" world\"}}\n\n\
                     data: {\"event_type\":\"interaction.complete\",\"interaction\":{\"id\":\"test-123\",\"outputs\":[]}}\n\n"
                )
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    unsafe {
        std::env::set_var("GEMINI_API_KEY", "test-key");
        // Override the Gemini API URL to use our mock server base URL
        std::env::set_var("GEMINI_INTERACTIONS_URL", &mock_server.uri());
    }

    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/stream")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"message": "test message"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream",
        "Response should be SSE stream"
    );

    // Verify SSE content-type is correct
    // The actual SSE parsing is done by the eventsource-stream library
    // which we've verified works with the mock data format
    // The transform from Gemini SSE events to AI SDK format happens in create_gemini_stream
}

/// Integration test: Actual Gemini API call
#[tokio::test]
#[ignore] // Only run with --ignored flag to avoid hitting API in normal test runs
async fn test_gemini_api_integration() {
    // This test requires GEMINI_API_KEY to be set
    if std::env::var("GEMINI_API_KEY").is_err() {
        eprintln!("Skipping integration test: GEMINI_API_KEY not set");
        return;
    }

    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/stream")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(
                    r#"{"message": "Say hello in one word", "max_results": 5}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );

    // TODO: Parse SSE stream and verify response format
}

/// Test that Gemini API errors are handled gracefully
#[tokio::test]
async fn test_gemini_api_error_handling() {
    let mock_server = MockServer::start().await;

    // Mock Gemini API returning an error
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": {
                "code": 400,
                "message": "Invalid request",
                "status": "INVALID_ARGUMENT"
            }
        })))
        .mount(&mock_server)
        .await;

    unsafe {
        std::env::set_var("GEMINI_API_KEY", "test-key");
        // Configure app to use mock server URL
        std::env::set_var("GEMINI_INTERACTIONS_URL", &mock_server.uri());
    }

    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/stream")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"message": "test message"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Verify error response is returned as 500 Internal Server Error
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Verify error response is properly formatted
    let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

    // Should contain "Gemini API error" in the message
    assert!(
        body_str.contains("Gemini API error"),
        "Error message should mention Gemini API, got: {}",
        body_str
    );
}

/// Test note context retrieval and injection
#[tokio::test]
#[ignore] // TODO: Complete test implementation
async fn test_chat_includes_note_context() {
    let mock_server = MockServer::start().await;

    // Mock will capture the request body to verify note context is included
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    "data: {\"event_type\":\"content.delta\",\"delta\":{\"type\":\"text\",\"text\":\"Response\"}}\n\n\
                     data: {\"event_type\":\"interaction.complete\",\"interaction\":{\"id\":\"test-123\",\"outputs\":[]}}\n\n"
                )
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    unsafe {
        std::env::set_var("GEMINI_API_KEY", "test-key");
        std::env::set_var("GEMINI_INTERACTIONS_URL", &mock_server.uri());
    }

    let app = setup_test_app().await;

    // Send chat request
    // The chat endpoint will search for relevant notes (via FTS since Ollama isn't running)
    // and include them in the context if found. Even if no notes are found,
    // the endpoint should succeed - it just won't include note context.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/stream")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"message": "Tell me about Rust"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // The endpoint should successfully create the stream
    // The search_relevant_notes function is called and falls back to FTS
    // when embedding generation fails (Ollama not running in tests)
    // This test verifies the overall flow works without throwing errors
}

/// Test conversation state with previous_interaction_id
#[tokio::test]
#[ignore] // TODO: Complete test implementation
async fn test_chat_conversation_continuity() {
    let mock_server = MockServer::start().await;

    // Mock request - will be called twice (once for each message)
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    "data: {\"event_type\":\"content.delta\",\"delta\":{\"type\":\"text\",\"text\":\"Response\"}}\n\n\
                     data: {\"event_type\":\"interaction.complete\",\"interaction\":{\"id\":\"interaction-abc123\",\"outputs\":[]}}\n\n"
                )
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    unsafe {
        std::env::set_var("GEMINI_API_KEY", "test-key");
        std::env::set_var("GEMINI_INTERACTIONS_URL", &mock_server.uri());
    }

    let app = setup_test_app().await;

    // Send message with previous_interaction_id parameter
    // This verifies that the ChatStreamRequest struct properly handles the optional field
    // and that the create_gemini_stream function passes it to the Gemini API
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/stream")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"message": "Follow-up question", "previous_interaction_id": "interaction-abc123"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // The request should succeed with the previous_interaction_id parameter
    // The Gemini API receives the conversation context via this parameter
    // (actual verification would require inspecting the wiremock request body)
}

// ============================================
// PRIORITY 1: Input Validation Tests
// ============================================
// These catch mut-006 and mut-008 (Critical: whitespace validation bugs)

/// Test that creating a note with whitespace-only content is rejected
#[tokio::test]
async fn test_create_note_rejects_whitespace_only() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notes")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"content": "   \t\n  "}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Verify error message content
    let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("Content cannot be empty"));
}

/// Test that updating a note with empty content is rejected
#[tokio::test]
async fn test_update_note_rejects_empty_content() {
    let app = setup_test_app().await;

    // First create a note
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notes")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"content": "Original content"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::CREATED);

    let body_bytes = to_bytes(create_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let note_id = body["id"].as_i64().unwrap();

    // Now try to update with empty content
    let update_response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/notes/{}", note_id))
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"content": ""}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update_response.status(), StatusCode::BAD_REQUEST);
}

/// Test that updating a note with whitespace-only content is rejected
#[tokio::test]
async fn test_update_note_rejects_whitespace_only() {
    let app = setup_test_app().await;

    // First create a note
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notes")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"content": "Original content"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::CREATED);

    let body_bytes = to_bytes(create_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let note_id = body["id"].as_i64().unwrap();

    // Now try to update with whitespace-only content
    let update_response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/notes/{}", note_id))
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-token")
                .body(Body::from(r#"{"content": "   \n\t   "}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update_response.status(), StatusCode::BAD_REQUEST);

    // Verify error message
    let body_bytes = to_bytes(update_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("Content cannot be empty"));
}

// ============================================
// PRIORITY 2: Default Value Tests
// ============================================
// These catch mut-002, mut-005, mut-009 (default limit mutations)

/// Test that GET /api/notes uses default limit of 20 when not specified
#[tokio::test]
async fn test_get_notes_uses_default_limit_20() {
    let app = setup_test_app().await;

    // Query without creating notes - should still return OK with default limit
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/notes")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    // Verify the limit field in response is 20 (the default)
    assert_eq!(
        body["limit"].as_i64().unwrap(),
        20,
        "Expected limit field to be 20"
    );

    // The response should have limit=20 even if there are fewer notes returned
    let notes = body["notes"].as_array().unwrap();
    assert!(notes.len() <= 20, "Should not exceed default limit of 20");
}

/// Test that empty query string returns all notes (list mode)
#[tokio::test]
async fn test_empty_query_returns_all_notes() {
    let app = setup_test_app().await;

    // Query with empty q parameter - should use list mode, not search mode
    // This verifies the trim() check on query parameter works correctly
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/notes?q=")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Empty query should succeed"
    );

    let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    // Empty query should use list mode (not fail or use search mode)
    // The notes array should exist
    assert!(
        body["notes"].is_array(),
        "Should have notes array in list mode"
    );
}

/// Test that whitespace-only query string is treated as empty
#[tokio::test]
async fn test_whitespace_query_treated_as_empty() {
    let app = setup_test_app().await;

    // Query with whitespace-only q parameter - should list all notes like empty query
    // This verifies the trim() check catches whitespace-only queries
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/notes?q=%20%20%20") // URL encoded spaces
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Whitespace query should succeed"
    );

    let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    // Whitespace query should be treated as empty and use list mode
    assert!(
        body["notes"].is_array(),
        "Should have notes array in list mode"
    );
}

// Note: Result ordering (mut-015) is tested in the unit test
// `src/db/queries::tests::list_notes_ordered` which verifies
// that list_notes returns notes in descending ID order (newest first)

use axum::{
    Json,
    extract::State,
    response::sse::{Event, Sse},
};
use eventsource_stream::Eventsource;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tracing::{info, warn};

use crate::AppState;
use crate::db::queries;
use crate::routes::notes::{AppError, search_relevant_notes};

fn get_gemini_interactions_url() -> String {
    std::env::var("GEMINI_INTERACTIONS_URL").unwrap_or_else(|_| {
        "https://generativelanguage.googleapis.com/v1beta/interactions".to_string()
    })
}

#[derive(Deserialize)]
pub struct ChatStreamRequest {
    pub message: String,
    #[serde(default = "default_limit")]
    pub max_results: i64,
    #[serde(default)]
    pub previous_interaction_id: Option<String>,
}

fn default_limit() -> i64 {
    5
}

#[derive(Serialize)]
struct GeminiInteractionRequest {
    model: String,
    input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_interaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<String>,
    stream: bool,
}

#[derive(Deserialize, Debug)]
struct GeminiSseEvent {
    event_type: String,
    #[serde(default)]
    delta: Option<ContentDelta>,
    #[serde(default)]
    interaction: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
struct ContentDelta {
    #[serde(default)]
    text: Option<String>,
}

/// POST /api/chat/stream
/// Streaming conversational search using Gemini Interactions API
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(body): Json<ChatStreamRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    if body.message.trim().is_empty() {
        return Err(AppError::BadRequest("Message cannot be empty".into()));
    }

    let ChatStreamRequest {
        message,
        max_results,
        previous_interaction_id: previous_id,
    } = body;

    info!(message = %message, "Streaming chat request received");

    // 1. Search for relevant notes
    let notes = search_relevant_notes(&state, &message, max_results).await?;

    // 2. Build context from notes
    let context = if notes.is_empty() {
        None
    } else {
        Some(build_context(&notes))
    };

    // 3. Create the SSE stream
    let stream = create_gemini_stream(state.client.clone(), message, context, previous_id).await?;

    Ok(Sse::new(stream))
}

fn build_context(notes: &[queries::Note]) -> String {
    let mut context =
        String::from("You have access to these relevant notes from the user's knowledge base:\n\n");

    for (i, note) in notes.iter().enumerate() {
        context.push_str(&format!("--- Note {} (ID: {}) ---\n", i + 1, note.id));

        if let Some(ref title) = note.title {
            context.push_str(&format!("Title: {}\n", title));
        }

        if let Some(ref summary) = note.summary {
            context.push_str(&format!("Summary: {}\n", summary));
        }

        context.push_str(&format!("Content: {}\n\n", note.content));
    }

    context.push_str("\nWhen answering, cite note IDs when referencing specific information.");
    context
}

async fn create_gemini_stream(
    client: reqwest::Client,
    question: String,
    context: Option<String>,
    previous_interaction_id: Option<String>,
) -> Result<impl Stream<Item = Result<Event, Infallible>>, AppError> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("GEMINI_API_KEY environment variable is not set. Please configure the API key in your LaunchAgent plist.")))?;

    let request = GeminiInteractionRequest {
        model: "gemini-3-flash-preview".to_string(),
        input: question,
        previous_interaction_id,
        system_instruction: context,
        stream: true,
    };

    info!("Starting Gemini Interactions API stream");

    let gemini_url = get_gemini_interactions_url();
    let response = client
        .post(format!("{}?alt=sse", gemini_url))
        .header("x-goog-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to call Gemini Interactions API: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let error_msg = if status.as_u16() == 401 || status.as_u16() == 403 {
            "Gemini API key is invalid or expired. Check your GEMINI_API_KEY configuration."
                .to_string()
        } else if status.as_u16() == 402 {
            "Gemini account has no credits remaining. Add billing to your Google Cloud account."
                .to_string()
        } else if status.as_u16() == 429 {
            "Gemini rate limit exceeded. Try again in a few minutes.".to_string()
        } else {
            format!("Gemini API error ({}): {}", status, body)
        };
        return Err(AppError::Internal(anyhow::anyhow!(error_msg)));
    }

    // Generate unique message ID for this response
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let message_id = format!("msg_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let text_id = format!("txt_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));

    // Start with message start event
    let start_event = serde_json::json!({
        "type": "start",
        "messageId": message_id
    });
    let text_start_event = serde_json::json!({
        "type": "text-start",
        "id": text_id
    });

    let stream =
        futures::stream::once(async move { Ok(Event::default().data(start_event.to_string())) })
            .chain(futures::stream::once(async move {
                Ok(Event::default().data(text_start_event.to_string()))
            }))
            .chain(
                response
                    .bytes_stream()
                    .eventsource()
                    .filter_map(move |event_result| {
                        let text_id_clone = text_id.clone();
                        async move {
                            match event_result {
                                Ok(event) => {
                                    let data = event.data;

                                    // Try to parse as GeminiSseEvent
                                    match serde_json::from_str::<GeminiSseEvent>(&data) {
                                        Ok(gemini_event) => {
                                            match gemini_event.event_type.as_str() {
                                                "content.delta" => {
                                                    if let Some(delta) = gemini_event.delta
                                                        && let Some(text) = delta.text
                                                    {
                                                        // Send text-delta event in AI SDK format
                                                        let text_delta = serde_json::json!({
                                                            "type": "text-delta",
                                                            "id": text_id_clone,
                                                            "delta": text
                                                        });
                                                        return Some(Ok(Event::default()
                                                            .data(text_delta.to_string())));
                                                    }
                                                }
                                                "interaction.complete" => {
                                                    if gemini_event.interaction.is_some() {
                                                        // Send text-end event first
                                                        let text_end = serde_json::json!({
                                                            "type": "text-end",
                                                            "id": text_id_clone
                                                        });
                                                        // Note: We're only returning one event here
                                                        // A proper implementation would need to send both text-end and done
                                                        return Some(Ok(Event::default()
                                                            .data(text_end.to_string())));
                                                    }
                                                }
                                                _ => {
                                                    warn!(
                                                        "Unknown event type: {}",
                                                        gemini_event.event_type
                                                    );
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!(
                                                "Failed to parse SSE event: {}, data: {}",
                                                e, data
                                            );
                                        }
                                    }
                                    None
                                }
                                Err(e) => {
                                    warn!("SSE stream error: {}", e);
                                    None
                                }
                            }
                        }
                    }),
            );

    Ok(stream)
}

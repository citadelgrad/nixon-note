use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::AppState;
use crate::db::queries;
use crate::routes::notes::{AppError, flatten_interact, search_relevant_notes};

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const GEMINI_API_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";
const CLAUDE_MODEL: &str = "claude-sonnet-4-5-20250929";
const GEMINI_MODEL: &str = "gemini-2.5-flash";

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum LlmProvider {
    Claude,
    #[default]
    Gemini,
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default = "default_limit")]
    pub max_results: i64,
    #[serde(default)]
    pub llm: LlmProvider,
}

fn default_limit() -> i64 {
    5
}

#[derive(Serialize)]
pub struct ChatResponse {
    pub answer: String,
    pub sources: Vec<SourceNote>,
}

#[derive(Serialize)]
pub struct SourceNote {
    pub id: i64,
    pub title: Option<String>,
    pub content_preview: String,
}

// Shared message type (used by both Claude and Gemini requests)
#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

// Claude API structures
#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct ClaudeUsage {
    input_tokens: i64,
    output_tokens: i64,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
    usage: Option<ClaudeUsage>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ClaudeContent {
    #[serde(rename = "text")]
    Text { text: String },
}

// Gemini API structures (OpenAI-compatible)
#[derive(Serialize)]
struct GeminiRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    choices: Vec<GeminiChoice>,
}

#[derive(Deserialize)]
struct GeminiChoice {
    message: GeminiResponseMessage,
}

#[derive(Deserialize)]
struct GeminiResponseMessage {
    content: String,
}

/// POST /api/chat
/// Conversational search: find relevant notes and synthesize answer via Claude
pub async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.message.trim().is_empty() {
        return Err(AppError::BadRequest("Message cannot be empty".into()));
    }

    info!(message = %body.message, "Chat request received");

    // 1. Search for relevant notes using hybrid search
    let notes = search_relevant_notes(&state, &body.message, body.max_results).await?;

    if notes.is_empty() {
        return Ok((
            StatusCode::OK,
            Json(ChatResponse {
                answer: "I couldn't find any relevant notes for your question.".to_string(),
                sources: vec![],
            }),
        ));
    }

    // 2. Build context from notes
    let context = build_context(&notes);

    // 3. Call LLM API to synthesize answer
    let answer = match body.llm {
        LlmProvider::Claude => {
            let (text, usage_tokens) =
                synthesize_answer_claude(&state.client, &body.message, &context).await?;

            // Record Claude API usage
            if let Some((input_tokens, output_tokens)) = usage_tokens {
                let cost = input_tokens as f64 * 3.0 / 1_000_000.0
                    + output_tokens as f64 * 15.0 / 1_000_000.0;
                let model_str = CLAUDE_MODEL.to_string();
                if let Ok(conn) = state.pool.get().await
                    && let Err(e) = flatten_interact(
                        conn.interact(move |conn| {
                            queries::record_usage(
                                conn,
                                "anthropic",
                                "chat",
                                None,
                                input_tokens,
                                output_tokens,
                                cost,
                                Some(&model_str),
                            )
                        })
                        .await,
                    )
                {
                    tracing::warn!(error = ?e, "Failed to record API usage for chat");
                }
            }

            text
        }
        LlmProvider::Gemini => {
            synthesize_answer_gemini(&state.client, &body.message, &context).await?
        }
    };

    // 4. Build source notes
    let sources: Vec<SourceNote> = notes
        .iter()
        .map(|n| SourceNote {
            id: n.id,
            title: n.title.clone(),
            content_preview: preview_content(&n.content, 150),
        })
        .collect();

    info!(note_count = notes.len(), "Chat response generated");

    Ok((StatusCode::OK, Json(ChatResponse { answer, sources })))
}

fn build_context(notes: &[queries::Note]) -> String {
    let mut context = String::from("Here are the relevant notes:\n\n");

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

    context
}

async fn synthesize_answer_claude(
    client: &reqwest::Client,
    question: &str,
    context: &str,
) -> Result<(String, Option<(i64, i64)>), AppError> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

    let request = ClaudeRequest {
        model: CLAUDE_MODEL.to_string(),
        max_tokens: 2048,
        messages: vec![Message {
            role: "user".to_string(),
            content: format!(
                "{}\n\nUser question: {}\n\nPlease answer the question based on the notes above. Cite note IDs when referencing specific information.",
                context, question
            ),
        }],
    };

    let res = client
        .post(CLAUDE_API_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&request)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to call Claude API: {e}"))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Claude API error {}: {}", status, body).into());
    }

    let claude_res: ClaudeResponse = res
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse Claude response: {e}"))?;

    // Extract usage info
    let usage_tokens = claude_res.usage.map(|u| (u.input_tokens, u.output_tokens));

    // Extract text from response
    if let Some(content) = claude_res.content.into_iter().next() {
        let ClaudeContent::Text { text } = content;
        return Ok((text, usage_tokens));
    }

    Err(anyhow::anyhow!("No text content in Claude response").into())
}

async fn synthesize_answer_gemini(
    client: &reqwest::Client,
    question: &str,
    context: &str,
) -> Result<String, AppError> {
    let api_key =
        std::env::var("GEMINI_API_KEY").map_err(|_| anyhow::anyhow!("GEMINI_API_KEY not set"))?;

    let request = GeminiRequest {
        model: GEMINI_MODEL.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: format!(
                "{}\n\nUser question: {}\n\nPlease answer the question based on the notes above. Cite note IDs when referencing specific information.",
                context, question
            ),
        }],
    };

    let res = client
        .post(GEMINI_API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to call Gemini API: {e}"))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Gemini API error {}: {}", status, body).into());
    }

    let gemini_res: GeminiResponse = res
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse Gemini response: {e}"))?;

    gemini_res
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| anyhow::anyhow!("No choices in Gemini response").into())
}

fn preview_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else {
        format!("{}...", &content[..max_len])
    }
}

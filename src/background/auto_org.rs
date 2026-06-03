use anyhow::{Context, Result};
use deadpool_sqlite::Pool;
use serde::{Deserialize, Serialize};
use tracing::info;

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-sonnet-4-5-20250929";

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    tools: Vec<Tool>,
    tool_choice: ToolChoice,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct ToolChoice {
    #[serde(rename = "type")]
    choice_type: String,
    name: String,
}

#[derive(Serialize)]
struct Tool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ClaudeUsage {
    input_tokens: i64,
    output_tokens: i64,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<Content>,
    usage: Option<ClaudeUsage>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Content {
    #[serde(rename = "tool_use")]
    ToolUse {
        #[allow(dead_code)]
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "text")]
    Text {
        #[allow(dead_code)]
        text: String,
    },
}

pub async fn auto_org_note(client: &reqwest::Client, pool: &Pool, note_id: i64) -> Result<()> {
    // Get note content
    let conn = pool.get().await?;
    let note = conn
        .interact(move |conn| crate::db::queries::get_note(conn, note_id))
        .await
        .map_err(|e| anyhow::anyhow!("Pool interaction error: {e}"))??;

    // Check if already has AI tags
    let existing_tags = conn
        .interact(move |conn| crate::db::queries::get_note_tags(conn, note_id))
        .await
        .map_err(|e| anyhow::anyhow!("Pool interaction error: {e}"))??;

    if existing_tags.iter().any(|t| t.source == "ai") {
        info!(note_id, "Note already has AI tags, skipping");
        return Ok(());
    }

    // Get API key from env
    let api_key = std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;

    // Build Claude request with single organize_note tool and force tool_choice
    let request = ClaudeRequest {
        model: MODEL.to_string(),
        max_tokens: 1024,
        tools: vec![Tool {
            name: "organize_note".to_string(),
            description: "Organize a captured note with title, summary, and tags".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Concise title (5-10 words)"
                    },
                    "summary": {
                        "type": "string",
                        "description": "1-2 sentence summary"
                    },
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "3-7 relevant topic tags"
                    }
                },
                "required": ["title", "summary", "tags"]
            }),
        }],
        tool_choice: ToolChoice {
            choice_type: "tool".to_string(),
            name: "organize_note".to_string(),
        },
        messages: vec![Message {
            role: "user".to_string(),
            content: format!("Organize this note:\n\n{}", note.content),
        }],
    };

    // Call Claude API
    let res = client
        .post(CLAUDE_API_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&request)
        .send()
        .await
        .context("Failed to call Claude API")?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("Claude API error {}: {}", status, body);
    }

    let claude_res: ClaudeResponse = res
        .json()
        .await
        .context("Failed to parse Claude response")?;

    // Extract usage before consuming content
    let api_usage = claude_res.usage;

    // Process organize_note tool call
    let mut tags = Vec::new();
    let mut title: Option<String> = None;
    let mut summary: Option<String> = None;

    // Find the organize_note tool use (should be the only/first one due to tool_choice)
    for content in claude_res.content {
        if let Content::ToolUse { name, input, .. } = content
            && name == "organize_note"
        {
            // Extract title
            if let Some(t) = input.get("title").and_then(|v| v.as_str()) {
                title = Some(t.to_string());
            }

            // Extract summary
            if let Some(s) = input.get("summary").and_then(|v| v.as_str()) {
                summary = Some(s.to_string());
            }

            // Extract tags
            if let Some(tag_arr) = input.get("tags").and_then(|v| v.as_array()) {
                for tag in tag_arr {
                    if let Some(tag_str) = tag.as_str() {
                        tags.push(tag_str.to_string());
                    }
                }
            }

            break; // Only one organize_note call expected
        }
    }

    // Ensure we got the required fields
    if title.is_none() || summary.is_none() || tags.is_empty() {
        anyhow::bail!("Claude did not return all required fields (title, summary, tags)");
    }

    // Update note with title and summary
    let title_clone = title.clone();
    let summary_clone = summary.clone();
    conn.interact(move |conn| {
        let mut stmt = conn.prepare(
            "UPDATE notes SET title = COALESCE(?1, title), summary = COALESCE(?2, summary), updated_at = datetime('now') WHERE id = ?3"
        )?;
        stmt.execute(rusqlite::params![title_clone, summary_clone, note_id])?;
        Ok::<(), rusqlite::Error>(())
    })
    .await
    .map_err(|e| anyhow::anyhow!("Pool interaction error: {e}"))??;

    // Add tags
    for tag_name in &tags {
        let tag_name_clone = tag_name.clone();
        let tag_id = conn
            .interact(move |conn| crate::db::queries::upsert_tag(conn, &tag_name_clone))
            .await
            .map_err(|e| anyhow::anyhow!("Pool interaction error: {e}"))??;

        conn.interact(move |conn| {
            crate::db::queries::add_note_tag(conn, note_id, tag_id, 1.0, "ai")
        })
        .await
        .map_err(|e| anyhow::anyhow!("Pool interaction error: {e}"))??;
    }

    // Record API usage
    if let Some(ref usage) = api_usage {
        let input_tokens = usage.input_tokens;
        let output_tokens = usage.output_tokens;
        let cost =
            input_tokens as f64 * 3.0 / 1_000_000.0 + output_tokens as f64 * 15.0 / 1_000_000.0;
        let model_str = MODEL.to_string();
        if let Err(e) = conn
            .interact(move |conn| {
                crate::db::queries::record_usage(
                    conn,
                    "anthropic",
                    "auto_org",
                    Some(note_id),
                    input_tokens,
                    output_tokens,
                    cost,
                    Some(&model_str),
                )
            })
            .await
            .map_err(|e| anyhow::anyhow!("Pool interaction error: {e}"))
            .and_then(|r| r)
        {
            tracing::warn!(error = ?e, "Failed to record API usage for auto_org");
        }
    }

    info!(note_id, title = ?title, summary = ?summary, tags = ?tags, "Auto-organized note");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    #[tokio::test]
    async fn test_organize_note_success() {
        let mock_server = MockServer::start().await;

        // Mock successful Claude API response with organize_note tool use
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg_123",
                "type": "message",
                "role": "assistant",
                "content": [
                    {
                        "type": "tool_use",
                        "id": "tool_123",
                        "name": "organize_note",
                        "input": {
                            "title": "Test Note Title",
                            "summary": "This is a test summary.",
                            "tags": ["test", "automation", "rust"]
                        }
                    }
                ],
                "model": "claude-sonnet-4-5-20250929",
                "stop_reason": "tool_use",
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 50
                }
            })))
            .mount(&mock_server)
            .await;

        // Test request structure
        let client = reqwest::Client::new();
        let request = ClaudeRequest {
            model: MODEL.to_string(),
            max_tokens: 1024,
            tools: vec![Tool {
                name: "organize_note".to_string(),
                description: "Organize a captured note with title, summary, and tags".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Concise title (5-10 words)"
                        },
                        "summary": {
                            "type": "string",
                            "description": "1-2 sentence summary"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "3-7 relevant topic tags"
                        }
                    },
                    "required": ["title", "summary", "tags"]
                }),
            }],
            tool_choice: ToolChoice {
                choice_type: "tool".to_string(),
                name: "organize_note".to_string(),
            },
            messages: vec![Message {
                role: "user".to_string(),
                content: "Organize this note:\n\nTest content".to_string(),
            }],
        };

        let res = client
            .post(format!("{}/v1/messages", mock_server.uri()))
            .header("x-api-key", "test-key")
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await
            .unwrap();

        assert!(res.status().is_success());

        let claude_res: ClaudeResponse = res.json().await.unwrap();

        // Extract and verify tool use
        let mut found_tool = false;
        for content in claude_res.content {
            if let Content::ToolUse { name, input, .. } = content {
                assert_eq!(name, "organize_note");
                assert_eq!(
                    input.get("title").and_then(|v| v.as_str()),
                    Some("Test Note Title")
                );
                assert_eq!(
                    input.get("summary").and_then(|v| v.as_str()),
                    Some("This is a test summary.")
                );
                assert_eq!(
                    input.get("tags").and_then(|v| v.as_array()).unwrap().len(),
                    3
                );
                found_tool = true;
            }
        }
        assert!(found_tool, "Expected organize_note tool use in response");
    }

    #[tokio::test]
    async fn test_organize_note_missing_required_fields() {
        let mock_server = MockServer::start().await;

        // Mock Claude API response missing required fields
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg_123",
                "type": "message",
                "role": "assistant",
                "content": [
                    {
                        "type": "tool_use",
                        "id": "tool_123",
                        "name": "organize_note",
                        "input": {
                            "title": "Test Title",
                            // Missing summary and tags
                        }
                    }
                ],
                "model": "claude-sonnet-4-5-20250929",
                "stop_reason": "tool_use",
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 50
                }
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let request = ClaudeRequest {
            model: MODEL.to_string(),
            max_tokens: 1024,
            tools: vec![Tool {
                name: "organize_note".to_string(),
                description: "Organize a captured note with title, summary, and tags".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "summary": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["title", "summary", "tags"]
                }),
            }],
            tool_choice: ToolChoice {
                choice_type: "tool".to_string(),
                name: "organize_note".to_string(),
            },
            messages: vec![Message {
                role: "user".to_string(),
                content: "Organize this note:\n\nTest".to_string(),
            }],
        };

        let res = client
            .post(format!("{}/v1/messages", mock_server.uri()))
            .header("x-api-key", "test-key")
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await
            .unwrap();

        let claude_res: ClaudeResponse = res.json().await.unwrap();

        // Verify parsing succeeds but fields are missing
        let mut title: Option<String> = None;
        let mut summary: Option<String> = None;
        let mut tags = Vec::new();

        for content in claude_res.content {
            if let Content::ToolUse { name, input, .. } = content {
                if name == "organize_note" {
                    if let Some(t) = input.get("title").and_then(|v| v.as_str()) {
                        title = Some(t.to_string());
                    }
                    if let Some(s) = input.get("summary").and_then(|v| v.as_str()) {
                        summary = Some(s.to_string());
                    }
                    if let Some(tag_arr) = input.get("tags").and_then(|v| v.as_array()) {
                        for tag in tag_arr {
                            if let Some(tag_str) = tag.as_str() {
                                tags.push(tag_str.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Should have title but missing summary and tags
        assert!(title.is_some());
        assert!(summary.is_none());
        assert!(tags.is_empty());
    }

    #[tokio::test]
    async fn test_organize_note_api_error() {
        let mock_server = MockServer::start().await;

        // Mock Claude API error response
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "type": "error",
                "error": {
                    "type": "authentication_error",
                    "message": "Invalid API key"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let request = ClaudeRequest {
            model: MODEL.to_string(),
            max_tokens: 1024,
            tools: vec![Tool {
                name: "organize_note".to_string(),
                description: "Organize a captured note".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "summary": {"type": "string"},
                        "tags": {"type": "array"}
                    },
                    "required": ["title", "summary", "tags"]
                }),
            }],
            tool_choice: ToolChoice {
                choice_type: "tool".to_string(),
                name: "organize_note".to_string(),
            },
            messages: vec![Message {
                role: "user".to_string(),
                content: "Organize this note:\n\nTest".to_string(),
            }],
        };

        let res = client
            .post(format!("{}/v1/messages", mock_server.uri()))
            .header("x-api-key", "bad-key")
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await
            .unwrap();

        // Verify error response
        assert!(!res.status().is_success());
        assert_eq!(res.status().as_u16(), 401);

        let body = res.text().await.unwrap();
        assert!(body.contains("authentication_error"));
    }
}

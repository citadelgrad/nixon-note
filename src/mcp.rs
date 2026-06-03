use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;

use crate::db;

#[derive(Clone)]
pub struct NoteMcpServer {
    db_path: String,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    /// Search query string
    pub query: String,
    /// Maximum number of results (default: 10)
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CaptureRequest {
    /// Note content (markdown supported)
    pub content: String,
    /// Source type (default: "mcp")
    #[serde(default = "default_source")]
    pub source_type: String,
    /// Tags to apply
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_source() -> String {
    "mcp".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetNoteRequest {
    /// Note ID to retrieve
    pub id: i64,
}

#[tool_router]
impl NoteMcpServer {
    pub fn new(db_path: String) -> Self {
        Self {
            db_path,
            tool_router: Self::tool_router(),
        }
    }

    fn open_db(&self) -> anyhow::Result<rusqlite::Connection> {
        db::open_and_migrate(&self.db_path)
    }

    #[tool(description = "Search notes in the knowledge base using full-text search")]
    fn search_notes(&self, Parameters(req): Parameters<SearchRequest>) -> String {
        match self.open_db() {
            Ok(conn) => match db::queries::search_fts(&conn, &req.query, req.limit) {
                Ok(notes) => {
                    let results: Vec<serde_json::Value> = notes
                        .iter()
                        .map(|n| {
                            serde_json::json!({
                                "id": n.id,
                                "title": n.title,
                                "content": if n.content.len() > 500 {
                                    format!("{}...", &n.content[..500])
                                } else {
                                    n.content.clone()
                                },
                                "source_type": n.source_type,
                                "created_at": n.created_at,
                            })
                        })
                        .collect();
                    serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
                }
                Err(e) => format!("Search error: {}", e),
            },
            Err(e) => format!("Database error: {}", e),
        }
    }

    #[tool(description = "Save a new note to the knowledge base")]
    fn capture_note(&self, Parameters(req): Parameters<CaptureRequest>) -> String {
        match self.open_db() {
            Ok(conn) => {
                match db::queries::insert_note(&conn, &req.content, "text", &req.source_type, None)
                {
                    Ok(id) => {
                        for tag_name in &req.tags {
                            if let Ok(tag_id) = db::queries::upsert_tag(&conn, tag_name) {
                                let _ = db::queries::add_note_tag(&conn, id, tag_id, 1.0, "manual");
                            }
                        }
                        format!("Note #{} created successfully", id)
                    }
                    Err(e) => format!("Failed to create note: {}", e),
                }
            }
            Err(e) => format!("Database error: {}", e),
        }
    }

    #[tool(description = "Get a specific note by ID with full content and tags")]
    fn get_note(&self, Parameters(req): Parameters<GetNoteRequest>) -> String {
        match self.open_db() {
            Ok(conn) => match db::queries::get_note(&conn, req.id) {
                Ok(note) => {
                    let tags = db::queries::get_note_tags(&conn, note.id)
                        .unwrap_or_default()
                        .iter()
                        .map(|t| t.tag_name.clone())
                        .collect::<Vec<_>>();
                    let result = serde_json::json!({
                        "id": note.id,
                        "content": note.content,
                        "title": note.title,
                        "summary": note.summary,
                        "source_type": note.source_type,
                        "source_url": note.source_url,
                        "tags": tags,
                        "created_at": note.created_at,
                        "updated_at": note.updated_at,
                    });
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
                }
                Err(e) => format!("Note not found: {}", e),
            },
            Err(e) => format!("Database error: {}", e),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for NoteMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_server_info(Implementation::new("nixonnote", "0.1.0"))
            .with_instructions("Search and manage notes in your personal knowledge base")
    }
}

pub async fn run_mcp_server(db_path: String) -> anyhow::Result<()> {
    let server = NoteMcpServer::new(db_path);
    let transport = rmcp::transport::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}

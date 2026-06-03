use std::env;
use std::net::SocketAddr;

use anyhow::{Context, Result, bail};
use tracing_subscriber::EnvFilter;

use note::{AppState, background, create_router, db, mcp};

fn db_path() -> String {
    env::var("NOTE_DB").unwrap_or_else(|_| "note.db".to_string())
}

// --- CLI mode ---

fn run_cli(args: &[String]) -> Result<()> {
    let content = args.join(" ");
    if content.trim().is_empty() {
        bail!("Usage: note <your thought here>");
    }

    let path = db_path();
    let conn = db::open_and_migrate(&path)?;
    let id = db::queries::insert_note(&conn, &content, "text", "cli", None)?;
    println!("Saved note #{id}");
    Ok(())
}

// --- MCP server mode ---

async fn run_mcp_server() -> Result<()> {
    let path = db_path();
    mcp::run_mcp_server(path).await
}

// --- Server mode ---

async fn run_server() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("note=info")),
        )
        .init();

    let path = db_path();

    // Run migrations on a direct connection first
    {
        let conn = db::open_and_migrate(&path)?;
        drop(conn);
    }

    let pool = db::create_pool(&path)?;

    // Verify pool works and set PRAGMAs on first connection
    {
        let conn = pool.get().await?;
        conn.interact(|c| db::setup_connection(c))
            .await
            .map_err(|e| anyhow::anyhow!("Pool interaction error: {e}"))??;
    }

    // Create HTTP client for background tasks (Ollama, Claude)
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    // Start background processor
    let background = background::BackgroundProcessor::new(pool.clone(), client.clone());

    // Re-derive queue on startup
    background.rederive_queue(&pool).await?;

    let state = AppState {
        pool,
        background,
        client,
    };

    let app = create_router(state);

    let port: u16 = env::var("NOTE_PORT")
        .unwrap_or_else(|_| "9999".to_string())
        .parse()
        .context("Invalid NOTE_PORT")?;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");
    tracing::info!("Shutting down...");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("mcp") {
        return run_mcp_server().await;
    }
    if args.is_empty() {
        run_server().await
    } else {
        run_cli(&args)
    }
}

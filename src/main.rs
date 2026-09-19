use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::{Context, Result, bail};
use tracing_subscriber::EnvFilter;

use note::{AppState, background, create_router, db, mcp};

fn db_path() -> String {
    env::var("NOTE_DB").unwrap_or_else(|_| "note.db".to_string())
}

/// Resolve the address the server binds to.
///
/// Defaults to loopback (`127.0.0.1`) so the server is not reachable from
/// other devices unless `NOTE_HOST` is set explicitly. An invalid `NOTE_HOST`
/// value is a hard error; it never silently falls back to an all-interfaces
/// bind such as `0.0.0.0`.
fn resolve_bind_host(note_host: Option<&str>) -> Result<IpAddr> {
    match note_host {
        None => Ok(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        Some(value) => value
            .parse()
            .with_context(|| format!("Invalid NOTE_HOST: {value:?}")),
    }
}

// --- CLI mode ---

fn run_cli(args: &[String]) -> Result<()> {
    if args.first().map(|s| s.as_str()) == Some("--backfill-audio-titles") {
        let path = db_path();
        let conn = db::open_and_migrate(&path)?;
        let updated = db::queries::backfill_audio_episode_titles(&conn)?;
        println!("Backfilled {updated} audio episode title(s)");
        return Ok(());
    }

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
        let updated = db::queries::backfill_audio_episode_titles(&conn)?;
        if updated > 0 {
            tracing::info!(updated, "Backfilled audio episode titles");
        }
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

    let host = resolve_bind_host(env::var("NOTE_HOST").ok().as_deref())?;

    let addr = SocketAddr::new(host, port);
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

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use super::*;

    #[test]
    fn resolve_bind_host_defaults_to_loopback() {
        let host = resolve_bind_host(None).unwrap();
        assert_eq!(host, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn resolve_bind_host_accepts_all_interfaces_override() {
        let host = resolve_bind_host(Some("0.0.0.0")).unwrap();
        assert_eq!(host, IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
    }

    #[test]
    fn resolve_bind_host_accepts_ipv6_loopback_override() {
        let host = resolve_bind_host(Some("::1")).unwrap();
        assert_eq!(host, IpAddr::V6(Ipv6Addr::LOCALHOST));
    }

    #[test]
    fn resolve_bind_host_rejects_invalid_value() {
        let err = resolve_bind_host(Some("not-an-ip")).unwrap_err();
        assert!(err.to_string().contains("NOTE_HOST"));
    }
}

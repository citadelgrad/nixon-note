pub mod background;
pub mod db;
pub mod mcp;
pub mod routes;

use std::env;

use subtle::ConstantTimeEq;

use axum::{
    Router,
    extract::{DefaultBodyLimit, Request},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use deadpool_sqlite::Pool;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub background: background::BackgroundProcessor,
    pub client: reqwest::Client,
}

async fn auth_middleware(req: Request, next: Next) -> Result<Response, StatusCode> {
    let token = env::var("NOTE_TOKEN").unwrap_or_default();
    if token.is_empty() {
        // No token configured = no auth required (dev mode)
        return Ok(next.run(req).await);
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(val)
            if val.starts_with("Bearer ")
                && bool::from(val.as_bytes()[7..].ct_eq(token.as_bytes())) =>
        {
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route(
            "/notes",
            get(routes::notes::get_notes).post(routes::notes::create_note),
        )
        .route("/notes/batch", post(routes::notes::create_notes_batch))
        .route(
            "/notes/{id}",
            get(routes::notes::get_note)
                .put(routes::notes::update_note)
                .delete(routes::notes::delete_note),
        )
        .route("/tags", get(routes::tags::list_tags))
        .route("/tags/filter", get(routes::tags::notes_by_tag))
        .route(
            "/voice",
            post(routes::voice::transcribe_voice).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route("/chat", post(routes::chat::chat))
        .route("/chat/stream", post(routes::chat_stream::chat_stream))
        .route("/ingest/bookmarks", post(routes::ingest::import_bookmarks))
        .route("/ingest/youtube", post(routes::ingest::ingest_youtube))
        .route("/ingest/url", post(routes::ingest::ingest_url))
        .route("/notes/random", get(routes::notes::random_notes))
        .route("/export", get(routes::export::export_notes))
        .route("/usage/summary", get(routes::usage::get_usage_summary))
        .route("/usage/daily", get(routes::usage::get_daily_usage))
        .route("/audio/generate", post(routes::audio::generate_audio))
        .route("/audio", get(routes::audio::list_episodes))
        .route("/podcast/feed.xml", get(routes::audio::podcast_feed))
        .route("/audio/{id}/file", get(routes::audio::serve_audio_file))
        .route(
            "/audio/{id}",
            get(routes::audio::get_episode).delete(routes::audio::delete_episode),
        )
        .route(
            "/settings",
            get(routes::settings::get_settings).put(routes::settings::update_settings),
        )
        .layer(middleware::from_fn(auth_middleware));

    let web_dir = env::var("NOTE_WEB_DIR").unwrap_or_else(|_| "web/dist".to_string());
    let index_file = format!("{web_dir}/index.html");
    let serve_dir = ServeDir::new(&web_dir).not_found_service(ServeFile::new(&index_file));

    Router::new()
        .route("/api/status", get(routes::status::get_status))
        .nest("/api", api_routes)
        .fallback_service(serve_dir)
        .layer(CompressionLayer::new())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

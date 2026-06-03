use axum::Json;
use axum::extract::State;
use serde::Serialize;
use std::env;
use std::time::Duration;

use crate::AppState;

#[derive(Serialize)]
pub struct StatusResponse {
    pub services: Services,
    pub server: ServerConfig,
}

#[derive(Serialize)]
pub struct Services {
    pub ollama: ServiceStatus,
    pub anthropic: ServiceStatus,
    pub gemini: ServiceStatus,
    pub whisper: ServiceStatus,
    pub web_clip: ServiceStatus,
    pub openai_tts: ServiceStatus,
    pub elevenlabs_tts: ServiceStatus,
}

#[derive(Serialize)]
pub struct ServiceStatus {
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_dir: Option<String>,
    pub healthy: bool,
}

#[derive(Serialize)]
pub struct ServerConfig {
    pub port: String,
    pub db_path: String,
    pub auth_enabled: bool,
    pub web_dir: String,
}

pub async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let ollama = check_ollama(&state.client).await;
    let anthropic = check_anthropic();
    let gemini = check_gemini();
    let whisper = check_whisper();
    let web_clip = ServiceStatus {
        configured: true,
        url: None,
        model: None,
        model_size: None,
        model_dir: None,
        healthy: true,
    };
    let openai_tts = check_openai_tts();
    let elevenlabs_tts = check_elevenlabs_tts();

    let server = ServerConfig {
        port: env::var("NOTE_PORT").unwrap_or_else(|_| "8999".to_string()),
        db_path: env::var("NOTE_DB").unwrap_or_else(|_| "note.db".to_string()),
        auth_enabled: env::var("NOTE_TOKEN")
            .map(|t| !t.is_empty())
            .unwrap_or(false),
        web_dir: env::var("NOTE_WEB_DIR").unwrap_or_else(|_| "web/dist".to_string()),
    };

    Json(StatusResponse {
        services: Services {
            ollama,
            anthropic,
            gemini,
            whisper,
            web_clip,
            openai_tts,
            elevenlabs_tts,
        },
        server,
    })
}

async fn check_ollama(client: &reqwest::Client) -> ServiceStatus {
    let url = "http://localhost:11434";
    let healthy = match client
        .get(format!("{}/api/tags", url))
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(res) => res.status().is_success(),
        Err(_) => false,
    };

    ServiceStatus {
        configured: true,
        url: Some(url.to_string()),
        model: Some("nomic-embed-text".to_string()),
        model_size: None,
        model_dir: None,
        healthy,
    }
}

fn check_anthropic() -> ServiceStatus {
    let key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    let configured = !key.is_empty();

    ServiceStatus {
        configured,
        url: None,
        model: Some("claude-sonnet-4-5-20250929".to_string()),
        model_size: None,
        model_dir: None,
        healthy: configured,
    }
}

fn check_gemini() -> ServiceStatus {
    let key = env::var("GEMINI_API_KEY").unwrap_or_default();
    let configured = !key.is_empty();

    ServiceStatus {
        configured,
        url: None,
        model: Some("gemini-2.5-flash".to_string()),
        model_size: None,
        model_dir: None,
        healthy: configured,
    }
}

fn check_openai_tts() -> ServiceStatus {
    let key = env::var("OPENAI_API_KEY").unwrap_or_default();
    let configured = !key.is_empty();

    ServiceStatus {
        configured,
        url: None,
        model: Some("tts-1".to_string()),
        model_size: None,
        model_dir: None,
        healthy: configured,
    }
}

fn check_elevenlabs_tts() -> ServiceStatus {
    let key = env::var("ELEVENLABS_API_KEY").unwrap_or_default();
    let configured = !key.is_empty();

    ServiceStatus {
        configured,
        url: None,
        model: Some(env::var("ELEVENLABS_TTS_MODEL").unwrap_or_else(|_| "eleven_v3".to_string())),
        model_size: None,
        model_dir: None,
        healthy: configured,
    }
}

fn check_whisper() -> ServiceStatus {
    let ffmpeg_exists = std::path::Path::new("/opt/homebrew/bin/ffmpeg").exists();
    let model_size = env::var("WHISPER_MODEL_SIZE").unwrap_or_else(|_| "medium".to_string());
    let model_dir =
        env::var("WHISPER_MODEL_DIR").unwrap_or_else(|_| "~/.cache/whisper".to_string());

    ServiceStatus {
        configured: ffmpeg_exists,
        url: None,
        model: None,
        model_size: Some(model_size),
        model_dir: Some(model_dir),
        healthy: ffmpeg_exists,
    }
}

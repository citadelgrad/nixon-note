use crate::AppState;
use crate::db::queries;
use crate::routes::notes::{AppError, flatten_interact};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct DailyQuery {
    #[serde(default = "default_days")]
    pub days: i64,
}

fn default_days() -> i64 {
    30
}

pub async fn get_usage_summary(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let summary = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(|conn| queries::get_usage_summary(conn))
            .await,
    )?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "summary": summary })),
    ))
}

pub async fn get_daily_usage(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<DailyQuery>,
) -> Result<impl IntoResponse, AppError> {
    let daily = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::get_daily_usage(conn, params.days))
            .await,
    )?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "daily": daily }))))
}

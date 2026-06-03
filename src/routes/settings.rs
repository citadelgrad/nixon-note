use axum::{Json, extract::State, response::IntoResponse};
use serde::Serialize;
use std::collections::HashMap;

use crate::AppState;
use crate::db::queries;
use crate::routes::notes::{AppError, flatten_interact};

#[derive(Serialize)]
pub struct SettingsResponse {
    pub settings: HashMap<String, String>,
}

/// GET /api/settings
pub async fn get_settings(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let settings = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(|conn| queries::get_all_settings(conn))
            .await,
    )?;

    Ok(Json(SettingsResponse { settings }))
}

/// PUT /api/settings
pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let pool = state.pool.clone();

    for (key, value) in &body {
        let k = key.clone();
        let v = value.clone();
        flatten_interact(
            pool.get()
                .await
                .map_err(anyhow::Error::from)?
                .interact(move |conn| queries::set_setting(conn, &k, &v))
                .await,
        )?;
    }

    // Return updated settings
    let settings = flatten_interact(
        pool.get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(|conn| queries::get_all_settings(conn))
            .await,
    )?;

    Ok(Json(SettingsResponse { settings }))
}

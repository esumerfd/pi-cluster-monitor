use std::sync::{Arc, RwLock};

use axum::{Json, extract::State};

use crate::types::ApiStats;

pub async fn stats_handler(
    State(state): State<Arc<RwLock<ApiStats>>>,
) -> Json<ApiStats> {
    Json(state.read().unwrap().clone())
}

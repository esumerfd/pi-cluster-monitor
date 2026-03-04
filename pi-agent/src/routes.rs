use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::{Json, extract::{ConnectInfo, State}, http::HeaderMap};
use tracing::{debug, info};

use crate::types::ApiStats;

pub async fn stats_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(state): State<Arc<RwLock<ApiStats>>>,
) -> Json<ApiStats> {
    info!(">>> GET /api/stats from {}", addr);
    if tracing::enabled!(tracing::Level::DEBUG) {
        for (name, value) in &headers {
            debug!("    {}: {}", name, value.to_str().unwrap_or("<binary>"));
        }
    }

    let stats = state.read().unwrap().clone();

    if tracing::enabled!(tracing::Level::DEBUG) {
        match serde_json::to_string_pretty(&stats) {
            Ok(json) => debug!("<<< response to {}:\n{}", addr, json),
            Err(e)   => debug!("<<< response to {} (serialise error: {})", addr, e),
        }
    }

    Json(stats)
}

mod collector;
mod routes;
mod types;

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use axum::{Router, routing::get};
use clap::Parser;
use tracing::info;

use types::ApiStats;

#[derive(Parser, Debug)]
#[command(name = "pi-agent", about = "Unified Pi node metrics agent")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value_t = 8765)]
    port: u16,

    /// Collector poll interval in milliseconds
    #[arg(long, default_value_t = 2000)]
    interval: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pi_agent=info".into()),
        )
        .init();

    let args = Args::parse();

    let state: Arc<RwLock<ApiStats>> = Arc::new(RwLock::new(ApiStats::default()));

    // Start background collector
    tokio::spawn({
        let s = state.clone();
        let interval = args.interval;
        async move { collector::run(s, interval).await }
    });

    let app = Router::new()
        .route("/api/stats", get(routes::stats_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    info!("pi-agent listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

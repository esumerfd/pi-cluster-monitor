mod network;
mod raspi_dash;
mod system;

use std::sync::{Arc, RwLock};
use crate::app::AppState;
use crate::inventory::InventoryNode;

/// Spawn all background collector tasks.
pub async fn start_collectors(
    state: Arc<RwLock<AppState>>,
    inventory_nodes: Vec<InventoryNode>,
    inventory_path: String,
) {
    tokio::spawn({
        let s = state.clone();
        async move { system::run(s).await }
    });

    tokio::spawn({
        let s = state.clone();
        let nodes = inventory_nodes.clone();
        let path = inventory_path.clone();
        async move { network::run(s, nodes, path).await }
    });

    tokio::spawn({
        let s = state.clone();
        let nodes = inventory_nodes.clone();
        async move { raspi_dash::run(s, nodes).await }
    });
}

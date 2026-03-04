mod network;
mod node_agent;

use std::sync::{Arc, RwLock};
use crate::app::AppState;
use crate::inventory::InventoryNode;

/// Spawn all background collector tasks.
pub async fn start_collectors(
    state: Arc<RwLock<AppState>>,
    inventory_nodes: Vec<InventoryNode>,
    inventory_path: String,
    agent_port: u16,
) {
    tokio::spawn({
        let s = state.clone();
        let nodes = inventory_nodes.clone();
        let path = inventory_path.clone();
        async move { network::run(s, nodes, path).await }
    });

    tokio::spawn({
        let s = state.clone();
        let nodes = inventory_nodes.clone();
        async move { node_agent::run(s, nodes, agent_port).await }
    });
}

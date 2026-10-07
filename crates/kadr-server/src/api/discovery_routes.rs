use axum::{extract::Extension, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::AppConfig;
use crate::identity::ServerIdentity;
use kadr_storage::repos::UserRepository;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveryResponse {
    pub app: String,           // "kadr"
    pub server_id: String,     // persistent UUID
    pub name: String,          // configured server name
    pub version: String,       // env!("CARGO_PKG_VERSION")
    pub protocol_version: u32, // 1
    pub port: u16,             // configured port
    pub setup_completed: bool, // user_repo.count() > 0
    pub status: String,        // "online"
}

pub async fn get_discovery(
    Extension(identity): Extension<Arc<ServerIdentity>>,
    Extension(config): Extension<Arc<RwLock<AppConfig>>>,
    Extension(user_repo): Extension<UserRepository>,
) -> (StatusCode, Json<DiscoveryResponse>) {
    let cfg = config.read().await;
    let user_count = user_repo.count().await.unwrap_or(0);
    let setup_completed = user_count > 0;

    let response = DiscoveryResponse {
        app: "kadr".to_string(),
        server_id: identity.id.clone(),
        name: cfg.server.name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        protocol_version: 1,
        port: cfg.server.port,
        setup_completed,
        status: "online".to_string(),
    };

    (StatusCode::OK, Json(response))
}

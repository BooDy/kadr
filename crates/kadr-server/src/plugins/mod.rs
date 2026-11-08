pub mod admin_routes;
pub mod auth;
pub mod client;
pub mod health;
pub mod playback;
pub mod publish_routes;
pub mod service;
pub mod url_guard;

pub use service::{PluginService, ServiceError};

/// All plugin-related HTTP routes (admin registration and plugin publishing).
pub fn plugin_routes() -> axum::Router {
    admin_routes::admin_routes().merge(publish_routes::publish_routes())
}


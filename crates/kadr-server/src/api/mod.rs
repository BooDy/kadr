pub mod auth_routes;
pub mod item_routes;
pub mod screen_routes;
pub mod user_routes;
pub mod widget_routes;

use axum::{
    routing::{delete, get, post},
    Extension, Router,
};
use std::sync::Arc;
use crate::auth::jwt::JwtService;
use crate::auth::rate_limiter::RateLimiter;
use crate::layout::LayoutRegistry;
use crate::playback::{self, SessionRegistry};
use crate::resolver::WidgetResolver;
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository};

#[allow(clippy::too_many_arguments)]
pub fn create_router_with_layout(
    user_repo: UserRepository,
    playback_repo: PlaybackRepository,
    media_repo: MediaItemRepository,
    lib_repo: LibraryRepository,
    jwt_svc: JwtService,
    limiter: RateLimiter,
    session_registry: Arc<SessionRegistry>,
    layout_registry: LayoutRegistry,
    widget_resolver: Arc<WidgetResolver>,
) -> Router {
    Router::new()
        // Public profile list & auth
        .route("/api/v1/users/profiles", get(user_routes::list_profiles))
        .route("/api/v1/auth/profile-pin", post(auth_routes::profile_pin_auth))
        .route("/api/v1/auth/me", get(auth_routes::get_current_user))
        .route("/api/v1/users", post(user_routes::create_user))
        // Media streaming direct-play route
        .route("/api/v1/stream/{item_id}", get(crate::streaming::stream_media_item))
        // Playback session tracking & scrobble synchronization
        .route("/api/v1/playback/sessions", post(playback::create_session))
        .route("/api/v1/playback/sessions/{session_id}", delete(playback::close_session))
        .route("/api/v1/playback/{session_id}", delete(playback::close_session))
        .route("/api/v1/playback/{session_id}/progress", post(playback::progress_heartbeat))
        .route("/api/v1/playback/states/{item_id}", get(playback::get_playback_state))
        .route("/api/v1/playback/continue-watching", get(playback::list_continue_watching))
        // Screen, Widget AST & Item Details routes
        .route("/api/v1/screens", get(screen_routes::list_screens))
        .route("/api/v1/screens/{screen_id}", get(screen_routes::get_screen))
        .route("/api/v1/widgets/{widget_id}/data", get(widget_routes::get_widget_data))
        .route("/api/v1/items/{item_id}/details", get(item_routes::get_item_details))
        .layer(Extension(user_repo))
        .layer(Extension(playback_repo))
        .layer(Extension(media_repo))
        .layer(Extension(lib_repo))
        .layer(Extension(jwt_svc))
        .layer(Extension(limiter))
        .layer(Extension(session_registry))
        .layer(Extension(layout_registry))
        .layer(Extension(widget_resolver))
}

pub fn create_router(
    user_repo: UserRepository,
    playback_repo: PlaybackRepository,
    media_repo: MediaItemRepository,
    lib_repo: LibraryRepository,
    jwt_svc: JwtService,
    limiter: RateLimiter,
    session_registry: Arc<SessionRegistry>,
) -> Router {
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));
    create_router_with_layout(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
        layout_registry,
        widget_resolver,
    )
}

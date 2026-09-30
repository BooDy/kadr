pub mod auth_routes;
pub mod user_routes;

use axum::{
    routing::{get, post},
    Extension, Router,
};
use crate::auth::jwt::JwtService;
use crate::auth::rate_limiter::RateLimiter;
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository};

pub fn create_router(
    user_repo: UserRepository,
    playback_repo: PlaybackRepository,
    media_repo: MediaItemRepository,
    lib_repo: LibraryRepository,
    jwt_svc: JwtService,
    limiter: RateLimiter,
) -> Router {
    Router::new()
        // Public profile list & auth
        .route("/api/v1/users/profiles", get(user_routes::list_profiles))
        .route("/api/v1/auth/profile-pin", post(auth_routes::profile_pin_auth))
        .route("/api/v1/auth/me", get(auth_routes::get_current_user))
        .route("/api/v1/users", post(user_routes::create_user))
        .layer(Extension(user_repo))
        .layer(Extension(playback_repo))
        .layer(Extension(media_repo))
        .layer(Extension(lib_repo))
        .layer(Extension(jwt_svc))
        .layer(Extension(limiter))
}

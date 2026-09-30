use axum::{
    extract::{ConnectInfo, FromRequestParts, Extension, Json},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use kadr_core::models::UserRole;
use crate::auth::jwt::{AuthUser, JwtService};
use crate::auth::pin::verify_pin;
use crate::auth::rate_limiter::{RateLimitStatus, RateLimiter};
use kadr_storage::repos::UserRepository;

#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let peer_ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip());

        // Only trust forwarded headers if the immediate peer is loopback (e.g. reverse proxy on localhost)
        // or if ConnectInfo is not available (e.g. during test setups without socket peer info).
        let is_trusted_proxy = match peer_ip {
            Some(ip) => ip.is_loopback(),
            None => true,
        };

        if is_trusted_proxy {
            if let Some(forwarded) = parts.headers.get("x-forwarded-for") {
                if let Ok(s) = forwarded.to_str() {
                    if let Some(first) = s.split(',').next() {
                        if let Ok(ip) = first.trim().parse::<IpAddr>() {
                            return Ok(ClientIp(ip));
                        }
                    }
                }
            }
            if let Some(real_ip) = parts.headers.get("x-real-ip") {
                if let Ok(s) = real_ip.to_str() {
                    if let Ok(ip) = s.trim().parse::<IpAddr>() {
                        return Ok(ClientIp(ip));
                    }
                }
            }
        }

        let final_ip = peer_ip.unwrap_or_else(|| "127.0.0.1".parse().unwrap());
        Ok(ClientIp(final_ip))
    }
}

#[derive(Deserialize)]
pub struct PinAuthRequest {
    pub user_id: String,
    pub pin: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub username: String,
    pub role: UserRole,
}

pub async fn profile_pin_auth(
    Extension(user_repo): Extension<UserRepository>,
    Extension(jwt_svc): Extension<JwtService>,
    Extension(limiter): Extension<RateLimiter>,
    ClientIp(client_ip): ClientIp,
    Json(payload): Json<PinAuthRequest>,
) -> Response {
    if let RateLimitStatus::LockedOut { retry_after_secs } = limiter.check_attempt(&client_ip).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", retry_after_secs.to_string())],
            Json(serde_json::json!({
                "error": "Too many failed attempts. Try again later.",
                "retry_after_seconds": retry_after_secs
            })),
        ).into_response();
    }

    let user = match user_repo.get_by_id(&payload.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            limiter.record_failure(&client_ip).await;
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid user or PIN" })),
            ).into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "Database error retrieving user during auth");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal server error" })),
            ).into_response();
        }
    };

    match verify_pin(&payload.pin, &user.pin_hash) {
        Ok(true) => {
            limiter.record_success(&client_ip).await;
            match jwt_svc.generate_token(&user) {
                Ok(token) => (
                    StatusCode::OK,
                    Json(AuthResponse {
                        token,
                        user_id: user.id,
                        username: user.username,
                        role: user.role,
                    }),
                ).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to create session token" })),
                ).into_response(),
            }
        }
        _ => {
            limiter.record_failure(&client_ip).await;
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid user or PIN" })),
            ).into_response()
        }
    }
}

pub async fn get_current_user(auth_user: AuthUser) -> impl IntoResponse {
    Json(serde_json::json!({
        "id": auth_user.id,
        "username": auth_user.username,
        "role": auth_user.role,
    }))
}

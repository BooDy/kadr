use axum::{
    extract::{Extension, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use kadr_core::models::{User, UserRole};
use crate::auth::jwt::RequireAdmin;
use crate::auth::pin::hash_pin;
use kadr_storage::repos::UserRepository;

#[derive(Serialize)]
pub struct ProfileCard {
    pub id: String,
    pub username: String,
    pub role: UserRole,
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub pin: String,
    pub role: UserRole,
}

pub async fn list_profiles(Extension(user_repo): Extension<UserRepository>) -> impl IntoResponse {
    match user_repo.list_all().await {
        Ok(users) => {
            let cards: Vec<ProfileCard> = users
                .into_iter()
                .map(|u| ProfileCard {
                    id: u.id,
                    username: u.username,
                    role: u.role,
                })
                .collect();
            Json(cards).into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to list profiles" })),
        ).into_response(),
    }
}

pub async fn create_user(
    _admin: RequireAdmin,
    Extension(user_repo): Extension<UserRepository>,
    Json(payload): Json<CreateUserRequest>,
) -> impl IntoResponse {
    let trimmed_username = payload.username.trim();
    if trimmed_username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Username cannot be empty or whitespace only" })),
        ).into_response();
    }

    let pin_hash = match hash_pin(&payload.pin) {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            ).into_response();
        }
    };

    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => dur.as_secs() as i64,
        Err(_) => 0,
    };
    let new_user = User {
        id: Uuid::new_v4().to_string(),
        username: trimmed_username.to_string(),
        pin_hash,
        role: payload.role,
        created_at: now,
    };

    match user_repo.create(&new_user).await {
        Ok(_) => (
            StatusCode::CREATED,
            Json(ProfileCard {
                id: new_user.id,
                username: new_user.username,
                role: new_user.role,
            }),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ).into_response(),
    }
}

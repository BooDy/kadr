use axum::{
    extract::{FromRequestParts, Query},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use kadr_core::models::{AuthClaims, User, UserRole};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    ttl_seconds: usize,
}

impl JwtService {
    pub fn new(secret: &str, ttl_seconds: usize) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            ttl_seconds,
        }
    }

    pub fn generate_token(&self, user: &User) -> Result<String, jsonwebtoken::errors::Error> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as usize;
        let claims = AuthClaims {
            sub: user.id.clone(),
            username: user.username.clone(),
            role: user.role,
            exp: now + self.ttl_seconds,
            iat: now,
        };
        encode(&Header::default(), &claims, &self.encoding_key)
    }

    pub fn verify_token(&self, token: &str) -> Result<AuthClaims, jsonwebtoken::errors::Error> {
        let validation = Validation::default();
        let token_data = decode::<AuthClaims>(token, &self.decoding_key, &validation)?;
        Ok(token_data.claims)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub role: UserRole,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequireAdmin(pub AuthUser);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = if let Some(auth_header) = parts.headers.get("authorization") {
            if let Ok(header_str) = auth_header.to_str() {
                header_str
                    .strip_prefix("Bearer ")
                    .map(|bearer| bearer.trim().to_string())
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| {
            // Check query param (?token=...)
            if parts.uri.query().is_some() {
                let parsed: Result<Query<TokenQuery>, _> = Query::try_from_uri(&parts.uri);
                parsed.ok().and_then(|Query(q)| q.token)
            } else {
                None
            }
        });

        let token = match token {
            Some(t) if !t.is_empty() => t,
            _ => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Missing authorization token" })),
                )
                    .into_response());
            }
        };

        let jwt_service = match parts.extensions.get::<JwtService>() {
            Some(svc) => svc,
            None => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "JWT service not configured" })),
                )
                    .into_response());
            }
        };

        match jwt_service.verify_token(&token) {
            Ok(claims) => Ok(AuthUser {
                id: claims.sub,
                username: claims.username,
                role: claims.role,
            }),
            Err(_) => Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid or expired token" })),
            )
                .into_response()),
        }
    }
}

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_user = AuthUser::from_request_parts(parts, state).await?;
        if auth_user.role == UserRole::Admin {
            Ok(RequireAdmin(auth_user))
        } else {
            Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "Admin privileges required" })),
            )
                .into_response())
        }
    }
}

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::prelude::*;
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const TOKEN_TYP: &str = "kadr-plugin";
const TOKEN_TTL_SECS: usize = 365 * 24 * 3600;

fn hmac_bytes(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

/// Secret the server sends as `Authorization: Bearer <secret>` on every call to a plugin.
/// Derived (never stored): `base64url_nopad(HMAC-SHA256(server_secret, "kadr-plugin-secret-v1:" + plugin_id))`.
pub fn derive_plugin_secret(server_secret: &[u8], plugin_id: &str) -> String {
    let msg = format!("kadr-plugin-secret-v1:{plugin_id}");
    BASE64_URL_SAFE_NO_PAD.encode(hmac_bytes(server_secret, msg.as_bytes()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginClaims {
    pub sub: String,
    pub plugin_id: String,
    pub typ: String,
    pub exp: usize,
    pub iat: usize,
}

/// Issues and verifies tokens a plugin uses to call back into Kadr.
/// The signing key is derived from the server secret with a distinct label, and the claim
/// shape differs from `AuthClaims`, so user and plugin tokens can never be confused.
#[derive(Clone)]
pub struct PluginTokenService {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl PluginTokenService {
    pub fn new(server_secret: &[u8]) -> Self {
        let key = hmac_bytes(server_secret, b"kadr-plugin-token-key-v1");
        Self {
            encoding: EncodingKey::from_secret(&key),
            decoding: DecodingKey::from_secret(&key),
        }
    }

    pub fn issue(&self, plugin_id: &str) -> Result<String, jsonwebtoken::errors::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as usize;
        let claims = PluginClaims {
            sub: format!("plugin:{plugin_id}"),
            plugin_id: plugin_id.to_string(),
            typ: TOKEN_TYP.to_string(),
            exp: now + TOKEN_TTL_SECS,
            iat: now,
        };
        encode(&Header::default(), &claims, &self.encoding)
    }

    pub fn verify(&self, token: &str) -> Result<PluginClaims, jsonwebtoken::errors::Error> {
        let data = decode::<PluginClaims>(token, &self.decoding, &Validation::default())?;
        let claims = data.claims;
        if claims.typ != TOKEN_TYP || claims.sub != format!("plugin:{}", claims.plugin_id) {
            return Err(jsonwebtoken::errors::ErrorKind::InvalidToken.into());
        }
        Ok(claims)
    }
}

/// Extractor for routes called by plugins. Only the `Authorization: Bearer` header is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginAuth {
    pub plugin_id: String,
}

fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": msg })),
    )
        .into_response()
}

impl<S> FromRequestParts<S> for PluginAuth
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        let Some(token) = token else {
            return Err(unauthorized("Missing plugin token"));
        };
        let Some(svc) = parts.extensions.get::<PluginTokenService>() else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Plugin token service not configured" })),
            )
                .into_response());
        };
        match svc.verify(&token) {
            Ok(claims) => Ok(PluginAuth {
                plugin_id: claims.plugin_id,
            }),
            Err(_) => Err(unauthorized("Invalid plugin token")),
        }
    }
}

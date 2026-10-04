use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::prelude::*;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashSet;
use std::ops::Deref;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnlockTokenClaims {
    pub library_id: String,
    pub exp: i64,
}

#[derive(Debug, Error)]
pub enum UnlockTokenError {
    #[error("Invalid token format")]
    InvalidFormat,
    #[error("Base64 decoding failed")]
    Base64DecodeError,
    #[error("Invalid token signature")]
    InvalidSignature,
    #[error("Token expired")]
    Expired,
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct UnlockTokenService {
    secret: Vec<u8>,
    ttl_seconds: u64,
}

impl UnlockTokenService {
    pub fn new(secret: impl AsRef<[u8]>, ttl_seconds: u64) -> Self {
        Self {
            secret: secret.as_ref().to_vec(),
            ttl_seconds,
        }
    }

    pub fn generate_token(&self, library_id: &str) -> Result<(String, i64), UnlockTokenError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let expires_at = now + self.ttl_seconds as i64;

        let claims = UnlockTokenClaims {
            library_id: library_id.to_string(),
            exp: expires_at,
        };

        let payload_json = serde_json::to_vec(&claims)?;
        let payload_b64 = BASE64_URL_SAFE_NO_PAD.encode(payload_json);

        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|_| UnlockTokenError::InvalidSignature)?;
        mac.update(payload_b64.as_bytes());
        let signature = mac.finalize().into_bytes();
        let signature_b64 = BASE64_URL_SAFE_NO_PAD.encode(signature);

        let token = format!("{}.{}", payload_b64, signature_b64);
        Ok((token, expires_at))
    }

    pub fn verify_token(&self, token: &str) -> Result<UnlockTokenClaims, UnlockTokenError> {
        let mut parts = token.split('.');
        let payload_b64 = parts.next().ok_or(UnlockTokenError::InvalidFormat)?;
        let sig_b64 = parts.next().ok_or(UnlockTokenError::InvalidFormat)?;
        if parts.next().is_some() {
            return Err(UnlockTokenError::InvalidFormat);
        }

        let provided_sig = BASE64_URL_SAFE_NO_PAD
            .decode(sig_b64)
            .map_err(|_| UnlockTokenError::Base64DecodeError)?;

        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|_| UnlockTokenError::InvalidSignature)?;
        mac.update(payload_b64.as_bytes());
        mac.verify_slice(&provided_sig)
            .map_err(|_| UnlockTokenError::InvalidSignature)?;

        let payload_bytes = BASE64_URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| UnlockTokenError::Base64DecodeError)?;

        let claims: UnlockTokenClaims = serde_json::from_slice(&payload_bytes)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if now >= claims.exp {
            return Err(UnlockTokenError::Expired);
        }

        Ok(claims)
    }
}

/// Axum extractor that parses `X-Kadr-Unlocked` header (and optional `unlocked` query param),
/// verifies all signed HMAC tokens against `UnlockTokenService`,
/// and returns the set of valid unlocked library IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnlockedLibraries(pub HashSet<String>);

impl UnlockedLibraries {
    pub fn new(ids: HashSet<String>) -> Self {
        Self(ids)
    }

    pub fn is_unlocked(&self, library_id: &str) -> bool {
        self.0.contains(library_id)
    }

    pub fn ids(&self) -> &HashSet<String> {
        &self.0
    }

    pub fn to_vec(&self) -> Vec<String> {
        self.0.iter().cloned().collect()
    }
}

impl Deref for UnlockedLibraries {
    type Target = HashSet<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for UnlockedLibraries
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let mut unlocked = HashSet::new();

        let token_service = parts
            .extensions
            .get::<UnlockTokenService>()
            .cloned()
            .or_else(|| {
                parts
                    .extensions
                    .get::<crate::auth::jwt::JwtService>()
                    .map(|jwt| UnlockTokenService::new(jwt.secret(), 86400 * 7))
            });

        if let Some(svc) = token_service {
            // Inspect X-Kadr-Unlocked header values (handles comma-separated and multiple headers)
            for header_val in parts.headers.get_all("x-kadr-unlocked") {
                if let Ok(header_str) = header_val.to_str() {
                    for token in header_str.split(',') {
                        let token = token.trim();
                        if !token.is_empty() {
                            if let Ok(claims) = svc.verify_token(token) {
                                unlocked.insert(claims.library_id);
                            }
                        }
                    }
                }
            }

            // Also inspect query parameter if present (?unlocked=<token>,...)
            if let Some(query_str) = parts.uri.query() {
                for pair in query_str.split('&') {
                    if let Some(val) = pair.strip_prefix("unlocked=") {
                        for token in val.split(',') {
                            let token = token.trim();
                            if !token.is_empty() {
                                if let Ok(claims) = svc.verify_token(token) {
                                    unlocked.insert(claims.library_id);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(UnlockedLibraries(unlocked))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify_token() {
        let svc = UnlockTokenService::new("my-test-secret-key-32-bytes-long!", 3600);
        let (token, exp) = svc.generate_token("lib-123").unwrap();
        assert!(exp > 0);

        let claims = svc.verify_token(&token).unwrap();
        assert_eq!(claims.library_id, "lib-123");
        assert_eq!(claims.exp, exp);
    }

    #[test]
    fn test_expired_token() {
        let svc = UnlockTokenService::new("my-test-secret-key-32-bytes-long!", 0);
        let (token, _) = svc.generate_token("lib-123").unwrap();

        // With 0 TTL, token is expired immediately (now >= exp)
        std::thread::sleep(std::time::Duration::from_millis(10));
        let res = svc.verify_token(&token);
        assert!(matches!(res, Err(UnlockTokenError::Expired)));
    }

    #[test]
    fn test_invalid_signature() {
        let svc1 = UnlockTokenService::new("secret-key-one-32-bytes-long!!!!!", 3600);
        let svc2 = UnlockTokenService::new("secret-key-two-32-bytes-long!!!!!", 3600);

        let (token, _) = svc1.generate_token("lib-123").unwrap();
        let res = svc2.verify_token(&token);
        assert!(matches!(res, Err(UnlockTokenError::InvalidSignature)));
    }

    #[test]
    fn test_malformed_token() {
        let svc = UnlockTokenService::new("secret-key-32-bytes-long!!!!!!!!!", 3600);
        assert!(matches!(
            svc.verify_token("invalid"),
            Err(UnlockTokenError::InvalidFormat)
        ));
        assert!(matches!(
            svc.verify_token("a.b.c"),
            Err(UnlockTokenError::InvalidFormat)
        ));
    }
}

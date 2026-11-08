use crate::plugins::client::PluginError;
use crate::plugins::service::{CachedDescriptor, PluginService};
use crate::plugins::url_guard::validate_descriptor;
use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use kadr_core::models::MediaItem;
use kadr_core::plugin::{
    ClientCapabilities, PlaybackDescriptor, PlaybackMode, PluginPermission, PluginStatus,
    ResolveRequest,
};
use std::time::{Duration, Instant};

const MAX_CACHE_ENTRIES: usize = 1024;

pub fn plugin_error_response(err: &PluginError) -> Response {
    (
        err.status(),
        Json(serde_json::json!({ "error": err.code(), "message": err.to_string() })),
    )
        .into_response()
}

impl PluginService {
    /// Asks the owning plugin how to play `item`, validates the answer and caches it for
    /// `expires_in_secs` (per item and user). Never called for plugins that are missing,
    /// disabled, pending or unhealthy.
    pub async fn resolve_item(
        &self,
        item: &MediaItem,
        user_id: &str,
        client: ClientCapabilities,
    ) -> Result<PlaybackDescriptor, PluginError> {
        let Some(pref) = item.plugin_ref() else {
            return Err(PluginError::BadResponse(
                "item is not owned by a plugin".to_string(),
            ));
        };
        let settings = self.settings().await;
        if !settings.enabled {
            return Err(PluginError::Unavailable("plugins are disabled".to_string()));
        }
        let record = self
            .repo()
            .get(&pref.plugin_id)
            .await
            .map_err(|e| PluginError::Unavailable(format!("storage error: {e}")))?
            .ok_or_else(|| PluginError::Unavailable("plugin is not installed".to_string()))?;
        if record.status != PluginStatus::Enabled {
            return Err(PluginError::Unavailable(format!(
                "plugin is {}",
                record.status.as_str()
            )));
        }
        if !record.has_permission(PluginPermission::PlaybackResolve) {
            return Err(PluginError::Unavailable(
                "plugin lacks the playback:resolve permission".to_string(),
            ));
        }

        let key = (
            item.id.unwrap_or_default(),
            user_id.to_string(),
            client.clone(),
        );
        {
            let mut cache = self.cache().lock().await;
            if let Some(cached) = cache.get(&key) {
                if cached.expires_at > Instant::now() {
                    return Ok(cached.descriptor.clone());
                }
            }
            cache.remove(&key);
        }

        let request = ResolveRequest {
            remote_id: pref.remote_id,
            user_id: user_id.to_string(),
            client,
        };
        let descriptor = self
            .client()
            .resolve(
                &record.id,
                &record.base_url,
                &self.secret_for(&record.id),
                &request,
            )
            .await?;
        validate_descriptor(&descriptor, settings.allow_private_proxy)
            .await
            .map_err(PluginError::BadResponse)?;

        if let Some(ttl) = descriptor.expires_in_secs.filter(|t| *t > 0) {
            let now = Instant::now();
            let mut cache = self.cache().lock().await;
            if cache.len() >= MAX_CACHE_ENTRIES {
                cache.retain(|_, c| c.expires_at > now);
            }
            if cache.len() < MAX_CACHE_ENTRIES {
                cache.insert(
                    key,
                    CachedDescriptor {
                        descriptor: descriptor.clone(),
                        expires_at: now + Duration::from_secs(ttl),
                    },
                );
            }
        }
        Ok(descriptor)
    }
}

/// Direct-play endpoint behavior for plugin items.
pub async fn stream_plugin_item(
    svc: &PluginService,
    item: &MediaItem,
    user_id: &str,
    request_headers: &HeaderMap,
) -> Response {
    let descriptor = match svc
        .resolve_item(item, user_id, ClientCapabilities::default())
        .await
    {
        Ok(d) => d,
        Err(e) => return plugin_error_response(&e),
    };
    match descriptor.mode {
        PlaybackMode::Direct => {
            // Re-serialize through the URL parser so the header value is always valid ASCII.
            let location = reqwest::Url::parse(&descriptor.url)
                .ok()
                .and_then(|u| HeaderValue::from_str(u.as_str()).ok());
            match location {
                Some(value) => Response::builder()
                    .status(StatusCode::TEMPORARY_REDIRECT)
                    .header(header::LOCATION, value)
                    .body(Body::empty())
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
                None => plugin_error_response(&PluginError::BadResponse(
                    "descriptor url is not a valid location".to_string(),
                )),
            }
        }
        PlaybackMode::Transcode => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "PLAYBACK_REQUIRES_TRANSCODE",
                "hls_url": format!("/api/v1/media/{}/hls/master.m3u8", item.id.unwrap_or_default()),
            })),
        )
            .into_response(),
        PlaybackMode::Proxy => proxy_upstream(svc, item, &descriptor, request_headers).await,
    }
}

async fn proxy_upstream(
    svc: &PluginService,
    item: &MediaItem,
    descriptor: &PlaybackDescriptor,
    request_headers: &HeaderMap,
) -> Response {
    let mut upstream_req = svc.proxy_client().get(&descriptor.url);
    for (name, value) in &descriptor.headers {
        upstream_req = upstream_req.header(name, value);
    }
    if item.is_seekable() {
        if let Some(range) = request_headers.get(header::RANGE) {
            upstream_req = upstream_req.header(header::RANGE, range.clone());
        }
    }
    let upstream = match upstream_req.send().await {
        Ok(r) => r,
        Err(e) if e.is_timeout() => return plugin_error_response(&PluginError::Timeout),
        Err(e) => {
            return plugin_error_response(&PluginError::Unavailable(format!(
                "upstream request failed: {e}"
            )))
        }
    };

    let status = upstream.status();
    if !matches!(status.as_u16(), 200 | 206 | 416) {
        return plugin_error_response(&PluginError::BadResponse(format!(
            "upstream returned {status} (redirects are not followed)"
        )));
    }

    let content_type = descriptor
        .content_type
        .clone()
        .or_else(|| {
            upstream
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let mut builder = Response::builder()
        .status(status.as_u16())
        .header(header::CONTENT_TYPE, content_type);
    for name in [
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
    ] {
        if let Some(v) = upstream.headers().get(&name) {
            builder = builder.header(name, v.clone());
        }
    }

    let stream = futures::stream::unfold(upstream, |mut resp| async move {
        match resp.chunk().await {
            Ok(Some(bytes)) => Some((Ok::<_, std::io::Error>(bytes), resp)),
            Ok(None) => None,
            Err(e) => Some((Err(std::io::Error::other(e)), resp)),
        }
    });
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

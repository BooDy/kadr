use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::Extension;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures::stream::Stream;
use kadr_core::events::TelemetrySnapshot;
use tokio::sync::broadcast::error::RecvError;

use crate::auth::jwt::RequireAdmin;
use crate::events::EventBus;
use crate::telemetry::TelemetryCollector;

/// Handler for `GET /api/v1/events`.
///
/// Public SSE endpoint that streams real-time system events to connected clients.
/// Configured with 15-second keep-alive heartbeats and gracefully handles lagged subscribers.
pub async fn stream_events(
    Extension(event_bus): Extension<Arc<EventBus>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = event_bus.subscribe();

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(system_event) => {
                    let sse_event = match Event::default()
                        .event(system_event.event_type())
                        .json_data(&system_event)
                    {
                        Ok(evt) => evt,
                        Err(e) => {
                            tracing::error!("Failed to serialize system event to SSE JSON: {e}");
                            continue;
                        }
                    };
                    return Some((Ok(sse_event), rx));
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!("SSE subscriber lagged behind, skipped {skipped} events");
                    continue;
                }
                Err(RecvError::Closed) => {
                    tracing::debug!("EventBus channel closed, terminating SSE stream");
                    return None;
                }
            }
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// Handler for `GET /api/v1/system/telemetry`.
///
/// Protected admin REST endpoint that returns point-in-time system health and telemetry metrics.
/// Requires admin role (Bearer token or `?token=` query parameter).
pub async fn get_telemetry(
    _admin: RequireAdmin,
    Extension(collector): Extension<Arc<TelemetryCollector>>,
) -> (StatusCode, Json<TelemetrySnapshot>) {
    let snapshot = collector.collect_snapshot().await;
    (StatusCode::OK, Json(snapshot))
}

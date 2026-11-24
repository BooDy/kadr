use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Extension, Path, Query},
    http::{HeaderMap, StatusCode},
    Json,
};
use kadr_core::ast::{
    parse_hhmm_minutes, ResolvedLayoutScope, ScreenId, ScreenLayout, TargetDevice,
};
use kadr_core::events::SystemEvent;
use kadr_core::models::UserRole;
use serde::{Deserialize, Serialize};

use kadr_storage::repos::{LayoutOverrideRepository, LibraryRepository, UserRepository};

pub use crate::api::item_routes::get_item_details;
use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::{AuthUser, RequireAdmin};
use crate::config::AppConfig;
use crate::events::EventBus;
use crate::layout::{default_library_layout, LayoutRegistry};
use crate::resolver::{server_local_minutes, WidgetResolver};

type ApiError = (StatusCode, Json<serde_json::Value>);

/// Summary representation of a registered screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenSummary {
    pub id: String,
    pub title: String,
}

/// Query parameters for fetching screen layouts.
#[derive(Debug, Default, Deserialize)]
pub struct ScreenQuery {
    #[serde(default)]
    pub unhydrated: bool,
    pub target_device: Option<String>,
    pub local_time: Option<String>,
    pub scope_user_id: Option<String>,
}

/// Query parameters for saving or resetting/deleting screen layouts.
#[derive(Debug, Default, Deserialize)]
pub struct ScopeMutationQuery {
    pub scope_user_id: Option<String>,
    pub target_device: Option<String>,
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Resolves a requested screen slug or library UUID to its canonical `ScreenId`
/// (registering a default library screen layout if `requested_id` is an unmapped library UUID).
pub async fn resolve_canonical_screen_id(
    requested_id: &str,
    registry: &LayoutRegistry,
    lib_repo: &LibraryRepository,
) -> Option<ScreenId> {
    let parsed_id: ScreenId = requested_id.parse().unwrap();
    match registry.get_screen(&parsed_id) {
        Some(_) => {
            if let ScreenId::Custom(ref id_str) = parsed_id {
                if let Ok(Some(lib)) = lib_repo.get_by_id(id_str).await {
                    if let Some(canonical) = registry.find_screen_for_library(&lib.id, &lib.name) {
                        return Some(canonical.id);
                    }
                }
            }
            Some(parsed_id)
        }
        None => {
            if let Ok(Some(lib)) = lib_repo.get_by_id(requested_id).await {
                if let Some(canonical) = registry.find_screen_for_library(&lib.id, &lib.name) {
                    Some(canonical.id)
                } else {
                    let default_layout = default_library_layout(&lib.id, &lib.name);
                    let cid = default_layout.id.clone();
                    registry.register_screen(default_layout);
                    Some(cid)
                }
            } else {
                None
            }
        }
    }
}

/// Parses `target_device` from query parameter or `X-Kadr-Device` header, defaulting to `TargetDevice::All`.
/// Returns `(device, explicitly_provided)`.
pub(crate) fn parse_requested_target_device(
    query_device: Option<&str>,
    headers: &HeaderMap,
) -> Result<(TargetDevice, bool), ApiError> {
    let raw_opt = query_device
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get("x-kadr-device")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        });
    match raw_opt {
        Some(raw) => raw
            .parse::<TargetDevice>()
            .map(|dev| (dev, true))
            .map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": err })),
                )
            }),
        None => Ok((TargetDevice::All, false)),
    }
}

fn parse_requested_active_minutes(
    query_local_time: Option<&str>,
    headers: &HeaderMap,
) -> Result<u16, ApiError> {
    let raw_opt = query_local_time
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get("x-kadr-local-time")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        });
    match raw_opt {
        Some(raw) => parse_hhmm_minutes(raw).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid local_time '{raw}': expected HH:MM (00:00..23:59)")
                })),
            )
        }),
        None => Ok(server_local_minutes()),
    }
}

async fn resolve_read_scope_user_id(
    scope_user_id: Option<&str>,
    unhydrated: bool,
    auth_user: &AuthUser,
    user_repo: &UserRepository,
) -> Result<Option<String>, ApiError> {
    let cleaned = scope_user_id.map(str::trim).filter(|s| !s.is_empty());
    match cleaned {
        None => {
            if unhydrated && auth_user.role == UserRole::Admin {
                Ok(None)
            } else {
                Ok(Some(auth_user.id.clone()))
            }
        }
        Some(raw) if raw.eq_ignore_ascii_case("me") || raw == auth_user.id => {
            Ok(Some(auth_user.id.clone()))
        }
        Some(raw) if raw.eq_ignore_ascii_case("global") => {
            if auth_user.role != UserRole::Admin {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "Admin privileges required to inspect global layout scope"
                    })),
                ));
            }
            Ok(None)
        }
        Some(other_uid) => {
            if auth_user.role != UserRole::Admin {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "Admin privileges required to inspect another user's layout scope"
                    })),
                ));
            }
            match user_repo.get_by_id(other_uid).await {
                Ok(Some(u)) => Ok(Some(u.id)),
                Ok(None) => Err((
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Target user not found" })),
                )),
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )),
            }
        }
    }
}

async fn resolve_mutation_scope_user_id(
    scope_user_id: Option<&str>,
    auth_user: &AuthUser,
    user_repo: &UserRepository,
) -> Result<Option<String>, ApiError> {
    let raw = scope_user_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("global");

    if raw.eq_ignore_ascii_case("global") {
        if auth_user.role != UserRole::Admin {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Admin privileges required to modify global layouts"
                })),
            ));
        }
        Ok(None)
    } else if raw.eq_ignore_ascii_case("me") || raw == auth_user.id {
        Ok(Some(auth_user.id.clone()))
    } else {
        if auth_user.role != UserRole::Admin {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Admin privileges required to modify another user's layouts"
                })),
            ));
        }
        match user_repo.get_by_id(raw).await {
            Ok(Some(u)) => Ok(Some(u.id)),
            Ok(None) => Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Target user not found" })),
            )),
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )),
        }
    }
}

/// Handler for `GET /api/v1/screens`.
/// Returns a list of all registered screens with their IDs and titles, deduplicated.
pub async fn list_screens(
    _auth_user: AuthUser,
    Extension(registry): Extension<LayoutRegistry>,
    Extension(lib_repo): Extension<LibraryRepository>,
) -> Json<Vec<ScreenSummary>> {
    if let Ok(libs) = lib_repo.get_all().await {
        for lib in libs {
            if registry
                .find_screen_for_library(&lib.id, &lib.name)
                .is_none()
            {
                registry.register_screen(default_library_layout(&lib.id, &lib.name));
            }
        }
    }

    let mut screens: Vec<ScreenSummary> = Vec::new();
    let mut seen_titles = std::collections::HashSet::new();
    let mut seen_ids = std::collections::HashSet::new();

    for (id, title) in registry.list_screens() {
        let id_str = id.to_string();
        let norm_title = title.trim().to_lowercase();
        let norm_id = id_str.trim().to_lowercase();
        if !seen_titles.insert(norm_title) || !seen_ids.insert(norm_id) {
            continue;
        }
        screens.push(ScreenSummary { id: id_str, title });
    }

    Json(screens)
}

/// Validates that a screen ID is not empty, does not start with '.', and contains only ASCII alphanumeric, '-', or '_'.
pub fn validate_screen_id(id: &str) -> Result<(), &'static str> {
    if id.is_empty() {
        return Err("Screen ID cannot be empty");
    }
    if id.starts_with('.') {
        return Err("Screen ID cannot start with '.'");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Screen ID must contain only ASCII alphanumeric, '-', or '_' characters");
    }
    Ok(())
}

/// Handler for `GET /api/v1/screens/{screen_id}`.
/// Returns the hydrated (or raw if `unhydrated=true`) AST layout for the requested screen and scope.
#[allow(clippy::too_many_arguments)]
pub async fn get_screen(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    headers: HeaderMap,
    Path(screen_id): Path<String>,
    Query(query): Query<ScreenQuery>,
    Extension(registry): Extension<LayoutRegistry>,
    Extension(resolver): Extension<Arc<WidgetResolver>>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(user_repo): Extension<UserRepository>,
) -> Result<Json<ScreenLayout>, ApiError> {
    if let Err(err_msg) = validate_screen_id(&screen_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err_msg })),
        ));
    }

    let (target_device, _) =
        parse_requested_target_device(query.target_device.as_deref(), &headers)?;
    let active_minutes = parse_requested_active_minutes(query.local_time.as_deref(), &headers)?;
    let requested_user_id = resolve_read_scope_user_id(
        query.scope_user_id.as_deref(),
        query.unhydrated,
        &auth_user,
        &user_repo,
    )
    .await?;

    let canonical_id = resolve_canonical_screen_id(&screen_id, &registry, &lib_repo)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Screen not found" })),
            )
        })?;

    let layout = registry
        .resolve_scoped_screen(&canonical_id, requested_user_id.as_deref(), target_device)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Screen not found" })),
            )
        })?;

    if query.unhydrated {
        Ok(Json(layout))
    } else {
        let parsed_requested_id: ScreenId = screen_id.parse().unwrap();
        let unlocked_ids = unlocked.to_vec();
        let mut default_library_id = resolver
            .resolve_library_id_for_screen_with_registry(&parsed_requested_id, &registry)
            .await;
        if default_library_id.is_none() && parsed_requested_id != canonical_id {
            default_library_id = resolver
                .resolve_library_id_for_screen_with_registry(&canonical_id, &registry)
                .await;
        }
        let hydrated = resolver
            .resolve_screen_with_library_at(
                layout,
                &auth_user.id,
                &unlocked_ids,
                default_library_id.as_deref(),
                active_minutes,
            )
            .await;
        Ok(Json(hydrated))
    }
}

/// Request body for creating a custom screen layout.
#[derive(Debug, Deserialize)]
pub struct CreateScreenRequest {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Handler for `PUT /api/v1/screens/{screen_id}`.
/// Saves global or scoped `(user_id, target_device)` screen AST layouts with RBAC and AST validation.
#[allow(clippy::too_many_arguments)]
pub async fn save_screen_handler(
    auth_user: AuthUser,
    headers: HeaderMap,
    Path(screen_id): Path<String>,
    Query(query): Query<ScopeMutationQuery>,
    Extension(registry): Extension<LayoutRegistry>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(user_repo): Extension<UserRepository>,
    Extension(layout_override_repo): Extension<LayoutOverrideRepository>,
    Extension(config): Extension<Arc<tokio::sync::RwLock<AppConfig>>>,
    event_bus: Option<Extension<Arc<EventBus>>>,
    Json(mut screen): Json<ScreenLayout>,
) -> Result<Json<ScreenLayout>, ApiError> {
    if let Err(err_msg) = validate_screen_id(&screen_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err_msg })),
        ));
    }

    let (target_device, _) =
        parse_requested_target_device(query.target_device.as_deref(), &headers)?;
    let effective_user_id =
        resolve_mutation_scope_user_id(query.scope_user_id.as_deref(), &auth_user, &user_repo)
            .await?;

    let is_global_all = effective_user_id.is_none() && target_device == TargetDevice::All;
    let canonical_id = match resolve_canonical_screen_id(&screen_id, &registry, &lib_repo).await {
        Some(cid) => cid,
        None if is_global_all => screen_id.parse().unwrap(),
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Screen not found" })),
            ));
        }
    };

    for widget in &screen.widgets {
        if let Some(tree) = widget
            .binding()
            .and_then(|b| b.filters.as_ref())
            .and_then(|f| f.rule_tree.as_ref())
        {
            if let Err(err) = tree.validate() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Invalid filter rule tree in widget '{}': {err}", widget.id())
                    })),
                ));
            }
        }
        if let Some(tw) = widget.time_window() {
            if let Err(err) = tw.validate() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Invalid time_window in widget '{}': {err}", widget.id())
                    })),
                ));
            }
        }
    }

    screen.id = canonical_id.clone();
    screen.strip_runtime_fields();
    let now = now_epoch_secs();

    if is_global_all {
        let screens_dir = config.read().await.server.data_dir.join("screens");
        registry
            .save_screen(screen.clone(), &screens_dir)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?;
    } else {
        layout_override_repo
            .upsert(
                &canonical_id.to_string(),
                effective_user_id.as_deref(),
                target_device,
                &screen,
                now,
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?;
        registry.set_scoped_override(
            canonical_id.clone(),
            effective_user_id.clone(),
            target_device,
            screen.clone(),
        );
    }

    if let Some(Extension(bus)) = event_bus {
        bus.publish(SystemEvent::LayoutChanged {
            screen_id: canonical_id.to_string(),
            user_id: effective_user_id.clone(),
            target_device: Some(target_device),
            timestamp: now,
        });
    }

    screen.resolved_scope = Some(ResolvedLayoutScope {
        user_id: effective_user_id,
        target_device,
        is_exact_override: true,
    });
    Ok(Json(screen))
}

/// Handler for `POST /api/v1/screens`.
/// Creates a new screen AST layout and persists it. Requires admin role.
pub async fn create_screen_handler(
    _admin: RequireAdmin,
    Extension(registry): Extension<LayoutRegistry>,
    Extension(config): Extension<Arc<tokio::sync::RwLock<AppConfig>>>,
    Json(payload): Json<CreateScreenRequest>,
) -> Result<(StatusCode, Json<ScreenLayout>), ApiError> {
    if let Err(err_msg) = validate_screen_id(&payload.id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err_msg })),
        ));
    }

    let parsed_id: ScreenId = payload.id.parse().unwrap();
    if registry.get_screen(&parsed_id).is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": format!("Screen '{}' already exists", payload.id) })),
        ));
    }
    let screen = ScreenLayout::new(parsed_id, payload.title, vec![]);
    let screens_dir = config.read().await.server.data_dir.join("screens");
    registry
        .save_screen(screen.clone(), &screens_dir)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;
    Ok((StatusCode::CREATED, Json(screen)))
}

/// Handler for `DELETE /api/v1/screens/{screen_id}`.
/// Resets scoped overrides, resets built-in/library screens to factory defaults, or deletes custom screens.
#[allow(clippy::too_many_arguments)]
pub async fn delete_screen_handler(
    auth_user: AuthUser,
    headers: HeaderMap,
    Path(screen_id): Path<String>,
    Query(query): Query<ScopeMutationQuery>,
    Extension(registry): Extension<LayoutRegistry>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(user_repo): Extension<UserRepository>,
    Extension(layout_override_repo): Extension<LayoutOverrideRepository>,
    Extension(config): Extension<Arc<tokio::sync::RwLock<AppConfig>>>,
    event_bus: Option<Extension<Arc<EventBus>>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Err(err_msg) = validate_screen_id(&screen_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err_msg })),
        ));
    }

    let (target_device, _) =
        parse_requested_target_device(query.target_device.as_deref(), &headers)?;
    let effective_user_id =
        resolve_mutation_scope_user_id(query.scope_user_id.as_deref(), &auth_user, &user_repo)
            .await?;

    let canonical_id = resolve_canonical_screen_id(&screen_id, &registry, &lib_repo)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Screen not found" })),
            )
        })?;

    let now = now_epoch_secs();
    let is_global_all = effective_user_id.is_none() && target_device == TargetDevice::All;

    if is_global_all {
        let screens_dir = config.read().await.server.data_dir.join("screens");
        let response_msg = match canonical_id {
            ScreenId::Home | ScreenId::Movies | ScreenId::Shows => {
                registry
                    .reset_screen(&canonical_id, &screens_dir)
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({ "error": e })),
                        )
                    })?;
                "Screen reset to defaults"
            }
            ScreenId::Custom(ref id_str) => {
                if let Ok(Some(lib)) = lib_repo.get_by_id(id_str).await {
                    registry
                        .reset_library_screen(&lib.id, &lib.name, &screens_dir)
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({ "error": e })),
                            )
                        })?;
                    "Screen reset to defaults"
                } else {
                    if registry.get_screen(&canonical_id).is_none() {
                        return Err((
                            StatusCode::NOT_FOUND,
                            Json(serde_json::json!({ "error": "Screen not found" })),
                        ));
                    }
                    registry
                        .delete_custom_screen(&canonical_id, &screens_dir)
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({ "error": e })),
                            )
                        })?;
                    let _ = layout_override_repo
                        .delete_all_for_screen(&canonical_id.to_string())
                        .await;
                    registry.remove_all_overrides_for_screen(&canonical_id);
                    "Screen deleted"
                }
            }
        };

        if let Some(Extension(bus)) = event_bus {
            bus.publish(SystemEvent::LayoutChanged {
                screen_id: canonical_id.to_string(),
                user_id: None,
                target_device: Some(TargetDevice::All),
                timestamp: now,
            });
        }

        Ok(Json(serde_json::json!({ "message": response_msg })))
    } else {
        layout_override_repo
            .delete(
                &canonical_id.to_string(),
                effective_user_id.as_deref(),
                target_device,
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?;
        registry.remove_scoped_override(&canonical_id, effective_user_id.as_deref(), target_device);

        if let Some(Extension(bus)) = event_bus {
            bus.publish(SystemEvent::LayoutChanged {
                screen_id: canonical_id.to_string(),
                user_id: effective_user_id,
                target_device: Some(target_device),
                timestamp: now,
            });
        }

        Ok(Json(
            serde_json::json!({ "message": "Scoped layout override reset" }),
        ))
    }
}

use crate::plugins::auth::PluginAuth;
use crate::plugins::service::{PluginService, ServiceError};
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, post, put},
    Extension, Json, Router,
};
use kadr_core::events::SystemEvent;
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use kadr_core::plugin::{plugin_item_path, validate_remote_id, PluginPermission};
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_BATCH: usize = 1000;

#[derive(Deserialize)]
pub struct CreateLibraryBody {
    pub id: Option<String>,
    pub name: String,
    pub media_type: Option<MediaType>,
}

#[derive(Deserialize)]
pub struct PublishItem {
    pub remote_id: String,
    pub title: String,
    pub item_type: Option<MediaType>,
    pub original_title: Option<String>,
    pub release_year: Option<i32>,
    #[serde(default)]
    pub duration_seconds: i64,
    #[serde(default)]
    pub live: bool,
    pub seekable: Option<bool>,
    pub resolution: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub container: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub overview: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
}

#[derive(Deserialize)]
pub struct PublishBody {
    pub items: Vec<PublishItem>,
}

pub fn publish_routes() -> Router {
    Router::new()
        .route("/api/v1/plugin/libraries", post(create_library))
        .route("/api/v1/plugin/libraries/{id}/items", put(upsert_items))
        .route(
            "/api/v1/plugin/libraries/{id}/items/{remote_id}",
            delete(delete_item),
        )
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn valid_library_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Returns the library only when it exists and is owned by the calling plugin.
/// Other plugins' libraries and regular libraries look identical to "not found".
async fn owned_library(
    svc: &PluginService,
    auth: &PluginAuth,
    library_id: &str,
) -> Result<Library, ServiceError> {
    let not_found = || ServiceError::NotFound("library not found".to_string());
    match svc.libs().plugin_id_of(library_id).await? {
        Some(owner) if owner == auth.plugin_id => {}
        _ => return Err(not_found()),
    }
    svc.libs()
        .get_by_id(library_id)
        .await?
        .ok_or_else(not_found)
}

async fn create_library(
    auth: PluginAuth,
    Extension(svc): Extension<PluginService>,
    Json(body): Json<CreateLibraryBody>,
) -> Result<(StatusCode, Json<Value>), ServiceError> {
    svc.authorize(&auth, PluginPermission::ItemsPublish).await?;
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ServiceError::Invalid("name must not be empty".to_string()));
    }
    let id = body.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if !valid_library_id(&id) {
        return Err(ServiceError::Invalid(
            "library id must be 1-64 characters of [A-Za-z0-9_-]".to_string(),
        ));
    }
    if svc.libs().get_by_id(&id).await?.is_some() {
        return Err(ServiceError::Conflict(format!(
            "library '{id}' already exists"
        )));
    }
    let library = svc
        .libs()
        .create_virtual(
            &auth.plugin_id,
            &id,
            &name,
            body.media_type.unwrap_or(MediaType::Unknown),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": library.id,
            "name": library.name,
            "media_type": library.media_type,
        })),
    ))
}

fn build_item(
    plugin_id: &str,
    library: &Library,
    p: PublishItem,
) -> Result<MediaItem, ServiceError> {
    validate_remote_id(&p.remote_id).map_err(ServiceError::Invalid)?;
    let title = p.title.trim().to_string();
    if title.is_empty() {
        return Err(ServiceError::Invalid(format!(
            "title must not be empty (remote_id '{}')",
            p.remote_id
        )));
    }
    let mut metadata = MediaMetadata {
        overview: p.overview,
        genres: p.genres,
        ..Default::default()
    };
    // Artwork URLs are kept in `extra`, never in `poster_path`/`backdrop_path`, which the
    // artwork routes interpret as local files.
    if let Some(url) = p.poster_url {
        metadata.extra.insert("poster_url".into(), json!(url));
    }
    if let Some(url) = p.backdrop_url {
        metadata.extra.insert("backdrop_url".into(), json!(url));
    }
    if p.live {
        metadata.extra.insert("live".into(), json!(true));
    }
    if let Some(seekable) = p.seekable {
        metadata.extra.insert("seekable".into(), json!(seekable));
    }
    Ok(MediaItem {
        id: None,
        library_id: library.id.clone(),
        item_type: p.item_type.unwrap_or(library.media_type),
        title: title.clone(),
        original_title: p.original_title,
        release_year: p.release_year,
        added_at: now_secs(),
        file_path: plugin_item_path(plugin_id, &p.remote_id),
        file_name: title.replace('/', "_"),
        file_size: 0,
        technical: TechnicalInfo {
            duration_seconds: p.duration_seconds,
            resolution: p.resolution,
            video_codec: p.video_codec,
            audio_codec: p.audio_codec,
            audio_channels: None,
            container: p.container,
        },
        metadata,
    })
}

async fn upsert_items(
    auth: PluginAuth,
    Path(library_id): Path<String>,
    Extension(svc): Extension<PluginService>,
    Json(body): Json<PublishBody>,
) -> Result<Json<Value>, ServiceError> {
    svc.authorize(&auth, PluginPermission::ItemsPublish).await?;
    let library = owned_library(&svc, &auth, &library_id).await?;
    if body.items.len() > MAX_BATCH {
        return Err(ServiceError::Invalid(format!(
            "at most {MAX_BATCH} items per request"
        )));
    }
    let items = body
        .items
        .into_iter()
        .map(|p| build_item(&auth.plugin_id, &library, p))
        .collect::<Result<Vec<_>, _>>()?;
    let upserted = svc.media().upsert_batch(&items).await?;
    let total = svc.media().count_by_library(&library_id).await?;
    svc.events().publish(SystemEvent::LibraryUpdated {
        library_id,
        item_count: total,
        timestamp: now_secs(),
    });
    Ok(Json(json!({ "upserted": upserted })))
}

async fn delete_item(
    auth: PluginAuth,
    Path((library_id, remote_id)): Path<(String, String)>,
    Extension(svc): Extension<PluginService>,
) -> Result<StatusCode, ServiceError> {
    svc.authorize(&auth, PluginPermission::ItemsPublish).await?;
    owned_library(&svc, &auth, &library_id).await?;
    let path = plugin_item_path(&auth.plugin_id, &remote_id);
    match svc.media().find_by_path(&path).await? {
        Some(item) if item.library_id == library_id => {
            svc.media().delete_by_path(&path).await?;
            let total = svc.media().count_by_library(&library_id).await?;
            svc.events().publish(SystemEvent::LibraryUpdated {
                library_id,
                item_count: total,
                timestamp: now_secs(),
            });
            Ok(StatusCode::NO_CONTENT)
        }
        _ => Err(ServiceError::NotFound("item not found".to_string())),
    }
}

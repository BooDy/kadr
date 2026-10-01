use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Query;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole};
use kadr_core::subtitles::{SubtitleFormat, SubtitleSource, SubtitleTrack};
use kadr_server::api::create_router_with_subtitles;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::tempdir;
use tower::ServiceExt;

const SAMPLE_SRT: &str = "\
1
00:00:01,000 --> 00:00:04,000
Hello, world!

2
00:00:05,000 --> 00:00:08,000
This is a test subtitle.
";

struct TestContext {
    app: Router,
    token: String,
    media_item_id: i64,
    sidecar_subtitle_id: i64,
    _temp_dir: tempfile::TempDir,
    subtitle_service: Arc<SubtitleDeliveryService>,
    _opensubtitles_client: Arc<OpenSubtitlesClient>,
}

async fn setup_test_context(mock_base_url: Option<String>) -> TestContext {
    let pool = create_in_memory_pool().expect("failed to create pool");
    initialize_database(&pool).await.expect("failed to initialize db");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let admin = User {
        id: "admin-1".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1000,
    };
    user_repo.create(&admin).await.unwrap();

    let jwt_svc = JwtService::new("super-secret-key-that-is-at-least-32-bytes-long", 3600);
    let token = jwt_svc.generate_token(&admin).unwrap();

    lib_repo
        .create(&Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/tmp/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
        })
        .await
        .unwrap();

    let temp_dir = tempdir().expect("tempdir failed");
    let media_dir = temp_dir.path().join("media");
    std::fs::create_dir_all(&media_dir).unwrap();
    let video_path = media_dir.join("movie.mp4");
    File::create(&video_path).unwrap();

    let sidecar_srt_path = media_dir.join("movie.en.srt");
    let mut srt_file = File::create(&sidecar_srt_path).unwrap();
    srt_file.write_all(SAMPLE_SRT.as_bytes()).unwrap();

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Inception".to_string(),
            original_title: None,
            release_year: Some(2010),
            added_at: 1000,
            file_path: video_path,
            file_name: "movie.mp4".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
        }])
        .await
        .unwrap();

    let items = media_repo.list_by_library("lib1", 10, 0).await.unwrap();
    let media_item_id = items[0].id.unwrap();

    let sidecar_subtitle_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Sidecar,
            language: "eng".to_string(),
            title: Some("English [SDH]".to_string()),
            format: SubtitleFormat::Srt,
            file_path: Some(sidecar_srt_path.to_str().unwrap().to_string()),
            stream_index: None,
            is_default: true,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    let cache_dir = temp_dir.path().join("cache");
    let data_dir = temp_dir.path().join("data");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();

    let subtitle_service = Arc::new(
        SubtitleDeliveryService::new(cache_dir, subtitle_repo, media_repo.clone())
            .with_data_dir(data_dir),
    );

    let opensubtitles_client = match mock_base_url {
        Some(url) => Arc::new(OpenSubtitlesClient::new(Some("test-api-key".to_string()), Some(url))),
        None => Arc::new(OpenSubtitlesClient::new(None, None)),
    };

    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));
    let limiter = RateLimiter::new(100, Duration::from_secs(300), Duration::from_secs(300));
    let session_registry = Arc::new(SessionRegistry::new());

    let app = create_router_with_subtitles(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
        layout_registry,
        widget_resolver,
        subtitle_service.clone(),
        opensubtitles_client.clone(),
    );

    TestContext {
        app,
        token,
        media_item_id,
        sidecar_subtitle_id,
        _temp_dir: temp_dir,
        subtitle_service,
        _opensubtitles_client: opensubtitles_client,
    }
}

#[tokio::test]
async fn test_list_subtitles_authenticated() {
    let ctx = setup_test_context(None).await;

    // 1. Unauthenticated -> 401
    let req = Request::builder()
        .uri(format!("/api/v1/items/{}/subtitles", ctx.media_item_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 2. Non-existent media item -> 404
    let req = Request::builder()
        .uri("/api/v1/items/999999/subtitles")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let err_json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err_json["error"], "Media item not found");

    // 3. Valid media item -> 200 with subtitle list
    let req = Request::builder()
        .uri(format!("/api/v1/items/{}/subtitles", ctx.media_item_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let tracks: Value = serde_json::from_slice(&bytes).unwrap();
    let list = tracks.as_array().expect("expected array");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], ctx.sidecar_subtitle_id);
    assert_eq!(list[0]["media_item_id"], ctx.media_item_id);
    assert_eq!(list[0]["source"], "sidecar");
    assert_eq!(list[0]["language"], "eng");
    assert_eq!(list[0]["title"], "English [SDH]");
    assert_eq!(list[0]["format"], "srt");
    assert_eq!(list[0]["is_default"], true);
    assert_eq!(list[0]["is_forced"], false);
    assert_eq!(
        list[0]["stream_url"],
        format!("/api/v1/subtitles/{}/stream.vtt", ctx.sidecar_subtitle_id)
    );
}

#[tokio::test]
async fn test_stream_webvtt_public_and_headers() {
    let ctx = setup_test_context(None).await;

    // 1. Public streaming without Authorization header -> 200 OK
    let req = Request::builder()
        .uri(format!("/api/v1/subtitles/{}/stream.vtt", ctx.sidecar_subtitle_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify response headers
    let content_type = res
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("missing Content-Type")
        .to_str()
        .unwrap();
    assert_eq!(content_type, "text/vtt; charset=utf-8");

    let cache_control = res
        .headers()
        .get(header::CACHE_CONTROL)
        .expect("missing Cache-Control")
        .to_str()
        .unwrap();
    assert_eq!(cache_control, "public, max-age=86400");

    let accept_ranges = res
        .headers()
        .get(header::ACCEPT_RANGES)
        .expect("missing Accept-Ranges")
        .to_str()
        .unwrap();
    assert_eq!(accept_ranges, "bytes");

    // Verify converted WebVTT body content
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let vtt_text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(vtt_text.starts_with("WEBVTT"));
    assert!(vtt_text.contains("00:00:01.000 --> 00:00:04.000"));
    assert!(vtt_text.contains("Hello, world!"));

    // 2. Missing / non-existent subtitle -> 404
    let req = Request::builder()
        .uri("/api/v1/subtitles/999999/stream.vtt")
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let err_json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err_json["error"], "Subtitle not found");
}

#[tokio::test]
async fn test_search_unconfigured() {
    let ctx = setup_test_context(None).await;

    // Unauthenticated -> 401
    let req = Request::builder()
        .uri(format!("/api/v1/subtitles/{}/search", ctx.media_item_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Non-existent item -> 404
    let req = Request::builder()
        .uri("/api/v1/subtitles/999999/search")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Unconfigured -> 200 with configured: false, matches: []
    let req = Request::builder()
        .uri(format!("/api/v1/subtitles/{}/search?languages=en,ar", ctx.media_item_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["configured"], false);
    assert_eq!(val["matches"].as_array().unwrap().len(), 0);
}

// Mock OpenSubtitles server helpers
#[derive(Clone, Default)]
struct MockOpenSubtitlesState {
    search_called: Arc<AtomicBool>,
    download_called: Arc<AtomicBool>,
}

#[derive(Deserialize)]
struct MockSearchQuery {
    query: String,
    year: Option<u32>,
    languages: Option<String>,
}

#[derive(Deserialize)]
struct MockDownloadReq {
    file_id: u64,
}

#[derive(Serialize)]
struct MockDownloadResp {
    link: String,
    file_name: String,
}

#[tokio::test]
async fn test_search_and_download_flow_with_mock_opensubtitles() {
    let mock_state = MockOpenSubtitlesState::default();
    let state_for_router = mock_state.clone();

    let mock_app = Router::new()
        .route(
            "/subtitles",
            get(
                move |headers: HeaderMap,
                      Query(params): Query<MockSearchQuery>| {
                    let st = state_for_router.clone();
                    async move {
                        assert_eq!(headers.get("Api-Key").unwrap(), "test-api-key");
                        assert_eq!(params.query, "Inception");
                        assert_eq!(params.year, Some(2010));
                        assert_eq!(params.languages.as_deref(), Some("en,ar"));
                        st.search_called.store(true, Ordering::SeqCst);

                        let body = serde_json::json!({
                            "data": [
                                {
                                    "id": "match-1",
                                    "attributes": {
                                        "language": "en",
                                        "release": "Inception.2010.1080p",
                                        "hearing_impaired": false,
                                        "download_count": 500,
                                        "ratings": 9.0,
                                        "format": "srt",
                                        "files": [
                                            {
                                                "file_id": 98765,
                                                "file_name": "Inception.srt"
                                            }
                                        ]
                                    }
                                }
                            ]
                        });
                        (StatusCode::OK, Json(body))
                    }
                },
            ),
        )
        .route(
            "/download",
            post({
                let st = mock_state.clone();
                move |headers: HeaderMap, Json(body): Json<MockDownloadReq>| {
                    let st = st.clone();
                    async move {
                        assert_eq!(headers.get("Api-Key").unwrap(), "test-api-key");
                        assert_eq!(body.file_id, 98765);
                        st.download_called.store(true, Ordering::SeqCst);

                        (
                            StatusCode::OK,
                            Json(MockDownloadResp {
                                link: "/files/downloaded-sub.srt".to_string(),
                                file_name: "downloaded-sub.srt".to_string(),
                            }),
                        )
                    }
                }
            }),
        )
        .route(
            "/files/downloaded-sub.srt",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                    SAMPLE_SRT,
                )
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, mock_app.into_make_service()).await.unwrap();
    });

    let mock_base_url = format!("http://{}", local_addr);
    let ctx = setup_test_context(Some(mock_base_url)).await;

    // 1. Search with configured mock client -> 200 with matches
    let req = Request::builder()
        .uri(format!("/api/v1/subtitles/{}/search?languages=en,ar", ctx.media_item_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["configured"], true);
    let matches = val["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["id"], "98765");
    assert_eq!(matches[0]["language"], "en");
    assert_eq!(matches[0]["download_count"], 500);

    // 2. Download subtitle -> 201 Created
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/subtitles/{}/download", ctx.media_item_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "file_id": "98765",
            "language": "en",
            "title": "Inception English Downloaded",
            "is_forced": false
        })).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let downloaded_track: Value = serde_json::from_slice(&bytes).unwrap();
    let new_track_id = downloaded_track["id"].as_i64().expect("expected track id");
    assert_eq!(downloaded_track["media_item_id"], ctx.media_item_id);
    assert_eq!(downloaded_track["source"], "downloaded");
    assert_eq!(downloaded_track["language"], "en");
    assert_eq!(downloaded_track["title"], "Inception English Downloaded");
    assert_eq!(downloaded_track["format"], "srt");
    assert_eq!(
        downloaded_track["stream_url"],
        format!("/api/v1/subtitles/{}/stream.vtt", new_track_id)
    );

    // Verify downloaded file saved to disk at <data_dir>/subtitles/{item_id}/{file_id}.srt
    let expected_saved_path = ctx
        .subtitle_service
        .data_dir()
        .join("subtitles")
        .join(ctx.media_item_id.to_string())
        .join("98765.srt");
    assert!(expected_saved_path.exists());
    let saved_content = tokio::fs::read_to_string(&expected_saved_path).await.unwrap();
    assert_eq!(saved_content, SAMPLE_SRT);

    // 3. Stream the newly downloaded subtitle via GET /api/v1/subtitles/:id/stream.vtt
    let req = Request::builder()
        .uri(format!("/api/v1/subtitles/{}/stream.vtt", new_track_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let vtt = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(vtt.starts_with("WEBVTT"));
    assert!(vtt.contains("Hello, world!"));

    // 4. Verify subtitle listing now contains both sidecar and downloaded tracks
    let req = Request::builder()
        .uri(format!("/api/v1/items/{}/subtitles", ctx.media_item_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let tracks: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(tracks.as_array().unwrap().len(), 2);

    // 5. Delete the downloaded track -> 200 OK
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/subtitles/{}", new_track_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let del_json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(del_json["deleted"], true);

    // Verify downloaded source file was deleted from disk
    assert!(!expected_saved_path.exists());

    // Deleting again -> 404
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/subtitles/{}", new_track_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Listing now has only 1 track again
    let req = Request::builder()
        .uri(format!("/api/v1/items/{}/subtitles", ctx.media_item_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let tracks: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(tracks.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_download_unconfigured_fails_with_bad_request() {
    let ctx = setup_test_context(None).await;

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/subtitles/{}/download", ctx.media_item_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "file_id": "12345",
            "language": "en"
        })).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["error"], "OpenSubtitles integration is not configured");
}

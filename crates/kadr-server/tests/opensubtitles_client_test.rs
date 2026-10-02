use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use kadr_core::subtitles::OnlineSubtitleMatch;
use kadr_server::subtitles::{OpenSubtitlesClient, OpenSubtitlesError};
use serde::{Deserialize, Serialize};

#[tokio::test]
async fn test_unconfigured_client_behavior() {
    let test_cases = vec![
        OpenSubtitlesClient::new(None, None),
        OpenSubtitlesClient::new(Some("".to_string()), None),
        OpenSubtitlesClient::new(Some("   ".to_string()), None),
    ];

    for client in test_cases {
        assert!(!client.is_configured());

        let search_res = client
            .search("Inception", Some(2010), &["en".to_string()])
            .await;
        assert!(search_res.is_ok());
        assert_eq!(search_res.unwrap(), Vec::<OnlineSubtitleMatch>::new());

        let download_res = client.download("19542031").await;
        assert!(matches!(
            download_res,
            Err(OpenSubtitlesError::NotConfigured)
        ));
    }
}

#[derive(Clone, Default)]
struct MockState {
    search_called: Arc<AtomicBool>,
    download_called: Arc<AtomicBool>,
    file_fetched: Arc<AtomicBool>,
}

#[derive(Deserialize)]
struct SearchQueryParams {
    query: String,
    year: Option<u32>,
    languages: Option<String>,
}

#[derive(Deserialize)]
struct DownloadReqBody {
    file_id: u64,
}

#[derive(Serialize)]
struct DownloadRespBody {
    link: String,
    file_name: String,
    requests: u32,
    remaining: u32,
    message: String,
}

#[tokio::test]
async fn test_configured_client_search_and_download() {
    let state = MockState::default();

    let app = Router::new()
        .route(
            "/subtitles",
            get(
                |headers: HeaderMap,
                 Query(params): Query<SearchQueryParams>,
                 State(st): State<MockState>| async move {
                    assert_eq!(headers.get("Api-Key").unwrap(), "test-api-key");
                    assert_eq!(
                        headers.get("User-Agent").unwrap(),
                        "Kadr Media Server v0.1.0"
                    );
                    assert_eq!(headers.get("Accept").unwrap(), "application/json");

                    assert_eq!(params.query, "Inception");
                    assert_eq!(params.year, Some(2010));
                    assert_eq!(params.languages.as_deref(), Some("en,ar"));

                    st.search_called.store(true, Ordering::SeqCst);

                    let body = serde_json::json!({
                        "total_pages": 1,
                        "total_count": 2,
                        "page": 1,
                        "data": [
                            {
                                "id": "sub_item_1",
                                "type": "subtitle",
                                "attributes": {
                                    "language": "en",
                                    "release": "Inception.2010.1080p.BluRay.x264",
                                    "hearing_impaired": false,
                                    "download_count": 1234,
                                    "ratings": 8.5,
                                    "files": [
                                        {
                                            "file_id": 19542031,
                                            "file_name": "Inception.2010.en.srt"
                                        }
                                    ]
                                }
                            },
                            {
                                "id": "sub_item_2",
                                "type": "subtitle",
                                "attributes": {
                                    "language": "ar",
                                    "release": "Inception.2010.Arabic.x264",
                                    "hearing_impaired": true,
                                    "download_count": 567,
                                    "ratings": 9.0,
                                    "files": [
                                        {
                                            "file_id": 19542032,
                                            "file_name": "Inception.2010.ar.srt"
                                        }
                                    ]
                                }
                            }
                        ]
                    });

                    (StatusCode::OK, Json(body)).into_response()
                },
            ),
        )
        .route(
            "/download",
            post(
                |headers: HeaderMap,
                 State(st): State<MockState>,
                 Json(body): Json<DownloadReqBody>| async move {
                    assert_eq!(headers.get("Api-Key").unwrap(), "test-api-key");
                    assert_eq!(
                        headers.get("User-Agent").unwrap(),
                        "Kadr Media Server v0.1.0"
                    );
                    assert_eq!(headers.get("Accept").unwrap(), "application/json");
                    assert_eq!(body.file_id, 19542031);

                    st.download_called.store(true, Ordering::SeqCst);

                    let resp = DownloadRespBody {
                        link: "/files/Inception.2010.en.srt".to_string(),
                        file_name: "Inception.2010.en.srt".to_string(),
                        requests: 1,
                        remaining: 19,
                        message: "Your download will start shortly".to_string(),
                    };

                    (StatusCode::OK, Json(resp)).into_response()
                },
            ),
        )
        .route(
            "/files/Inception.2010.en.srt",
            get(|State(st): State<MockState>| async move {
                st.file_fetched.store(true, Ordering::SeqCst);
                "1\n00:00:01,000 --> 00:00:04,000\nWe need to go deeper.\n"
            }),
        )
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = OpenSubtitlesClient::new(Some("test-api-key".to_string()), Some(base_url));
    assert!(client.is_configured());

    // Test search
    let matches = client
        .search(
            "Inception",
            Some(2010),
            &["en".to_string(), "ar".to_string()],
        )
        .await
        .unwrap();

    assert!(state.search_called.load(Ordering::SeqCst));
    assert_eq!(matches.len(), 2);

    assert_eq!(
        matches[0],
        OnlineSubtitleMatch {
            id: "19542031".to_string(),
            language: "en".to_string(),
            release_name: Some("Inception.2010.1080p.BluRay.x264".to_string()),
            hearing_impaired: false,
            format: "srt".to_string(),
            download_count: 1234,
            rating: Some(8.5),
        }
    );

    assert_eq!(
        matches[1],
        OnlineSubtitleMatch {
            id: "19542032".to_string(),
            language: "ar".to_string(),
            release_name: Some("Inception.2010.Arabic.x264".to_string()),
            hearing_impaired: true,
            format: "srt".to_string(),
            download_count: 567,
            rating: Some(9.0),
        }
    );

    // Test download
    let (bytes, file_name) = client.download("19542031").await.unwrap();

    assert!(state.download_called.load(Ordering::SeqCst));
    assert!(state.file_fetched.load(Ordering::SeqCst));
    assert_eq!(file_name, "Inception.2010.en.srt");
    assert_eq!(
        bytes,
        b"1\n00:00:01,000 --> 00:00:04,000\nWe need to go deeper.\n"
    );
}

#[tokio::test]
async fn test_configured_client_api_error() {
    let app = Router::new().route(
        "/subtitles",
        get(|| async { (StatusCode::UNAUTHORIZED, "Invalid API key") }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = OpenSubtitlesClient::new(Some("bad-key".to_string()), Some(base_url));
    let err = client.search("Inception", None, &[]).await.unwrap_err();

    match err {
        OpenSubtitlesError::Api { status, message } => {
            assert_eq!(status, 401);
            assert_eq!(message, "Invalid API key");
        }
        other => panic!("expected Api error, got {other:?}"),
    }
}

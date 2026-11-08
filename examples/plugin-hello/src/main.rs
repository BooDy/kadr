use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

#[derive(Clone)]
struct Config {
    secret: Option<String>,
}

const SAMPLES: &[(&str, &str, &str)] = &[
    (
        "big-buck-bunny",
        "Big Buck Bunny",
        "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4",
    ),
    (
        "elephants-dream",
        "Elephants Dream",
        "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ElephantsDream.mp4",
    ),
    (
        "sintel",
        "Sintel",
        "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/Sintel.mp4",
    ),
];

fn authorized(cfg: &Config, headers: &HeaderMap) -> bool {
    let Some(secret) = &cfg.secret else {
        return false;
    };
    headers.get("authorization").and_then(|h| h.to_str().ok())
        == Some(format!("Bearer {secret}").as_str())
}

async fn manifest() -> Json<Value> {
    Json(json!({
        "id": "hello",
        "name": "Hello Plugin",
        "version": "0.1.0",
        "api_version": 1,
        "permissions": ["items:publish", "playback:resolve"],
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    }))
}

async fn health(State(cfg): State<Config>, headers: HeaderMap) -> StatusCode {
    if authorized(&cfg, &headers) {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}

async fn resolve(
    State(cfg): State<Config>,
    headers: HeaderMap,
    Json(req): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if !authorized(&cfg, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let remote_id = req["remote_id"].as_str().unwrap_or_default();
    let (_, _, url) = SAMPLES
        .iter()
        .find(|(id, _, _)| *id == remote_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(
        json!({ "mode": "direct", "url": url, "expires_in_secs": 300 }),
    ))
}

async fn publish(kadr_url: &str, token: &str) -> Result<(), reqwest::Error> {
    let http = reqwest::Client::new();
    // 201 on first run, 409 afterwards: both are fine.
    http.post(format!("{kadr_url}/api/v1/plugin/libraries"))
        .bearer_auth(token)
        .json(&json!({ "id": "hello-videos", "name": "Hello Videos", "media_type": "movie" }))
        .send()
        .await?;
    let items: Vec<Value> = SAMPLES
        .iter()
        .map(|(id, title, _)| json!({ "remote_id": id, "title": title, "duration_seconds": 600 }))
        .collect();
    let resp = http
        .put(format!(
            "{kadr_url}/api/v1/plugin/libraries/hello-videos/items"
        ))
        .bearer_auth(token)
        .json(&json!({ "items": items }))
        .send()
        .await?;
    println!("published {} items: HTTP {}", SAMPLES.len(), resp.status());
    Ok(())
}

#[tokio::main]
async fn main() {
    let secret = std::env::var("KADR_PLUGIN_SECRET").ok();
    if secret.is_none() {
        eprintln!("KADR_PLUGIN_SECRET is unset: authenticated routes (/health, /playback/resolve) will reject requests until it is configured");
    }
    let bind = std::env::var("PLUGIN_BIND").unwrap_or_else(|_| "127.0.0.1:9100".to_string());
    let kadr_url =
        std::env::var("KADR_URL").unwrap_or_else(|_| "http://127.0.0.1:8492".to_string());

    if let Ok(token) = std::env::var("KADR_PLUGIN_TOKEN") {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if let Err(e) = publish(&kadr_url, &token).await {
                eprintln!("publish failed: {e}");
            }
        });
    }

    let app = Router::new()
        .route("/manifest.json", get(manifest))
        .route("/health", get(health))
        .route("/playback/resolve", post(resolve))
        .with_state(Config { secret });
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .expect("bind failed");
    println!("plugin-hello listening on http://{bind}");
    axum::serve(listener, app).await.expect("server failed");
}

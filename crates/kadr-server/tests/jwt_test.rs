use axum::{
    body::Body,
    extract::Extension,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use kadr_core::models::{User, UserRole};
use kadr_server::auth::jwt::{AuthUser, JwtService, RequireAdmin};
use tower::ServiceExt;

#[test]
fn test_jwt_encode_and_decode() {
    let service = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let user = User {
        id: "u1".to_string(),
        username: "boody".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Admin,
        created_at: 1000,
    };

    let token = service.generate_token(&user).unwrap();
    let claims = service.verify_token(&token).unwrap();

    assert_eq!(claims.sub, "u1");
    assert_eq!(claims.username, "boody");
    assert_eq!(claims.role, UserRole::Admin);
}

#[test]
fn test_jwt_invalid_signature() {
    let service1 = JwtService::new("key-one-that-is-at-least-32-bytes-long", 3600);
    let service2 = JwtService::new("key-two-that-is-at-least-32-bytes-long", 3600);
    let user = User {
        id: "u1".to_string(),
        username: "boody".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };

    let token = service1.generate_token(&user).unwrap();
    assert!(service2.verify_token(&token).is_err());
}

#[tokio::test]
async fn test_auth_user_extractor_bearer_header() {
    let service = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let user = User {
        id: "u1".to_string(),
        username: "boody".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    let token = service.generate_token(&user).unwrap();

    let app = Router::new()
        .route("/protected", get(|user: AuthUser| async move {
            format!("hello {}", user.username)
        }))
        .layer(Extension(service));

    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_user_extractor_query_param() {
    let service = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let user = User {
        id: "u2".to_string(),
        username: "streamer".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    let token = service.generate_token(&user).unwrap();

    let app = Router::new()
        .route("/stream", get(|user: AuthUser| async move {
            format!("stream for {}", user.id)
        }))
        .layer(Extension(service));

    let req = Request::builder()
        .uri(format!("/stream?token={}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_user_extractor_missing_token() {
    let service = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);

    let app = Router::new()
        .route("/protected", get(|_user: AuthUser| async { "ok" }))
        .layer(Extension(service));

    let req = Request::builder()
        .uri("/protected")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_user_extractor_invalid_token() {
    let service = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);

    let app = Router::new()
        .route("/protected", get(|_user: AuthUser| async { "ok" }))
        .layer(Extension(service));

    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", "Bearer invalid-token-string")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_require_admin_extractor() {
    let service = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let admin_user = User {
        id: "admin-1".to_string(),
        username: "admin".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Admin,
        created_at: 1000,
    };
    let standard_user = User {
        id: "std-1".to_string(),
        username: "standard".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };

    let admin_token = service.generate_token(&admin_user).unwrap();
    let standard_token = service.generate_token(&standard_user).unwrap();

    let app = Router::new()
        .route("/admin", get(|RequireAdmin(user): RequireAdmin| async move {
            format!("admin {}", user.username)
        }))
        .layer(Extension(service));

    // Admin should succeed
    let req = Request::builder()
        .uri("/admin")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Standard user should be forbidden (403)
    let req = Request::builder()
        .uri("/admin")
        .header("Authorization", format!("Bearer {}", standard_token))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn test_jwt_expired_token() {
    use jsonwebtoken::{encode, EncodingKey, Header};
    use kadr_core::models::AuthClaims;

    let secret = "my-secret-key-that-is-at-least-32-bytes-long";
    let service = JwtService::new(secret, 3600);

    // Create a token expired in 1970
    let claims = AuthClaims {
        sub: "u1".to_string(),
        username: "boody".to_string(),
        role: UserRole::Admin,
        exp: 10,
        iat: 0,
    };
    let expired_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap();

    let result = service.verify_token(&expired_token);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_auth_user_extractor_missing_jwt_service() {
    // App WITHOUT .layer(Extension(service))
    let app = Router::new().route("/protected", get(|_user: AuthUser| async { "ok" }));

    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", "Bearer some-token")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_auth_user_extractor_query_param_with_non_bearer_header() {
    let service = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let user = User {
        id: "u3".to_string(),
        username: "streamer3".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    let token = service.generate_token(&user).unwrap();

    let app = Router::new()
        .route("/stream", get(|user: AuthUser| async move { user.username }))
        .layer(Extension(service));

    // Request has non-Bearer auth header AND valid query token
    let req = Request::builder()
        .uri(format!("/stream?token={}", token))
        .header("Authorization", "Basic dXNlcjpwYXNz")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}


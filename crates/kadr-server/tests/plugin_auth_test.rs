use kadr_core::models::{User, UserRole};
use kadr_server::auth::jwt::JwtService;
use kadr_server::plugins::auth::{derive_plugin_secret, PluginTokenService};

const SECRET: &[u8] = b"test-secret-with-sufficient-length-for-hmac-sha256";

#[test]
fn derived_secret_is_deterministic_and_scoped() {
    let a1 = derive_plugin_secret(SECRET, "iptv");
    let a2 = derive_plugin_secret(SECRET, "iptv");
    let b = derive_plugin_secret(SECRET, "hello");
    let c = derive_plugin_secret(b"another-server-secret-value-0123456789", "iptv");
    assert_eq!(a1, a2);
    assert_ne!(a1, b);
    assert_ne!(a1, c);
    assert!(a1.len() >= 43, "256-bit value, base64url without padding");
    assert!(!a1.contains('='));
}

#[test]
fn plugin_token_roundtrip() {
    let svc = PluginTokenService::new(SECRET);
    let token = svc.issue("iptv").unwrap();
    let claims = svc.verify(&token).unwrap();
    assert_eq!(claims.plugin_id, "iptv");
    assert_eq!(claims.sub, "plugin:iptv");
}

#[test]
fn tampered_plugin_token_is_rejected() {
    let svc = PluginTokenService::new(SECRET);
    let token = svc.issue("iptv").unwrap();
    let mut chars: Vec<char> = token.chars().collect();
    let last = chars.len() - 1;
    chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
    let tampered: String = chars.into_iter().collect();
    assert!(svc.verify(&tampered).is_err());
}

#[test]
fn user_tokens_are_not_plugin_tokens_and_vice_versa() {
    let jwt = JwtService::new(std::str::from_utf8(SECRET).unwrap(), 3600);
    let plugin = PluginTokenService::new(SECRET);

    let user = User {
        id: "u1".into(),
        username: "admin".into(),
        pin_hash: String::new(),
        role: UserRole::Admin,
        created_at: 0,
    };
    let user_token = jwt.generate_token(&user).unwrap();
    let plugin_token = plugin.issue("iptv").unwrap();

    assert!(
        plugin.verify(&user_token).is_err(),
        "user token must not verify as plugin token"
    );
    assert!(
        jwt.verify_token(&plugin_token).is_err(),
        "plugin token must not verify as user token"
    );
}

#[tokio::test]
async fn plugin_auth_extractor_success_and_failure() {
    use axum::extract::FromRequestParts;
    use axum::http::Request;
    use kadr_server::plugins::auth::PluginAuth;

    let svc = PluginTokenService::new(SECRET);
    let token = svc.issue("iptv").unwrap();

    // 1. Missing header
    let (mut parts, _) = Request::builder().body(()).unwrap().into_parts();
    parts.extensions.insert(svc.clone());
    let res = PluginAuth::from_request_parts(&mut parts, &()).await;
    assert!(res.is_err());

    // 2. Missing service extension
    let (mut parts, _) = Request::builder()
        .header("authorization", format!("Bearer {token}"))
        .body(())
        .unwrap()
        .into_parts();
    let res = PluginAuth::from_request_parts(&mut parts, &()).await;
    assert!(res.is_err());

    // 3. Invalid token
    let (mut parts, _) = Request::builder()
        .header("authorization", "Bearer invalid-token")
        .body(())
        .unwrap()
        .into_parts();
    parts.extensions.insert(svc.clone());
    let res = PluginAuth::from_request_parts(&mut parts, &()).await;
    assert!(res.is_err());

    // 4. Valid token
    let (mut parts, _) = Request::builder()
        .header("authorization", format!("Bearer {token}"))
        .body(())
        .unwrap()
        .into_parts();
    parts.extensions.insert(svc.clone());
    let auth = PluginAuth::from_request_parts(&mut parts, &())
        .await
        .unwrap();
    assert_eq!(auth.plugin_id, "iptv");
}

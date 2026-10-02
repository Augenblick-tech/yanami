//! 功能测试：HTTP 链路（真实路由 + Bearer 鉴权中间件 + 处理器 + 查询 + SQLite）。
//!
//! 请求通过 `tower::ServiceExt::oneshot` 直连生产路由，账号用生产代码创建（真实 argon2 哈希），
//! 令牌走真实登录接口签发与校验。

mod common;

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::{MockDownloaderManager, TestApp, nyaa_item, sword_anime};
use serde_json::{Value, json};
use tower::ServiceExt;
use user::entity::model::UserRole;

const PASSWORD: &str = "test-password-123456";

/// 用生产代码建账号（`Users::create` 会做真实 argon2 哈希），返回 (用户 id, space_id)。
async fn create_account(app: &TestApp, username: &str, role: UserRole) -> (i64, i64) {
    let users = app.users(Arc::new(MockDownloaderManager::new(false)));
    let user = users
        .create(username, PASSWORD, role, false)
        .await
        .expect("create account failed");
    (user.id(), user.space_id())
}

async fn request(app: &TestApp, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.router().oneshot(req).await.expect("router request failed");
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("read response body failed");
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

fn json_request(method: &str, uri: &str, body: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).expect("build request failed")
}

/// 登录并取出访问令牌。
async fn login_token(app: &TestApp, username: &str) -> String {
    let req = json_request(
        "POST",
        "/api/v1/user/login",
        &json!({"username": username, "password": PASSWORD}).to_string(),
        None,
    );
    let (status, body) = request(app, req).await;
    assert_eq!(status, StatusCode::OK, "login should succeed, body {body}");
    body["data"]["access_token"]
        .as_str()
        .expect("login response should carry an access token")
        .to_string()
}

#[tokio::test]
async fn web_api_login_returns_token_for_created_user() {
    let app = TestApp::new().await;
    let (user_id, _) = create_account(&app, "api-admin", UserRole::Admin).await;

    let req = json_request(
        "POST",
        "/api/v1/user/login",
        &json!({"username": "api-admin", "password": PASSWORD}).to_string(),
        None,
    );
    let (status, body) = request(&app, req).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], json!(200));
    assert_eq!(body["data"]["user_id"], json!(user_id));
    assert_eq!(body["data"]["role"], json!(1));
    assert!(!login_token(&app, "api-admin").await.is_empty());
}

#[tokio::test]
async fn web_api_login_rejects_wrong_password() {
    let app = TestApp::new().await;
    create_account(&app, "api-wrong", UserRole::Admin).await;

    let req = json_request(
        "POST",
        "/api/v1/user/login",
        &json!({"username": "api-wrong", "password": "not-the-password"}).to_string(),
        None,
    );
    let (status, body) = request(&app, req).await;

    assert!(!status.is_success(), "login should fail with a wrong password");
    assert_ne!(body["code"], json!(200));
}

#[tokio::test]
async fn web_api_rejects_request_without_token() {
    let app = TestApp::new().await;

    let (status, _) = request(&app, json_request("POST", "/api/v1/anime", "{}", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "missing Authorization header should be 401");

    let (status, _) = request(
        &app,
        json_request("POST", "/api/v1/anime", "{}", Some("not-a-jwt")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "invalid token should be 401");
}

#[tokio::test]
async fn web_api_forbids_non_admin_on_admin_route() {
    let app = TestApp::new().await;
    create_account(&app, "api-user", UserRole::User).await;

    let token = login_token(&app, "api-user").await;
    let (status, _) = request(
        &app,
        json_request("POST", "/api/v1/feed", "{}", Some(&token)),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "non-admin user should not create a feed");
}

#[tokio::test]
async fn web_api_lists_rules_of_logged_in_space() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "rule-owner", UserRole::User).await;
    app.seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;

    let token = login_token(&app, "rule-owner").await;
    let (status, body) = request(
        &app,
        json_request("GET", "/api/v1/rule", "", Some(&token)),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], json!(200));
    let items = body["data"].as_array().expect("rule list should be an array");
    assert_eq!(items.len(), 1, "only rules of the current space should be returned");
    assert!(
        body.to_string().contains("kr-rule"),
        "response should contain the rule name, actual {body}"
    );
}

#[tokio::test]
async fn web_api_lists_episodes_of_subscription() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "ep-owner", UserRole::User).await;

    let anime = app.seed_anime(&sword_anime()).await;
    app.seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;
    app.save_feed_items(vec![nyaa_item("전생했더니")]).await;
    app.match_all_resources(&sub_anime).await;

    let token = login_token(&app, "ep-owner").await;
    let (status, body) = request(
        &app,
        json_request(
            "GET",
            &format!("/api/v1/subscription/{}/episode", sub_anime.id()),
            "",
            Some(&token),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let eps = body["data"].as_array().expect("episode list should be an array");
    assert_eq!(eps.len(), 1, "the matched episode should be readable through the api");
    assert!(
        body.to_string().contains("전생했더니"),
        "response should contain the real item title, actual {body}"
    );
}

#[tokio::test]
async fn web_api_pages_subscribed_anime() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "anime-viewer", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    app.subscribe(space_id, anime.data.id).await;

    let token = login_token(&app, "anime-viewer").await;
    let (status, body) = request(
        &app,
        json_request(
            "POST",
            "/api/v1/anime",
            &json!({"page": 1, "page_size": 10}).to_string(),
            Some(&token),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "anime paging query failed, body {body}");
    assert_eq!(body["data"]["total"], json!(1));
    let list = body["data"]["data"].as_array().expect("anime list should be an array");
    assert_eq!(list.len(), 1, "subscribed anime should appear in the list");
    assert!(
        body.to_string().contains("転生したら剣でした"),
        "response should contain the real captured anime title, actual {body}"
    );
}

#[tokio::test]
async fn web_api_returns_system_stat() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "stat-viewer", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    app.subscribe(space_id, anime.data.id).await;

    let token = login_token(&app, "stat-viewer").await;
    let (status, body) = request(
        &app,
        json_request("GET", "/api/v1/stat", "", Some(&token)),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "system stat query failed, body {body}");
    assert_eq!(body["data"]["total_anime_count"], json!(1));
    assert_eq!(body["data"]["user_subscribed_count"], json!(1));
    assert!(body["data"]["quarter_stats"].is_array());
}

#[tokio::test]
async fn web_api_returns_not_found_for_unknown_route() {
    let app = TestApp::new().await;

    let (status, _) = request(&app, json_request("GET", "/api/v1/not-exist", "", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

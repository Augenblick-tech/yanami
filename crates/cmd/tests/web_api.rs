//! 功能测试：HTTP 链路（真实路由 + Bearer 鉴权中间件 + 处理器 + 查询 + SQLite）。
//!
//! 请求通过 `tower::ServiceExt::oneshot` 直连生产路由，账号用生产代码创建（真实 argon2 哈希），
//! 令牌走真实登录接口签发与校验。

mod common;

use std::sync::Arc;

use anime::entity::{
    cap::AnimeRepository,
    model::{AnimeIdType, AnimeListQuery, AnimeMetadata, AnimeSourceTarget},
};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::{MockDownloaderManager, TestApp, nyaa_item, sword_anime};
use serde_json::{Value, json};
use tower::ServiceExt;
use user::entity::model::UserRole;
use web::model::AnimeMetadataItem;

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
    let res = app
        .router()
        .oneshot(req)
        .await
        .expect("router request failed");
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
    builder
        .body(Body::from(body.to_string()))
        .expect("build request failed")
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

    assert!(
        !status.is_success(),
        "login should fail with a wrong password"
    );
    assert_ne!(body["code"], json!(200));
}

#[tokio::test]
async fn web_api_rejects_request_without_token() {
    let app = TestApp::new().await;

    let (status, _) = request(&app, json_request("POST", "/api/v1/anime", "{}", None)).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "missing Authorization header should be 401"
    );

    let (status, _) = request(
        &app,
        json_request("POST", "/api/v1/anime", "{}", Some("not-a-jwt")),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "invalid token should be 401"
    );
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

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "non-admin user should not create a feed"
    );
}

#[tokio::test]
async fn web_api_lists_rules_of_logged_in_space() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "rule-owner", UserRole::User).await;
    app.seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;

    let token = login_token(&app, "rule-owner").await;
    let (status, body) = request(&app, json_request("GET", "/api/v1/rule", "", Some(&token))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], json!(200));
    let items = body["data"]
        .as_array()
        .expect("rule list should be an array");
    assert_eq!(
        items.len(),
        1,
        "only rules of the current space should be returned"
    );
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
    let eps = body["data"]
        .as_array()
        .expect("episode list should be an array");
    assert_eq!(
        eps.len(),
        1,
        "the matched episode should be readable through the api"
    );
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

    assert_eq!(
        status,
        StatusCode::OK,
        "anime paging query failed, body {body}"
    );
    assert_eq!(body["data"]["total"], json!(1));
    let list = body["data"]["data"]
        .as_array()
        .expect("anime list should be an array");
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
    let (status, body) = request(&app, json_request("GET", "/api/v1/stat", "", Some(&token))).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "system stat query failed, body {body}"
    );
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

/// 手动添加番剧：请求体带回系列信息时，与同步链路一样在同一次落库写 `anime_series`。
#[tokio::test]
async fn web_api_create_anime_writes_series_row_from_request_body() {
    let app = TestApp::new().await;
    create_account(&app, "anime-creator", UserRole::Admin).await;
    let token = login_token(&app, "anime-creator").await;

    // 前端把 `bgm_info` 拿到的元数据原样回传：这里用同一套 DTO 构造同样的 body
    let metadata = sword_anime();
    let expected = metadata
        .series_metadata
        .clone()
        .expect("captured metadata should carry series info");
    let tmdb_id = match metadata
        .external_link
        .iter()
        .find(|link| link.target == AnimeSourceTarget::TMDB)
        .expect("captured metadata should carry a tmdb link")
        .id
        .clone()
    {
        AnimeIdType::Int(id) => id,
        AnimeIdType::String(_) => panic!("captured tmdb id should be an integer"),
    };

    let body = json!({
        "metadata": AnimeMetadataItem::from(metadata),
        "lock": false,
    })
    .to_string();

    let (status, response) = request(
        &app,
        json_request("POST", "/api/v1/anime/create", &body, Some(&token)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "anime create failed, body {response}"
    );

    let series = app
        .ctx
        .repo
        .anime_repo
        .find_series(tmdb_id)
        .await
        .expect("find series failed");
    let series = series.expect("create should write the anime_series row");
    assert_eq!(series.origin_name, expected.origin_name);
    assert_eq!(series.cn_name, expected.cn_name);
    assert_eq!(series.air_date, expected.air_date);
    assert_eq!(series.genres, expected.genres);

    let anime_id = response["data"]
        .as_i64()
        .expect("create should return the new anime id");
    let linked = app
        .ctx
        .repo
        .anime_repo
        .list_by_series(tmdb_id)
        .await
        .expect("list by series failed");
    assert_eq!(
        linked
            .iter()
            .filter(|item| item.data.id == anime_id)
            .count(),
        1,
        "the created anime should be linked to the series"
    );
}

/// 创建时就带锁：锁状态必须和番剧在同一个事务里一起落库。
#[tokio::test]
async fn web_api_create_anime_writes_lock_state_in_one_write() {
    let app = TestApp::new().await;
    create_account(&app, "anime-locker", UserRole::Admin).await;
    let token = login_token(&app, "anime-locker").await;

    let body = json!({
        "metadata": AnimeMetadataItem::from(sword_anime()),
        "lock": true,
    })
    .to_string();

    let (status, response) = request(
        &app,
        json_request("POST", "/api/v1/anime/create", &body, Some(&token)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "anime create failed, body {response}"
    );

    let anime_id = response["data"]
        .as_i64()
        .expect("create should return the new anime id");
    let stored = app
        .ctx
        .repo
        .anime_repo
        .find(anime_id)
        .await
        .expect("find anime failed")
        .expect("created anime should be stored");
    assert!(stored.data.lock, "lock should be persisted by the create");
}

/// 取夹具元数据里的 TMDB 系列 id。
fn tmdb_id_of(metadata: &AnimeMetadata) -> i64 {
    match metadata
        .external_link
        .iter()
        .find(|link| link.target == AnimeSourceTarget::TMDB)
        .expect("captured metadata should carry a tmdb link")
        .id
        .clone()
    {
        AnimeIdType::Int(id) => id,
        AnimeIdType::String(_) => panic!("captured tmdb id should be an integer"),
    }
}

/// 有 TMDB 系列身份却不带系列信息的创建请求属于非法数据：入库前就要被拦下。
#[tokio::test]
async fn web_api_create_anime_rejects_metadata_without_series() {
    let app = TestApp::new().await;
    create_account(&app, "anime-illegal", UserRole::Admin).await;
    let token = login_token(&app, "anime-illegal").await;

    let mut metadata = AnimeMetadataItem::from(sword_anime());
    metadata.series_metadata = None;
    let body = json!({ "metadata": metadata, "lock": false }).to_string();

    let (status, response) = request(
        &app,
        json_request("POST", "/api/v1/anime/create", &body, Some(&token)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "anime create without series metadata should be rejected, body {response}"
    );
    assert_eq!(response["code"], json!(400));

    let stored = app
        .ctx
        .repo
        .anime_repo
        .list(&AnimeListQuery {
            keyword: Some("転生したら剣でした".to_string()),
            ..Default::default()
        })
        .await
        .expect("list anime failed");
    assert!(
        stored.is_empty(),
        "rejected anime should not be stored, actual count {}",
        stored.len()
    );
}

/// 编辑同样拦截非法数据，并且不改动已落库的番剧与系列关联。
#[tokio::test]
async fn web_api_edit_anime_rejects_metadata_without_series() {
    let app = TestApp::new().await;
    create_account(&app, "anime-illegal-editor", UserRole::Admin).await;
    let token = login_token(&app, "anime-illegal-editor").await;
    let metadata = sword_anime();
    let tmdb_id = tmdb_id_of(&metadata);
    let seeded = app.seed_anime(&metadata).await;

    let mut illegal = AnimeMetadataItem::from(sword_anime());
    illegal.series_metadata = None;
    let body = json!({ "metadata": illegal }).to_string();

    let (status, response) = request(
        &app,
        json_request(
            "PUT",
            &format!("/api/v1/anime/{}", seeded.data.id),
            &body,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "anime edit without series metadata should be rejected, body {response}"
    );
    assert_eq!(response["code"], json!(400));

    let linked = app
        .ctx
        .repo
        .anime_repo
        .list_by_series(tmdb_id)
        .await
        .expect("list by series failed");
    assert_eq!(
        linked
            .iter()
            .filter(|item| item.data.id == seeded.data.id)
            .count(),
        1,
        "rejected edit should leave the stored anime linked to its series"
    );
}

/// 带回系列信息的合法编辑必须照常通过，并把系列信息写进 anime_series。
#[tokio::test]
async fn web_api_edit_anime_keeps_series_metadata_from_request_body() {
    let app = TestApp::new().await;
    create_account(&app, "anime-editor", UserRole::Admin).await;
    let token = login_token(&app, "anime-editor").await;
    let seeded = app.seed_anime(&sword_anime()).await;

    let mut edited = sword_anime();
    let tmdb_id = tmdb_id_of(&edited);
    let updated_cn_name = "转生就是剑 第二季".to_string();
    edited
        .series_metadata
        .as_mut()
        .expect("captured metadata should carry series info")
        .cn_name = updated_cn_name.clone();
    let body = json!({
        "metadata": AnimeMetadataItem::from(edited),
        "lock": false,
    })
    .to_string();

    let (status, response) = request(
        &app,
        json_request(
            "PUT",
            &format!("/api/v1/anime/{}", seeded.data.id),
            &body,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "anime edit failed, body {response}");

    let series = app
        .ctx
        .repo
        .anime_repo
        .find_series(tmdb_id)
        .await
        .expect("find series failed")
        .expect("edit should keep the anime_series row");
    assert_eq!(series.cn_name, updated_cn_name);
}

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
use subscription::entity::cap::{SearchMandateRepository, SubAnimeRepository};
use subscription::entity::model::Mandate;
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
async fn web_api_search_status_comes_from_mandate_list() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "search-viewer", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;

    // 订阅在搜索委托上：搜索中由委托算出来，订阅表里存的是不搜索
    app.ctx
        .repo
        .mandate_repo
        .save(
            anime.data.id,
            sub_anime.id(),
            &[Mandate {
                anime_id: anime.data.id,
                feed_id: 1,
                url: "http://feed/1".to_string(),
            }],
        )
        .await
        .expect("save search mandate failed");

    let token = login_token(&app, "search-viewer").await;
    let page = json!({"page": 1, "page_size": 10}).to_string();
    let (status, body) = request(
        &app,
        json_request("POST", "/api/v1/anime", &page, Some(&token)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "anime paging query failed, body {body}"
    );
    assert_eq!(
        body["data"]["data"][0]["sub_info"]["search_status"],
        json!(3),
        "subscription waiting on a mandate should read as searching, actual {body}"
    );

    // 订阅不再搜索：从搜索委托上去掉
    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/search_status", sub_anime.id()),
            &json!({"enable": false}).to_string(),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "cancel search failed, body {body}");

    let (_, body) = request(
        &app,
        json_request("POST", "/api/v1/anime", &page, Some(&token)),
    )
    .await;
    assert_eq!(
        body["data"]["data"][0]["sub_info"]["search_status"],
        json!(0),
        "subscription should read back to the status it holds, actual {body}"
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

/// 手动挑资源：按关键字调列表接口，取出命中的 40 位 hex 资源标识。
async fn search_resource_hashes(app: &TestApp, token: &str, keyword: &str) -> Vec<String> {
    let (status, body) = request(
        app,
        json_request(
            "POST",
            "/api/v1/resource/list",
            &json!({"keyword": keyword, "page": 1, "page_size": 10}).to_string(),
            Some(token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "resource list failed, body {body}");

    body["data"]["data"]
        .as_array()
        .expect("resource list should be an array")
        .iter()
        .map(|item| {
            item["info_hash"]
                .as_str()
                .expect("resource item should carry a hex info hash")
                .to_string()
        })
        .collect()
}

#[tokio::test]
async fn web_api_manual_match_routes_require_token() {
    let app = TestApp::new().await;

    for (method, uri) in [
        ("POST", "/api/v1/resource/list"),
        ("POST", "/api/v1/subscription/1/eps"),
        ("DELETE", "/api/v1/subscription/1/eps"),
    ] {
        let (status, body) = request(&app, json_request(method, uri, "{}", None)).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} without token should be 401, body {body}"
        );
    }
}

#[tokio::test]
async fn web_api_resource_list_matches_keyword_and_hides_magnet() {
    let app = TestApp::new().await;
    create_account(&app, "res-viewer", UserRole::User).await;
    app.save_feed_items(vec![nyaa_item("전생했더니"), nyaa_item("트릭컬")])
        .await;

    let token = login_token(&app, "res-viewer").await;
    let (status, body) = request(
        &app,
        json_request(
            "POST",
            "/api/v1/resource/list",
            &json!({"keyword": "전생했더니", "page": 1, "page_size": 10}).to_string(),
            Some(&token),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "resource list failed, body {body}");
    let items = body["data"]["data"]
        .as_array()
        .expect("resource list should be an array");
    assert_eq!(
        items.len(),
        1,
        "resource list should only return keyword hits, actual {body}"
    );
    assert!(
        items[0]["title"]
            .as_str()
            .expect("resource item should carry a title")
            .contains("전생했더니"),
        "resource list should return the matched resource, actual {body}"
    );
    let info_hash = items[0]["info_hash"]
        .as_str()
        .expect("resource item should carry a hex info hash");
    assert_eq!(
        info_hash.len(),
        40,
        "info_hash should be 40 hex characters, actual {info_hash}"
    );
    assert!(
        items[0].get("url").is_none(),
        "resource list should not carry the magnet url, actual {}",
        items[0]
    );
    assert_eq!(body["data"]["has_more"], json!(false));
}

#[tokio::test]
async fn web_api_resource_list_forbids_other_space_subscription() {
    let app = TestApp::new().await;
    let (_, owner_space) = create_account(&app, "res-owner", UserRole::User).await;
    create_account(&app, "res-intruder", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    let sub_anime = app.subscribe(owner_space, anime.data.id).await;

    let token = login_token(&app, "res-intruder").await;
    let (status, body) = request(
        &app,
        json_request(
            "POST",
            "/api/v1/resource/list",
            &json!({"sub_anime_id": sub_anime.id()}).to_string(),
            Some(&token),
        ),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "resource list should reject a subscription of another space, body {body}"
    );
}

#[tokio::test]
async fn web_api_adds_and_deletes_eps_manually() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "manual-matcher", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    let rule_id = app
        .seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;
    app.save_feed_items(vec![nyaa_item("전생했더니")]).await;

    let token = login_token(&app, "manual-matcher").await;
    let info_hash = search_resource_hashes(&app, &token, "전생했더니")
        .await
        .into_iter()
        .next()
        .expect("keyword should match the captured resource");

    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/bind_rule", sub_anime.id()),
            &json!({"rule_id": rule_id}).to_string(),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "bind rule should succeed, body {body}"
    );

    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/eps", sub_anime.id()),
            &json!({"info_hashes": [info_hash.clone()]}).to_string(),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "add eps should succeed, body {body}"
    );

    let eps = app.list_eps(sub_anime.id()).await;
    assert_eq!(eps.len(), 1, "add eps should save the episode");
    assert!(
        eps[0].extend.title.contains("전생했더니"),
        "add eps should save the picked resource, actual {}",
        eps[0].extend.title
    );
    let ep_id = eps[0].data.id;
    let bound = app
        .ctx
        .repo
        .sub_anime_repo
        .find_sub_anime(sub_anime.id())
        .await
        .expect("find sub anime failed")
        .expect("sub anime should exist");
    assert_eq!(
        bound.data.rule_id,
        Some(rule_id),
        "add eps should bind the rule to the subscription"
    );
    assert_eq!(
        bound.data.progress, 1,
        "add eps should recompute progress from ep_num"
    );

    // 请求给的剧集 ID 要在这条订阅的剧集里认得出，否则什么都不删
    let (status, body) = request(
        &app,
        json_request(
            "DELETE",
            &format!("/api/v1/subscription/{}/eps", sub_anime.id()),
            &json!({"ep_ids": [ep_id + 1000]}).to_string(),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "delete eps with unknown ep id should succeed, body {body}"
    );
    assert_eq!(
        app.list_eps(sub_anime.id()).await.len(),
        1,
        "delete eps should keep the episode when the ep id is not in the subscription"
    );

    let (status, body) = request(
        &app,
        json_request(
            "DELETE",
            &format!("/api/v1/subscription/{}/eps", sub_anime.id()),
            &json!({"ep_ids": [ep_id]}).to_string(),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "delete eps should succeed, body {body}"
    );
    assert!(
        app.list_eps(sub_anime.id()).await.is_empty(),
        "delete eps should remove the episode"
    );
    let after_delete = app
        .ctx
        .repo
        .sub_anime_repo
        .find_sub_anime(sub_anime.id())
        .await
        .expect("find sub anime failed")
        .expect("sub anime should exist");
    assert_eq!(
        after_delete.data.progress, 0,
        "delete eps should recompute progress from the remaining episodes"
    );
}

#[tokio::test]
async fn web_api_add_eps_requires_binded_rule() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "rule-less-matcher", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;
    app.save_feed_items(vec![nyaa_item("전생했더니")]).await;

    let token = login_token(&app, "rule-less-matcher").await;
    let info_hash = search_resource_hashes(&app, &token, "전생했더니")
        .await
        .into_iter()
        .next()
        .expect("keyword should match the captured resource");

    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/eps", sub_anime.id()),
            &json!({"info_hashes": [info_hash]}).to_string(),
            Some(&token),
        ),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "add eps should reject a subscription without rule when rule_id is missing, body {body}"
    );
    assert_eq!(body["code"], json!(40901));
    assert!(
        app.list_eps(sub_anime.id()).await.is_empty(),
        "failed add should not leave episodes"
    );
}

#[tokio::test]
async fn web_api_add_eps_rejects_unknown_resource() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "ghost-matcher", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    let rule_id = app
        .seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;

    let token = login_token(&app, "ghost-matcher").await;
    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/eps", sub_anime.id()),
            &json!({
                "info_hashes": ["0000000000000000000000000000000000000000"],
                "rule_id": rule_id,
            })
            .to_string(),
            Some(&token),
        ),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "add eps should reject an unknown resource, body {body}"
    );
    assert_eq!(body["code"], json!(404));
    assert!(
        app.list_eps(sub_anime.id()).await.is_empty(),
        "failed add should not leave episodes"
    );
}

#[tokio::test]
async fn web_api_add_eps_rejects_rule_of_another_space() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "cross-rule-matcher", UserRole::User).await;
    let (_, other_space_id) = create_account(&app, "other-rule-owner", UserRole::User).await;
    let other_rule_id = app
        .seed_rule(other_space_id, "other-rule", "전생했더니")
        .await;
    let anime = app.seed_anime(&sword_anime()).await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;
    app.save_feed_items(vec![nyaa_item("전생했더니")]).await;

    let token = login_token(&app, "cross-rule-matcher").await;
    let info_hash = search_resource_hashes(&app, &token, "전생했더니")
        .await
        .into_iter()
        .next()
        .expect("keyword should match the captured resource");

    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/eps", sub_anime.id()),
            &json!({"info_hashes": [info_hash], "rule_id": other_rule_id}).to_string(),
            Some(&token),
        ),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "add eps should reject a rule of another space, body {body}"
    );
    assert!(
        app.list_eps(sub_anime.id()).await.is_empty(),
        "failed add should not leave episodes"
    );
    let bound = app
        .ctx
        .repo
        .sub_anime_repo
        .find_sub_anime(sub_anime.id())
        .await
        .expect("find sub anime failed")
        .expect("sub anime should exist");
    assert_eq!(
        bound.data.rule_id, None,
        "failed add should not bind the rule"
    );
}

#[tokio::test]
async fn web_api_add_eps_rejects_unknown_rule() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "no-rule-matcher", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;
    app.save_feed_items(vec![nyaa_item("전생했더니")]).await;

    let token = login_token(&app, "no-rule-matcher").await;
    let info_hash = search_resource_hashes(&app, &token, "전생했더니")
        .await
        .into_iter()
        .next()
        .expect("keyword should match the captured resource");

    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/eps", sub_anime.id()),
            &json!({"info_hashes": [info_hash], "rule_id": 987654}).to_string(),
            Some(&token),
        ),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "add eps should reject an unknown rule, body {body}"
    );
    assert_eq!(body["code"], json!(404));
    assert!(
        app.list_eps(sub_anime.id()).await.is_empty(),
        "failed add should not leave episodes"
    );
}

#[tokio::test]
async fn web_api_add_eps_binds_rule_when_subscription_has_none() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "rule-adding-matcher", UserRole::User).await;
    let rule_id = app
        .seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;
    let anime = app.seed_anime(&sword_anime()).await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;
    app.save_feed_items(vec![nyaa_item("전생했더니")]).await;

    let token = login_token(&app, "rule-adding-matcher").await;
    let info_hash = search_resource_hashes(&app, &token, "전생했더니")
        .await
        .into_iter()
        .next()
        .expect("keyword should match the captured resource");

    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/eps", sub_anime.id()),
            &json!({"info_hashes": [info_hash], "rule_id": rule_id}).to_string(),
            Some(&token),
        ),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "add eps should succeed with a rule to bind, body {body}"
    );
    assert_eq!(
        app.list_eps(sub_anime.id()).await.len(),
        1,
        "add eps should save the episode"
    );
    let added = app
        .ctx
        .repo
        .sub_anime_repo
        .find_sub_anime(sub_anime.id())
        .await
        .expect("find sub anime failed")
        .expect("sub anime should exist");
    assert_eq!(
        added.data.rule_id,
        Some(rule_id),
        "add eps should bind the given rule to a subscription without rule"
    );
}

#[tokio::test]
async fn web_api_add_eps_rejects_rule_other_than_binded() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "rebind-matcher", UserRole::User).await;
    let binded_rule_id = app
        .seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;
    let other_rule_id = app.seed_rule(space_id, "kr-rule-2", "전생했더니").await;
    let anime = app.seed_anime(&sword_anime()).await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;
    app.save_feed_items(vec![nyaa_item("전생했더니")]).await;

    let token = login_token(&app, "rebind-matcher").await;
    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/bind_rule", sub_anime.id()),
            &json!({"rule_id": binded_rule_id}).to_string(),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "bind rule should succeed, body {body}"
    );

    let info_hash = search_resource_hashes(&app, &token, "전생했더니")
        .await
        .into_iter()
        .next()
        .expect("keyword should match the captured resource");

    let (status, body) = request(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/subscription/{}/eps", sub_anime.id()),
            &json!({"info_hashes": [info_hash], "rule_id": other_rule_id}).to_string(),
            Some(&token),
        ),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "add eps should reject a rule other than the binded one, body {body}"
    );
    assert_eq!(body["code"], json!(40901));
    assert!(
        app.list_eps(sub_anime.id()).await.is_empty(),
        "failed add should not leave episodes"
    );
    let still_binded = app
        .ctx
        .repo
        .sub_anime_repo
        .find_sub_anime(sub_anime.id())
        .await
        .expect("find sub anime failed")
        .expect("sub anime should exist");
    assert_eq!(
        still_binded.data.rule_id,
        Some(binded_rule_id),
        "failed add should keep the binded rule"
    );
}

#[tokio::test]
async fn web_api_add_eps_rejects_invalid_info_hashes() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "bad-hash-matcher", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    let rule_id = app
        .seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;

    let token = login_token(&app, "bad-hash-matcher").await;
    let url = format!("/api/v1/subscription/{}/eps", sub_anime.id());
    let bodies = [
        json!({"info_hashes": [], "rule_id": rule_id}),
        json!({"info_hashes": ["not a hex string"], "rule_id": rule_id}),
        json!({"info_hashes": ["d18e3d4491ddae3e33cc23786ea264c1eac22"], "rule_id": rule_id}),
        json!({"info_hashes": vec!["d18e3d4491ddae3e33cc23786ea264c1eac225cc"; 101], "rule_id": rule_id}),
    ];

    for body in bodies {
        let (status, response) = request(
            &app,
            json_request("POST", &url, &body.to_string(), Some(&token)),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "add eps should reject {body}, body {response}"
        );
    }
    assert!(
        app.list_eps(sub_anime.id()).await.is_empty(),
        "failed add should not leave episodes"
    );
}

#[tokio::test]
async fn web_api_delete_eps_rejects_invalid_ep_ids() {
    let app = TestApp::new().await;
    let (_, space_id) = create_account(&app, "bad-ep-id", UserRole::User).await;
    let anime = app.seed_anime(&sword_anime()).await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;

    let token = login_token(&app, "bad-ep-id").await;
    let url = format!("/api/v1/subscription/{}/eps", sub_anime.id());
    let bodies = [
        json!({"ep_ids": []}),
        json!({"ep_ids": [0]}),
        json!({"ep_ids": [-1]}),
        json!({"ep_ids": vec![1_i64; 101]}),
    ];

    for body in bodies {
        let (status, response) = request(
            &app,
            json_request("DELETE", &url, &body.to_string(), Some(&token)),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "delete eps should reject {body}, body {response}"
        );
    }
}

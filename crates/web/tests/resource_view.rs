//! 手动挑资源用的列表查询口径。
//! 关键字同时匹配资源标题与标题的规范化形式，发布时间收窄扫描范围，关联订阅时标出该订阅已经匹配到的集号，
//! 返回项里不带磁力链。

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use web::model::PageResourceRequest;
use web::query::resource_view::ResourceViewQuery;

struct Fixture {
    // 临时库路径，随 TempDir 一起删掉
    _dir: tempfile::TempDir,
    pool: Pool<Sqlite>,
}

// 查询只引用这几个列，这里按生产 SQL 实际用到的列最小化建表
async fn setup() -> Fixture {
    let dir = tempfile::tempdir().expect("create temp dir failed");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(dir.path().join("web-resource-view-test.db"))
                .create_if_missing(true),
        )
        .await
        .expect("connect sqlite failed");

    for ddl in [
        "CREATE TABLE resource (info_hash BLOB PRIMARY KEY NOT NULL, title TEXT NOT NULL, match_title TEXT NOT NULL, published_at INTEGER NOT NULL, url TEXT NOT NULL DEFAULT '');",
        "CREATE TABLE sub_anime_episode (sub_anime_id INTEGER NOT NULL, resource_id BLOB NOT NULL, ep_num REAL, CONSTRAINT uk_sub_anime_resource UNIQUE (sub_anime_id, resource_id));",
    ] {
        sqlx::query(ddl)
            .execute(&pool)
            .await
            .expect("create table failed");
    }

    Fixture { _dir: dir, pool }
}

const HASH_A: [u8; 20] = [0x01; 20];
const HASH_B: [u8; 20] = [0x02; 20];
const HASH_C: [u8; 20] = [0x03; 20];

async fn seed_resource(
    pool: &Pool<Sqlite>,
    info_hash: [u8; 20],
    title: &str,
    match_title: &str,
    published_at: i64,
) {
    sqlx::query(
        "INSERT INTO resource (info_hash, title, match_title, published_at) VALUES (?, ?, ?, ?)",
    )
    .bind(info_hash.as_slice())
    .bind(title)
    .bind(match_title)
    .bind(published_at)
    .execute(pool)
    .await
    .expect("insert resource failed");
}

async fn seed_episode(pool: &Pool<Sqlite>, sub_anime_id: i64, resource_id: [u8; 20], ep_num: f64) {
    sqlx::query(
        "INSERT INTO sub_anime_episode (sub_anime_id, resource_id, ep_num) VALUES (?, ?, ?)",
    )
    .bind(sub_anime_id)
    .bind(resource_id.as_slice())
    .bind(ep_num)
    .execute(pool)
    .await
    .expect("insert sub_anime_episode failed");
}

fn request(keyword: &str) -> PageResourceRequest {
    PageResourceRequest {
        keyword: Some(keyword.to_string()),
        start_at: None,
        end_at: None,
        sub_anime_id: None,
        page: None,
        page_size: None,
    }
}

async fn page(
    fixture: &Fixture,
    request: &PageResourceRequest,
    page: usize,
    page_size: usize,
) -> web::model::ResourcePage {
    ResourceViewQuery::new(fixture.pool.clone())
        .page_resources(request, page, page_size)
        .await
        .expect("page resources failed")
}

#[tokio::test]
async fn test_page_resources_matches_title_and_match_title() {
    let fixture = setup().await;
    seed_resource(
        &fixture.pool,
        HASH_A,
        "[ANi] 败犬女主 第 3 集",
        "败犬女主 第 3 集",
        300,
    )
    .await;
    seed_resource(
        &fixture.pool,
        HASH_B,
        "[ANi] bocchi 第 3 集",
        "败犬女主 第 3 集",
        200,
    )
    .await;
    seed_resource(
        &fixture.pool,
        HASH_C,
        "[ANi] 别当欧尼酱 第 3 集",
        "别当欧尼酱 第 3 集",
        100,
    )
    .await;

    let result = page(&fixture, &request("败犬女主"), 1, 10).await;

    let titles: Vec<&str> = result.data.iter().map(|item| item.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["[ANi] 败犬女主 第 3 集", "[ANi] bocchi 第 3 集"],
        "page_resources should return hits on both title and match_title"
    );
}

#[tokio::test]
async fn test_page_resources_item_does_not_carry_magnet() {
    let fixture = setup().await;
    seed_resource(
        &fixture.pool,
        HASH_A,
        "[ANi] 败犬女主 第 3 集",
        "败犬女主 第 3 集",
        300,
    )
    .await;

    let result = page(&fixture, &request("败犬女主"), 1, 10).await;
    let item = result.data.first().expect("one resource should match");
    let value = serde_json::to_value(item).expect("resource item should serialise");

    let mut keys: Vec<&str> = value
        .as_object()
        .expect("resource item should be an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["ep_num", "info_hash", "published_at", "title"],
        "resource item should not carry the magnet url"
    );
    assert_eq!(item.info_hash, hex::encode(HASH_A));
}

#[tokio::test]
async fn test_page_resources_filters_by_published_at_window() {
    let fixture = setup().await;
    seed_resource(
        &fixture.pool,
        HASH_A,
        "[ANi] 败犬女主 第 1 集",
        "败犬女主 第 1 集",
        100,
    )
    .await;
    seed_resource(
        &fixture.pool,
        HASH_B,
        "[ANi] 败犬女主 第 2 集",
        "败犬女主 第 2 集",
        200,
    )
    .await;
    seed_resource(
        &fixture.pool,
        HASH_C,
        "[ANi] 败犬女主 第 3 集",
        "败犬女主 第 3 集",
        300,
    )
    .await;

    let mut window = request("败犬女主");
    window.start_at = Some(150);
    window.end_at = Some(250);
    let result = page(&fixture, &window, 1, 10).await;

    let info_hashes: Vec<&str> = result
        .data
        .iter()
        .map(|item| item.info_hash.as_str())
        .collect();
    assert_eq!(
        info_hashes,
        vec![hex::encode(HASH_B).as_str()],
        "page_resources should not return resources outside the published_at window"
    );
}

#[tokio::test]
async fn test_page_resources_reports_has_more_and_pages_newest_first() {
    let fixture = setup().await;
    seed_resource(
        &fixture.pool,
        HASH_A,
        "[ANi] 败犬女主 第 1 集",
        "败犬女主 第 1 集",
        100,
    )
    .await;
    seed_resource(
        &fixture.pool,
        HASH_B,
        "[ANi] 败犬女主 第 2 集",
        "败犬女主 第 2 集",
        200,
    )
    .await;
    seed_resource(
        &fixture.pool,
        HASH_C,
        "[ANi] 败犬女主 第 3 集",
        "败犬女主 第 3 集",
        300,
    )
    .await;

    let first = page(&fixture, &request("败犬女主"), 1, 2).await;
    assert_eq!(
        first
            .data
            .iter()
            .map(|item| item.info_hash.as_str())
            .collect::<Vec<_>>(),
        vec![hex::encode(HASH_C).as_str(), hex::encode(HASH_B).as_str()],
        "page_resources should order by published_at desc"
    );
    assert!(
        first.has_more,
        "page_resources should report has_more when another page exists"
    );
    assert_eq!((first.page, first.page_size), (1, 2));

    let second = page(&fixture, &request("败犬女主"), 2, 2).await;
    assert_eq!(
        second
            .data
            .iter()
            .map(|item| item.info_hash.as_str())
            .collect::<Vec<_>>(),
        vec![hex::encode(HASH_A).as_str()],
        "page_resources second page should hold the oldest resource"
    );
    assert!(
        !second.has_more,
        "page_resources should not report has_more on the last page"
    );
}

#[tokio::test]
async fn test_page_resources_marks_ep_num_of_bound_subscription() {
    let fixture = setup().await;
    seed_resource(
        &fixture.pool,
        HASH_A,
        "[ANi] 败犬女主 第 3 集",
        "败犬女主 第 3 集",
        300,
    )
    .await;
    seed_resource(
        &fixture.pool,
        HASH_B,
        "[ANi] 败犬女主 第 4 集",
        "败犬女主 第 4 集",
        200,
    )
    .await;
    seed_episode(&fixture.pool, 7, HASH_A, 3.0).await;
    seed_episode(&fixture.pool, 8, HASH_B, 4.0).await;

    let mut request = request("败犬女主");
    request.sub_anime_id = Some(7);
    let result = page(&fixture, &request, 1, 10).await;

    assert_eq!(
        result
            .data
            .iter()
            .map(|item| (item.info_hash.clone(), item.ep_num))
            .collect::<Vec<_>>(),
        vec![
            (hex::encode(HASH_A), Some(3.0)),
            (hex::encode(HASH_B), None)
        ],
        "page_resources should only mark ep_num of the given subscription"
    );
}

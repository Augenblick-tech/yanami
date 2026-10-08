//! 列表与统计接口读取订阅搜索状态的口径。
//! 搜索中(3) 是订阅表里存着不搜索(0) 又挂在搜索委托上，等待中(1)、匹配中(2) 按订阅表里存的状态读。

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use web::model::PageAnimeRequest;
use web::query::anime_view::AnimeViewQuery;
use web::query::stat_view::StatQuery;

struct Fixture {
    // 临时库路径，随 TempDir 一起删掉
    _dir: tempfile::TempDir,
    pool: Pool<Sqlite>,
}

// web 的读取查询只引用这几个列，这里按生产 SQL 实际用到的列最小化建表
async fn setup() -> Fixture {
    let dir = tempfile::tempdir().expect("create temp dir failed");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(dir.path().join("web-query-test.db"))
                .create_if_missing(true),
        )
        .await
        .expect("connect sqlite failed");

    for ddl in [
        "CREATE TABLE anime (id INTEGER PRIMARY KEY, air_weekday INTEGER NOT NULL DEFAULT 0, air_date TEXT, air_quarter INTEGER);",
        "CREATE TABLE anime_title (id INTEGER PRIMARY KEY AUTOINCREMENT, anime_id INTEGER NOT NULL, name TEXT NOT NULL, match_name TEXT NOT NULL, is_origin INTEGER NOT NULL DEFAULT 0, lang_target TEXT NOT NULL DEFAULT '');",
        "CREATE TABLE anime_season (anime_id INTEGER NOT NULL, target_source TEXT NOT NULL, season_number INTEGER NOT NULL, planned_ep_count INTEGER NOT NULL, description TEXT NOT NULL DEFAULT '');",
        "CREATE TABLE sub_anime (id INTEGER PRIMARY KEY, anime_id INTEGER NOT NULL, space_id INTEGER, rule_id INTEGER, search_status INTEGER NOT NULL, progress INTEGER NOT NULL DEFAULT 0);",
        "CREATE TABLE search_mandate_sub_anime (search_mandate_id INTEGER NOT NULL, sub_anime_id INTEGER NOT NULL);",
        "CREATE TABLE search_pool (id INTEGER PRIMARY KEY);",
        "CREATE TABLE rule (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
    ] {
        sqlx::query(ddl)
            .execute(&pool)
            .await
            .expect("create table failed");
    }

    Fixture { _dir: dir, pool }
}

async fn seed_anime(pool: &Pool<Sqlite>, anime_id: i64) {
    sqlx::query(
        "INSERT INTO anime (id, air_weekday, air_date, air_quarter) VALUES (?, 5, '2024-04-05', 202402)",
    )
    .bind(anime_id)
    .execute(pool)
    .await
    .expect("insert anime failed");
    sqlx::query(
        "INSERT INTO anime_title (anime_id, name, match_name, is_origin) VALUES (?, '番A', 'fana', 1)",
    )
    .bind(anime_id)
    .execute(pool)
    .await
    .expect("insert anime_title failed");
    sqlx::query(
        "INSERT INTO anime_season (anime_id, target_source, season_number, planned_ep_count, description) VALUES (?, 'Bangumi', 1, 12, '描述')",
    )
    .bind(anime_id)
    .execute(pool)
    .await
    .expect("insert anime_season failed");
}

async fn seed_subscription(
    pool: &Pool<Sqlite>,
    sub_anime_id: i64,
    anime_id: i64,
    search_status: i64,
    on_mandate: bool,
) {
    sqlx::query(
        "INSERT INTO sub_anime (id, anime_id, space_id, search_status) VALUES (?, ?, 1, ?)",
    )
    .bind(sub_anime_id)
    .bind(anime_id)
    .bind(search_status)
    .execute(pool)
    .await
    .expect("insert sub_anime failed");
    if on_mandate {
        sqlx::query(
            "INSERT INTO search_mandate_sub_anime (search_mandate_id, sub_anime_id) VALUES (1, ?)",
        )
        .bind(sub_anime_id)
        .execute(pool)
        .await
        .expect("insert search_mandate_sub_anime failed");
    }
}

fn request(search_status: Option<i64>) -> PageAnimeRequest {
    PageAnimeRequest {
        page: None,
        page_size: None,
        keyword: None,
        lang: None,
        year: None,
        month: None,
        subscription: None,
        search_status,
        status: None,
    }
}

#[tokio::test]
async fn list_reports_pending_for_a_subscription_that_still_waits_for_the_local_match() {
    // 订阅表里存着等待中(1)，又挂在搜索委托上：本地匹配还没做完，读出来仍然是等待中
    let f = setup().await;
    seed_anime(&f.pool, 100).await;
    seed_subscription(&f.pool, 9, 100, 1, true).await;

    let pending = AnimeViewQuery::new(f.pool.clone())
        .page_anime_views(&request(Some(1)), 1, 1, 10)
        .await
        .expect("page waiting subscriptions");

    assert_eq!(pending.total, 1);
    let sub_info = pending.data[0].sub_info.as_ref().expect("sub info");
    assert_eq!(sub_info.search_status, 1);

    // 本地匹配没做完就不算搜索中
    let searching = AnimeViewQuery::new(f.pool.clone())
        .page_anime_views(&request(Some(3)), 1, 1, 10)
        .await
        .expect("page searching subscriptions");
    assert!(searching.data.is_empty());
}

#[tokio::test]
async fn list_reports_searching_after_the_local_match_is_done() {
    // 订阅表里存着不搜索(0)，又挂在搜索委托上：本地匹配已经做完，读出来是搜索中(3)
    let f = setup().await;
    seed_anime(&f.pool, 100).await;
    seed_subscription(&f.pool, 9, 100, 0, true).await;

    let searching = AnimeViewQuery::new(f.pool.clone())
        .page_anime_views(&request(Some(3)), 1, 1, 10)
        .await
        .expect("page searching subscriptions");

    assert_eq!(searching.total, 1);
    let sub_info = searching.data[0].sub_info.as_ref().expect("sub info");
    assert_eq!(sub_info.search_status, 3);

    let pending = AnimeViewQuery::new(f.pool.clone())
        .page_anime_views(&request(Some(1)), 1, 1, 10)
        .await
        .expect("page waiting subscriptions");
    assert!(pending.data.is_empty());
}

#[tokio::test]
async fn stat_counts_pending_and_searching_by_the_subscription_table() {
    let f = setup().await;
    seed_anime(&f.pool, 100).await;
    seed_anime(&f.pool, 101).await;
    // 一条还没做完本地匹配，一条已经做完，两条都挂在搜索委托上
    seed_subscription(&f.pool, 9, 100, 1, true).await;
    seed_subscription(&f.pool, 10, 101, 0, true).await;

    let stat = StatQuery::new(f.pool.clone())
        .get_system_stat(1, &[])
        .await
        .expect("get system stat");

    let quarter = stat.quarter_stats.first().expect("quarter stat");
    assert_eq!(quarter.not_search_count, 0);
    assert_eq!(quarter.pending_count, 1);
    assert_eq!(quarter.searching_count, 1);
}

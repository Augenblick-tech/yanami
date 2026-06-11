use std::time::Duration;

use anime::entity::{anime_source::AnimeSources, animes::Animes};
use anyhow::Result;
use chrono::Local;
use feed::entity::feeds::Feeds;
use job::{model::TaskConfig, scheduler::TaskScheduler};
use resource::entity::resources::Resources;
use subscription::entity::{search_mandates::SearchMandates, sub_animes::SubAnimes};
use tracing::info;
use user::entity::users::Users;

use crate::task::{
    check_missing_episodes_task::check_missing_episodes_task, download_task::download_task, get_resource_task::get_resource_and_match_task, search_task::{local_match_task, search_task}, sync_calendar_task::sync_calendar_task,
};

pub async fn setup(
    users: Users,
    animes: Animes,
    source: AnimeSources,
    sub_animes: SubAnimes,
    resources: Resources,
    feeds: Feeds,
    search_mandates: SearchMandates,
) -> Result<TaskScheduler> {
    let mut scheduer = TaskScheduler::new();

    let sync_animes = animes.clone();
    let sync_source = source.clone();
    let sync_users = users.clone();
    let sync_sub_animes = sub_animes.clone();
    // 注册同步新番列表任务
    scheduer.register(
        TaskConfig {
            name: "sync anime seasonal info".to_string(),
            interval: Duration::from_hours(24),
            allow_reentry: false,
        },
        move || {
            let animes = sync_animes.clone();
            let source = sync_source.clone();
            let users = sync_users.clone();
            let sub_animes = sync_sub_animes.clone();
            async move {
                if let Err(e) = sync_calendar_task(animes, source, users, sub_animes).await {
                    tracing::error!("sync calender task failed, {}", e);
                }
            }
        },
    );

    let get_resource_resources = resources.clone();
    let get_resource_feeds = feeds.clone();
    let get_resource_sub_animes = sub_animes.clone();
    scheduer.register(
        TaskConfig {
            name: "get resource task".to_string(),
            interval: Duration::from_mins(5),
            allow_reentry: false,
        },
        move || {
            let resources = get_resource_resources.clone();
            let feeds = get_resource_feeds.clone();
            let sub_animes = get_resource_sub_animes.clone();
            async move {
                let now = Local::now();
                info!("start match task in {}", now);
                if let Err(e) = get_resource_and_match_task(feeds, resources, sub_animes).await {
                    tracing::error!("get resource task failed, {}", e);
                }
                info!("match task run time {}", Local::now() - now);
            }
        },
    );

    let local_match_sub_animes = sub_animes.clone();
    let local_match_resources = resources.clone();
    let local_match_search_mandates = search_mandates.clone();
    let local_match_feeds = feeds.clone();
    scheduer.register(
        TaskConfig {
            name: "search local match task".to_string(),
            interval: Duration::from_secs(10),
            allow_reentry: false,
        },
        move || {
            let sub_animes = local_match_sub_animes.clone();
            let resources = local_match_resources.clone();
            let feeds = local_match_feeds.clone();
            let search_mandates = local_match_search_mandates.clone();
            async move {
                if let Err(e) =
                    local_match_task(sub_animes, resources, feeds, search_mandates).await
                {
                    tracing::error!("local match task failed, {}", e);
                }
            }
        },
    );

    let search_task_sub_animes = sub_animes.clone();
    let search_task_resources = resources.clone();
    let search_task_search_mandates = search_mandates.clone();
    scheduer.register(
        TaskConfig {
            name: "search task".to_string(),
            interval: Duration::from_secs(10),
            allow_reentry: false,
        },
        move || {
            let sub_animes = search_task_sub_animes.clone();
            let resources = search_task_resources.clone();
            let search_mandates = search_task_search_mandates.clone();
            async move {
                if let Err(e) = search_task(search_mandates, resources, sub_animes).await {
                    tracing::error!("search task failed, {}", e);
                }
            }
        },
    );

    let check_missing_sub_animes = sub_animes.clone();
    scheduer.register(
        TaskConfig {
            name: "check missing epsiodes task".to_string(),
            interval: Duration::from_hours(12),
            allow_reentry: false,
        },
        move || {
            let sub_animes = check_missing_sub_animes.clone();
            async move {
                if let Err(e) = check_missing_episodes_task(sub_animes).await {
                    tracing::error!("check missing epsiodes task failed, {}", e);
                }
            }
        },
    );

    let download_sub_animes = sub_animes.clone();
    let download_users = users.clone();
    scheduer.register(
        TaskConfig {
            name: "download epsiode task".to_string(),
            interval: Duration::from_secs(10),
            allow_reentry: false,
        },
        move || {
            let sub_animes = download_sub_animes.clone();
            let users = download_users.clone();
            async move {
                if let Err(e) = download_task(sub_animes, users).await {
                    tracing::error!("download epsiodes task failed, {}", e);
                }
            }
        },
    );

    Ok(scheduer)
}

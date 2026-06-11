use anyhow::Result;
use chrono::{Duration, Local};
use feed::entity::{feeds::Feeds, model::FeedFetchResult::Success};
use futures::StreamExt;
use resource::entity::{model::ResourceQuery, resources::Resources};
use subscription::entity::{
    model::{SubAnimeListQuery, SubAnimeSearchStatus, SubAnimeStatus},
    sub_animes::SubAnimes,
};
use tracing::error;

pub async fn get_resource_and_match_task(
    feeds: Feeds,
    resources: Resources,
    sub_animes: SubAnimes,
) -> Result<()> {
    let feed_entity_list = feeds.list_site_feeds().await?;
    for feed_entity in &feed_entity_list {
        match feed_entity.list().await {
            Ok(res) => {
                if let Success(res) = res
                    && let Err(e) = resources.just_save(res).await {
                        error!(
                            "get resource task save {} data failed, {}",
                            feed_entity.id(),
                            e
                        );
                    }
            }
            Err(e) => {
                error!(
                    "get resource task get {} feed failed, {}",
                    feed_entity.id(),
                    e
                );
            }
        }
    }

    // 资源获取之后，无论是否获取到新资源，都执行匹配
    let sub_anime_entity_list = sub_animes
        .list(&SubAnimeListQuery {
            anime_id: None,
            space_id: None,
            search_status: Some(SubAnimeSearchStatus::NotSearch),
            sub_status: Some(SubAnimeStatus::Enable),
            limit: None,
        })
        .await?;
    if sub_anime_entity_list.is_empty() {
        return Ok(());
    }

    let mut matchers = vec![];
    for i in &sub_anime_entity_list {
        let matcher = match sub_animes.as_matcher(i).await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    "get resource made {} entity to matcher failed, {}",
                    i.id(),
                    e
                );
                continue;
            }
        };
        matchers.push(matcher);
    }

    let now = Local::now();
    let start_at = now - Duration::hours(3);

    let query = ResourceQuery {
        keywords: None,
        start_at: Some(start_at.timestamp()),
        end_at: None,
        limit: None,
        offset: None,
    };
    let mut stream = resources.stream(&query);
    while let Some(res) = stream.next().await {
        for matcher in &mut matchers {
            match &res {
                Ok(res) => {
                    if let Err(e) = matcher.match_resource(res) {
                        tracing::error!(
                            "get resource match task {} match {} failed, {}",
                            matcher.sub_anime_id(),
                            res.title(),
                            e
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "lget resource match task {} get res failed, {}",
                        matcher.sub_anime_id(),
                        e
                    );
                }
            }
        }
    }

    for matcher in matchers {
        if let Err(e) = sub_animes.save_matcher(&matcher).await {
            error!(
                "get resource match task save {} matcher failed, {}",
                matcher.sub_anime_id(),
                e
            );
        }
    }

    Ok(())
}

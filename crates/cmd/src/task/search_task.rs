use anyhow::Result;
use feed::entity::feeds::Feeds;
use feed::entity::model::FeedFetchResult::{Failure, Success};
use futures::StreamExt;
use resource::entity::{model::ResourceQuery, resources::Resources};
use subscription::entity::model::SubAnimeStatus;
use subscription::entity::sub_anime_entity::SubAnimeEntityMatcher;
use subscription::entity::{
    model::{SubAnimeListQuery, SubAnimeSearchStatus},
    search_mandates::SearchMandates,
    sub_animes::SubAnimes,
};

pub async fn local_match_task(
    sub_animes: SubAnimes,
    resources: Resources,
    feeds: Feeds,
    search_mandates: SearchMandates,
) -> Result<()> {
    let mut res = sub_animes
        .list(&SubAnimeListQuery {
            anime_id: None,
            space_id: None,
            search_status: Some(SubAnimeSearchStatus::Matching),
            sub_status: None,
            limit: Some(1),
        })
        .await?;

    if res.is_empty() {
        res = sub_animes
            .list(&SubAnimeListQuery {
                anime_id: None,
                space_id: None,
                search_status: Some(SubAnimeSearchStatus::Pending),
                sub_status: None,
                limit: Some(1),
            })
            .await?;
    }

    if let Some(sub_anime_entity) = res.first() {
        let mut sub_anime_entity = sub_anime_entity.clone();
        match sub_anime_entity.try_claim() {
            // 进入匹配状态，保存状态并继续
            subscription::entity::model::ClaimResult::Matched => {
                sub_animes.save(&sub_anime_entity).await?;
            }
            // 已经完结，保存状态并结束
            subscription::entity::model::ClaimResult::Completed => {
                sub_animes.save(&sub_anime_entity).await?;
                return Ok(());
            }
            // 正在匹配
            subscription::entity::model::ClaimResult::AlreayMartched => {}
        }
        let time_range = sub_anime_entity.match_time_range();
        let mut matcher = sub_animes.as_matcher(&sub_anime_entity).await?;
        let query = ResourceQuery {
            keywords: None,
            start_at: Some(time_range.start.and_utc().timestamp()),
            end_at: Some(time_range.end.and_utc().timestamp()),
            limit: None,
            offset: None,
        };
        let mut stream = resources.stream(&query);
        while let Some(res) = stream.next().await {
            match res {
                Ok(res) => {
                    if let Err(e) = matcher.match_resource(&res) {
                        tracing::error!(
                            "local match task {} match {} failed, {}",
                            sub_anime_entity.id(),
                            res.title(),
                            e
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "local match task {} get res failed, {}",
                        sub_anime_entity.id(),
                        e
                    );
                }
            }
        }

        // 保存匹配结果
        sub_animes.save_matcher(&matcher).await?;

        // 判断是否完结
        let new_sub_anime_entity = sub_animes
            .find_by_sub_anime_id(sub_anime_entity.id())
            .await?;
        if let Some(entity) = new_sub_anime_entity {
            sub_anime_entity = entity;
        } else {
            tracing::warn!("not found sub anime entity {}", sub_anime_entity.id());
            return Ok(());
        }

        // 确认是否需要搜索
        if sub_anime_entity.request_search() {
            let search_url_provider = feeds.get_search_feeds().await?;
            let search_urls = sub_anime_entity.get_search_urls(&search_url_provider);
            if search_mandates
                .create_from_search_urls(sub_anime_entity.anime_id(), search_urls)
                .await?
                .is_empty()
            {
                // 如果搜索委托数量为零，则取消搜索
                sub_anime_entity.cancel_search();
            }
        }
        // 保存结果
        sub_animes.save(&sub_anime_entity).await?;
    }

    Ok(())
}

pub async fn search_task(
    search_mandates: SearchMandates,
    resources: Resources,
    sub_animes: SubAnimes,
) -> Result<()> {
    if let Some(mut mandate_entity) = search_mandates.get_one().await? {
        let data = mandate_entity.fetch().await?;
        let anime_id = mandate_entity.anime_id();
        match data {
            feed::entity::model::FeedFetchResult::Retryable(error) => {
                tracing::error!(
                    "search task fetch {} mandate failed, {}, will retry",
                    mandate_entity.id(),
                    error
                );
                return Ok(());
            }
            feed::entity::model::FeedFetchResult::Denied => {
                tracing::error!(
                    "search task fetch {} mandate denied by policy, will retry",
                    mandate_entity.id()
                );
                return Ok(());
            }
            _ => {}
        };

        let sub_anime_entity_list = sub_animes
            .list(&SubAnimeListQuery {
                anime_id: Some(anime_id),
                space_id: None,
                search_status: Some(SubAnimeSearchStatus::Searching),
                sub_status: Some(SubAnimeStatus::Enable),
                limit: None,
            })
            .await?;
        let done = match data {
            Failure(error) => {
                tracing::error!(
                    "search task fetch {} mandate failed, {}, will drop",
                    mandate_entity.id(),
                    error
                );
                search_mandates.drop(mandate_entity).await?
            }
            Success(data) => {
                let res = resources.save(data).await?;
                for entity in &sub_anime_entity_list {
                    if let Ok(mut matcher) = sub_animes.as_matcher(entity).await {
                        for res_item in &res {
                            if let Err(e) = matcher.match_resource(res_item) {
                                tracing::error!(
                                    "search task {} match {} resource failed, {}",
                                    entity.id(),
                                    res_item.title(),
                                    e
                                );
                            }
                        }
                        if let Err(e) = sub_animes.save_matcher(&matcher).await {
                            tracing::error!(
                                "search task save {} matcher failed, {}",
                                entity.id(),
                                e
                            );
                        }
                    }
                }
                search_mandates.completed(mandate_entity).await?
            }
            _ => false,
        };

        if done {
            // 保存失败时，尝试有限次数重试
            // TODO: 需要一种合理的机制，根治这种分步导致的状态不一致问题
            for _ in 1..4 {
                let Ok(mut pending_sub_anime_entity_list) = sub_animes
                    .list(&SubAnimeListQuery {
                        anime_id: Some(anime_id),
                        space_id: None,
                        search_status: None,
                        sub_status: None,
                        limit: None,
                    })
                    .await
                else {
                    continue;
                };

                pending_sub_anime_entity_list.retain_mut(|i| i.cancel_search());

                if !pending_sub_anime_entity_list.is_empty()
                    && let Err(e) = sub_animes.saves(&pending_sub_anime_entity_list).await
                {
                    tracing::error!("search task saves sub anime entity failed, {}", e);
                } else {
                    break;
                }
            }
        }
    }

    Ok(())
}

use anime::entity::{anime_source::AnimeSources, animes::Animes};
use anyhow::Result;
use subscription::entity::sub_animes::SubAnimes;
use tracing::{error, info};
use user::entity::users::Users;

pub async fn sync_calendar_task(
    animes: Animes,
    source: AnimeSources,
    users: Users,
    sub_animes: SubAnimes,
) -> Result<()> {
    let list = source.sync().await?;
    let anime_entity_list = animes.sync_metadata(list).await?;

    if anime_entity_list.is_empty() {
        return Ok(());
    }

    let user_entity_list = users.list_auto_sub().await?;
    if user_entity_list.is_empty() {
        return Ok(());
    }

    let anime_ids = anime_entity_list
        .iter()
        .map(|anime| anime.id())
        .collect::<Vec<_>>();

    for user in &user_entity_list {
        let subscribed = match sub_animes
            .find_by_anime_ids(user.space_id(), anime_ids.clone())
            .await
        {
            Ok(subscribed) => subscribed,
            Err(e) => {
                error!(
                    "space {} find subscribed animes failed, {}",
                    user.space_id(),
                    e
                );
                continue;
            }
        };

        for anime in &anime_entity_list {
            // 已订阅的番剧不再重复订阅
            if subscribed.contains_key(&anime.id()) {
                continue;
            }

            if let Err(e) = sub_animes.create(user.space_id(), anime.id()).await {
                error!(
                    "space {} auto sub anime {} failed, {}",
                    user.space_id(),
                    anime.id(),
                    e
                );
            } else {
                info!("user {} auto sub {:?}", user.username(), anime.title())
            }
        }
    }

    Ok(())
}

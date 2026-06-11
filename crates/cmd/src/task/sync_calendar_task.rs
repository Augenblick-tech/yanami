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

    for user in &user_entity_list {
        for anime in &anime_entity_list {
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

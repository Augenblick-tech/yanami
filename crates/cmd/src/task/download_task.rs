use anime::entity::animes::Animes;
use anyhow::Result;
use subscription::entity::sub_animes::SubAnimes;
use user::entity::users::Users;

pub async fn download_task(sub_animes: SubAnimes, users: Users, animes: Animes) -> Result<()> {
    let Some(mut epsiode_entity) = sub_animes.get_one_undownload_ep().await? else {
        return Ok(());
    };

    let Some(user_entity) = users.get_by_space_id(epsiode_entity.space_id()).await? else {
        return Ok(());
    };

    let downloader = match users.as_downloader(&user_entity).await {
        Ok(Some(v)) => v,
        Ok(None) => return Ok(()),
        Err(e) => Err(e)?,
    };
    if let Some(sub_anime_entity) = sub_animes
        .find_by_sub_anime_id(epsiode_entity.sub_anime_id())
        .await?
    {
        // 落点由系列决定：系列或季号取不到就记日志放回队列，下次再试
        let series = match animes.series(epsiode_entity.anime_id()).await {
            Ok(series) => series,
            Err(e) => {
                tracing::error!(
                    "download epsiode series resolve failed, episode_id = {}, anime_id = {}, {}",
                    epsiode_entity.id(),
                    epsiode_entity.anime_id(),
                    e
                );
                return Ok(());
            }
        };

        let sub_anime_eps = sub_animes.as_eps(&sub_anime_entity).await;
        if epsiode_entity.download(&downloader, &series).await? {
            sub_anime_eps.save_epsiode(&epsiode_entity).await?;
        }
    };

    Ok(())
}

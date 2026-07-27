use anyhow::Result;
use subscription::entity::sub_animes::SubAnimes;
use user::entity::users::Users;

pub async fn download_task(sub_animes: SubAnimes, users: Users) -> Result<()> {
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
        let sub_anime_eps = sub_animes.as_eps(&sub_anime_entity).await;
        if epsiode_entity.download(&downloader).await? {
            sub_anime_eps.save_epsiode(&epsiode_entity).await?;
        }
    };

    Ok(())
}

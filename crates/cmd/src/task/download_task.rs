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
    if epsiode_entity.download(&downloader).await? {
        sub_animes.save_epsiode(&epsiode_entity).await?;
    }

    Ok(())
}

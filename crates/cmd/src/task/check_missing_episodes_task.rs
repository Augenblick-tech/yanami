use anyhow::Result;
use subscription::entity::{
    model::{SubAnimeListQuery, SubAnimeSearchStatus, SubAnimeStatus},
    sub_animes::SubAnimes,
};
use tracing::{error, info};

pub async fn check_missing_episodes_task(sub_animes: SubAnimes) -> Result<()> {
    let list = sub_animes
        .list(&SubAnimeListQuery {
            anime_id: None,
            space_id: None,
            search_status: Some(SubAnimeSearchStatus::NotSearch),
            sub_status: Some(SubAnimeStatus::Enable),
            limit: None,
        })
        .await?;
    if list.is_empty() {
        return Ok(());
    }

    let mut save_list = vec![];

    for mut sub_anime_entity in list {
        let sub_anime_eps = sub_animes.as_eps(&sub_anime_entity).await;
        match sub_anime_eps.check_missing_episodes().await {
            Ok(missing) => {
                if missing
                    && sub_anime_entity.enable_search() {
                        info!(
                            "check_missing_episodes_task will enable {} search",
                            sub_anime_entity.id()
                        );
                        save_list.push(sub_anime_entity);
                    }
            }
            Err(e) => {
                error!(
                    "check_missing_episodes_task check {} failed, {}",
                    sub_anime_entity.id(),
                    e
                );
            }
        }
    }

    sub_animes.saves(&save_list).await?;

    Ok(())
}

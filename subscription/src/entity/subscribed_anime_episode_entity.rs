#[derive(Debug, Clone)]
pub struct SubscribedAnimeEpisodeEntity {
    id: u32,
    sub_anime_id: u32,
    title: String,
    url: String,
    published_at: i64,
    created_at: i64,
}

impl SubscribedAnimeEpisodeEntity {
    pub(super) fn new(
        id: u32,
        sub_anime_id: u32,
        title: String,
        url: String,
        published_at: i64,
        created_at: i64,
    ) -> Self {
        Self {
            id,
            sub_anime_id,
            title,
            url,
            published_at,
            created_at,
        }
    }
}

use crate::entity::model::{
    Episode, EpisodeBaseData, EpisodeProp, Mandate, SearchMandateProp, SubAnimeBaseData,
    SubAnimeListQuery, SubAnimeProps,
};
use crate::entity::model::{Rule, RuleBaseData, RuleQuery};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait SpaceRuleMatcher: Send + Sync {
    fn is_match(&self, text: &str) -> (bool, i64);
}

#[async_trait]
pub trait SubAnimeRepository: Send + Sync {
    async fn insert_sub_anime(&self, space_id: i64, anime_id: i64) -> Result<SubAnimeProps>;
    async fn update_sub_anime(&self, data: &SubAnimeBaseData) -> Result<()>;
    async fn update_sub_animes(&self, data: &[SubAnimeBaseData]) -> Result<()>;
    async fn find_sub_anime(&self, id: i64) -> Result<Option<SubAnimeProps>>;
    async fn list(&self, query: &SubAnimeListQuery) -> Result<Vec<SubAnimeProps>>;
    async fn find_by_anime_ids(
        &self,
        space_id: i64,
        anime_ids: Vec<i64>,
    ) -> Result<Vec<SubAnimeProps>>;

    async fn list_eps(&self, sub_anime_id: i64) -> Result<Vec<EpisodeProp>>;
    async fn get_one_undownload_ep(&self) -> Result<Option<EpisodeProp>>;
    async fn update_epsiode_status(&self, data: &EpisodeBaseData) -> Result<()>;
    async fn update_sub_anime_progress(
        &self,
        data: &SubAnimeBaseData,
        eps: &[Episode],
    ) -> Result<()>;
    async fn delete(&self, sub_anime: i64) -> Result<()>;
    async fn binding_rule_and_clear_eps(&self, sub_anime: i64, rule_id: i64) -> Result<()>;
}

#[async_trait]
pub trait RuleMatcher: Send + Sync {
    fn is_match(&self, pattern: &str, text: &str) -> bool;
    fn validate(&self, pattern: &str) -> Result<()>;
}

#[async_trait]
pub trait RuleRepository: Send + Sync {
    async fn list(&self, query: &RuleQuery) -> Result<Vec<RuleBaseData>>;
    async fn find(&self, id: i64) -> Result<Option<RuleBaseData>>;
    async fn insert(&self, rule: &Rule) -> Result<RuleBaseData>;
    async fn save(&self, rule: &RuleBaseData) -> Result<()>;
    async fn delete(&self, id: i64) -> Result<()>;
}

#[async_trait]
pub trait SearchMandateRepository: Send + Sync {
    async fn get_one(&self, block_feed_ids: &[i64]) -> Result<Option<SearchMandateProp>>;
    async fn delete_and_count(&self, id: i64, anime_id: i64) -> Result<u64>;
    async fn save(&self, data: &[Mandate]) -> Result<Vec<SearchMandateProp>>;
    async fn count(&self) -> Result<u64>;
    async fn delete(&self, id: i64) -> Result<()>;
}

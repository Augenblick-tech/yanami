use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use common::shared::error::Error;

use crate::entity::{
    cap::{RuleMatcher, RuleRepository, SpaceRuleMatcher, SubAnimeRepository},
    episode_entity::EpsiodeEntity,
    model::{RuleQuery, SubAnimeListQuery},
    rule_entity::RuleEntity,
    space_rules::SpaceRules,
    sub_anime_entity::SubAnimeEntity,
    sub_anime_episode::SubAnimeEpsiodes,
    sub_anime_matcher::SubAnimeMatcher,
};

#[derive(Clone)]
pub struct SubAnimes {
    repo: Arc<dyn SubAnimeRepository>,
    rule_repo: Arc<dyn RuleRepository>,
    matcher: Arc<dyn RuleMatcher>,
}

impl SubAnimes {
    pub fn new(
        repo: Arc<dyn SubAnimeRepository>,
        rule_repo: Arc<dyn RuleRepository>,
        matcher: Arc<dyn RuleMatcher>,
    ) -> Self {
        Self {
            repo,
            rule_repo,
            matcher,
        }
    }
}

impl SubAnimes {
    pub async fn create(&self, space_id: i64, anime_id: i64) -> Result<SubAnimeEntity, Error> {
        let prop = self
            .repo
            .insert_sub_anime(space_id, anime_id)
            .await
            .map_err(|e| Error::external("subanimes create failed", e))?;
        Ok(SubAnimeEntity::new(prop.data, prop.extend))
    }

    pub async fn unsub(&self, entity: &SubAnimeEntity) -> Result<(), Error> {
        self.repo
            .delete(entity.id())
            .await
            .map_err(|e| Error::external("unsub anime failed", e))?;
        Ok(())
    }

    pub async fn list(&self, query: &SubAnimeListQuery) -> Result<Vec<SubAnimeEntity>, Error> {
        let props = self
            .repo
            .list(query)
            .await
            .map_err(|e| Error::external(format!("sub animes list {:?} failed", query), e))?;
        let list = props
            .into_iter()
            .map(|prop| SubAnimeEntity::new(prop.data, prop.extend))
            .collect::<Vec<_>>();
        Ok(list)
    }

    pub async fn find_by_sub_anime_id(&self, id: i64) -> Result<Option<SubAnimeEntity>, Error> {
        let prop = self
            .repo
            .find_sub_anime(id)
            .await
            .map_err(|e| Error::external("sub anime find entity by id failed", e))?;
        Ok(Some(SubAnimeEntity::new(prop.data, prop.extend)))
    }

    pub async fn find_by_anime_ids(
        &self,
        space_id: i64,
        ids: Vec<i64>,
    ) -> Result<HashMap<i64, SubAnimeEntity>, Error> {
        let list = self
            .repo
            .find_by_anime_ids(space_id, ids)
            .await
            .map_err(|e| Error::external("sub animes find by anime ids failed", e))?;
        let mut res = HashMap::new();
        for i in list {
            res.insert(i.data.anime_id, SubAnimeEntity::new(i.data, i.extend));
        }
        Ok(res)
    }

    pub async fn save(&self, entity: &SubAnimeEntity) -> Result<(), Error> {
        self.repo
            .update_sub_anime(entity.get_base_data())
            .await
            .map_err(|e| Error::external("subanimes save failed", e))?;
        Ok(())
    }

    pub async fn saves(&self, list: &[SubAnimeEntity]) -> Result<(), Error> {
        self.repo
            .update_sub_animes(
                &list
                    .iter()
                    .map(|i| i.get_base_data().clone())
                    .collect::<Vec<_>>(),
            )
            .await
            .map_err(|e| Error::external("sub animes saves failed", e))
    }

    pub async fn save_matcher(&self, matcher: &SubAnimeMatcher) -> Result<SubAnimeEpsiodes, Error> {
        // 如果没有匹配到任何规则
        let sub_anime_eps = SubAnimeEpsiodes::new(
            matcher.sub_anime_id(),
            matcher.eps_number(),
            self.repo.clone(),
        );
        let Some(rule_id) = matcher.get_rule_id() else {
            return Ok(sub_anime_eps);
        };

        // 如果没有匹配到任何新剧集
        let eps = matcher.get_match_eps();
        if eps.is_empty() {
            return Ok(sub_anime_eps);
        }

        sub_anime_eps.save_eps(rule_id, eps.to_vec()).await?;
        Ok(sub_anime_eps)
    }

    pub async fn as_eps(&self, entity: &SubAnimeEntity) -> SubAnimeEpsiodes {
        SubAnimeEpsiodes::new(entity.id(), entity.eps_number(), self.repo.clone())
    }

    pub async fn save_epsiode(&self, entity: &EpsiodeEntity) -> Result<(), Error> {
        self.repo
            .update_epsiode_status(entity.get_base_data())
            .await
            .map_err(|e| Error::external("save epsiode download status failed", e))?;
        Ok(())
    }

    pub async fn as_matcher(&self, entity: &SubAnimeEntity) -> Result<SubAnimeMatcher, Error> {
        let matcher: Arc<dyn SpaceRuleMatcher> = match entity.get_rule_id() {
            Some(rule_id) => {
                let rule_data = self
                    .rule_repo
                    .find(rule_id)
                    .await
                    .map_err(|e| Error::external("subanimes find rule failed", e))?
                    .context("not found binding rule")
                    .map_err(|e| Error::conflict(e.to_string()))?;
                Arc::new(RuleEntity::new(rule_data, self.matcher.clone()))
            }
            None => {
                let rules = self
                    .rule_repo
                    .list(&RuleQuery {
                        space_id: Some(entity.space_id()),
                        active: Some(true),
                    })
                    .await
                    .map_err(|e| Error::external("subanimes find space_rules failed", e))?;
                if rules.is_empty() {
                    return Err(Error::not_found(format!(
                        "subanimes not found space rules by {}",
                        entity.space_id()
                    )));
                }
                Arc::new(SpaceRules::new(rules, self.matcher.clone()))
            }
        };

        Ok(SubAnimeMatcher::new(
            entity.id(),
            entity.get_rule_id(),
            entity.eps_number(),
            entity.keywords(),
            matcher,
        ))
    }
}

impl SubAnimes {
    pub async fn get_one_undownload_ep(&self) -> Result<Option<EpsiodeEntity>, Error> {
        let ep = self
            .repo
            .get_one_undownload_ep()
            .await
            .map_err(|e| Error::external("get one undownload ep failed", e))?;
        Ok(ep.map(|i| EpsiodeEntity::new(i.data, i.extend)))
    }
}

use std::sync::Arc;

use domain::{
    anime::AnimeId,
    shared::error::DomainError,
    space::{AutoSubscribeDecision, Space, SpaceId, SpaceRepository, capability::SpaceAutoSubscribeCap},
};

use crate::{rule_entity::RuleEntity, space_rules::SpaceRules};

/// 订阅空间聚合根
#[derive(Debug, Clone)]
pub struct SpaceEntity {
    /// 空间标识
    id: SpaceId,
    /// 是否自动订阅新入库番剧
    auto_subscribe: bool,
}

impl SpaceEntity {
    /// 基于空间快照构造订阅空间聚合根
    pub fn new(space: Space) -> Result<Self, DomainError> {
        if space.id.0 <= 0 {
            return Err(DomainError::InvariantViolation("space id must be positive"));
        }
        Ok(Self {
            id: space.id,
            auto_subscribe: space.auto_subscribe,
        })
    }

    /// 创建个人订阅空间聚合根
    pub fn personal(space_id: SpaceId, auto_subscribe: bool) -> Result<Self, DomainError> {
        Self::new(Space {
            id: space_id,
            auto_subscribe,
        })
    }

    pub fn get_space_rules(&self, repo: Arc<dyn SpaceRepository>) -> SpaceRules {
        {
            SpaceRules::new(repo, self.id)
        }
    }

    /// 读取空间快照
    pub fn read_data(&self) -> Space {
        Space { id: self.id, auto_subscribe: self.auto_subscribe }
    }

    /// 消费聚合根并返回空间快照
    pub fn into_snapshot(self) -> Space {
        self.read_data()
    }

    /// 设置是否自动订阅新番
    pub async fn set_auto_subscribe(
        &mut self,
        cap: &dyn SpaceAutoSubscribeCap,
        enabled: bool,
    ) -> Result<(), DomainError> {
        cap.write_auto_subscribe(self.id, enabled).await?;
        self.auto_subscribe = enabled;
        Ok(())
    }

    /// 信息专家：空间根据自身规则判断是否应自动订阅该番剧
    pub fn try_auto_subscribe(&self, anime_id: AnimeId) -> Option<AutoSubscribeDecision> {
        self.auto_subscribe.then_some(AutoSubscribeDecision {
            space_id: self.id,
            anime_id,
        })
    }
}

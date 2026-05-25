use domain::{
    anime::AnimeId,
    shared::error::DomainError,
    space::{AutoSubscribeDecision, Space, SpaceId},
    space::capability::SpaceAutoSubscribeCap,
};

/// 订阅空间聚合根。
#[derive(Debug, Clone)]
pub struct SpaceEntity {
    space: Space,
}

impl SpaceEntity {
    /// 基于空间快照构造订阅空间聚合根。
    pub fn new(space: Space) -> Result<Self, DomainError> {
        if space.id.0 <= 0 {
            return Err(DomainError::InvariantViolation("space id must be positive"));
        }
        Ok(Self { space })
    }

    /// 创建个人订阅空间聚合根。
    pub fn personal(space_id: SpaceId, auto_subscribe: bool) -> Result<Self, DomainError> {
        Self::new(Space {
            id: space_id,
            auto_subscribe,
        })
    }

    /// 读取空间快照。
    pub fn read_data(&self) -> &Space {
        &self.space
    }

    /// 消费聚合根并返回空间快照。
    pub fn into_snapshot(self) -> Space {
        self.space
    }

    /// 设置是否自动订阅新番。
    pub async fn set_auto_subscribe(
        &mut self,
        cap: &dyn SpaceAutoSubscribeCap,
        enabled: bool,
    ) -> Result<(), DomainError> {
        cap.write_auto_subscribe(self.space.id, enabled).await?;
        self.space.auto_subscribe = enabled;
        Ok(())
    }

    /// 信息专家：空间根据自身规则判断是否应自动订阅该番剧。
    pub fn try_auto_subscribe(&self, anime_id: AnimeId) -> Option<AutoSubscribeDecision> {
        self.space.auto_subscribe.then_some(AutoSubscribeDecision {
            space_id: self.space.id,
            anime_id,
        })
    }
}

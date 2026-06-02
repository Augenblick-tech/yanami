use async_trait::async_trait;

use crate::{anime::AnimeId, shared::biz::BizContext, shared::error::DomainError, user::UserId};

pub mod capability;

/// 订阅空间稳定标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpaceId(pub i64);

/// 订阅空间聚合的共享读写模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Space {
    /// 空间标识。
    pub id: SpaceId,
    /// 是否自动订阅新入库番剧。
    pub auto_subscribe: bool,
}

/// 空间决定自动订阅的结果，供 service 编排层使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoSubscribeDecision {
    pub space_id: SpaceId,
    pub anime_id: AnimeId,
}

/// 个人空间绑定信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalSpaceBinding {
    /// 个人空间标识。
    pub personal_space_id: SpaceId,
}

/// Space context 下的订阅空间仓储端口。
#[async_trait]
pub trait SpaceRepository: Send + Sync {
    /// 保存订阅空间。
    async fn save_subscription_space(&self, space: &Space) -> Result<(), DomainError>;

    /// 按空间标识读取订阅空间。
    async fn find_subscription_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Option<Space>, DomainError>;

    /// 按用户标识查找个人空间绑定。
    async fn find_personal_space_binding(
        &self,
        user_id: UserId,
    ) -> Result<Option<PersonalSpaceBinding>, DomainError>;

    /// 保存用户与个人空间的绑定关系。
    async fn save_personal_space_binding(
        &self,
        user_id: UserId,
        binding: &PersonalSpaceBinding,
    ) -> Result<(), DomainError>;

    /// 列出所有启用了自动订阅的空间。
    async fn list_auto_subscribing_spaces(&self) -> Result<Vec<Space>, DomainError>;

    /// 按空间 ID 批量查出绑定用户。
    async fn find_personal_space_user_ids(
        &self,
        space_ids: &[SpaceId],
    ) -> Result<Vec<(SpaceId, UserId)>, DomainError>;

    async fn list_spaces_rules(&self, space_id: SpaceId) -> Result<(), DomainError>;

    fn with_biz(&self, _: &BizContext) -> Result<std::sync::Arc<dyn SpaceRepository>, DomainError> {
        Err(DomainError::InvariantViolation(
            "space repository does not support biz context",
        ))
    }
}

use async_trait::async_trait;

use crate::{shared::biz::BizContext, shared::error::DomainError, space::SpaceId};

/// 内部标识分配端口。
#[async_trait]
pub trait IdSequence: Send + Sync {
    /// 生成订阅空间标识。
    async fn next_subscription_space_id(&self) -> Result<SpaceId, DomainError>;

    fn with_biz(&self, _: &BizContext) -> Result<std::sync::Arc<dyn IdSequence>, DomainError> {
        Err(DomainError::InvariantViolation(
            "id sequence does not support biz context",
        ))
    }
}

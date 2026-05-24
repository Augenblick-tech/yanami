use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    anime::AnimeId, shared::biz::BizContext, shared::error::DomainError, space::SpaceId,
    user::UserId,
};

use super::SubscriptionSearchState;

pub type SubscriptionPk = (UserId, SpaceId, AnimeId);

#[async_trait]
pub trait SubscriptionToggleCap: Send + Sync {
    async fn write_enabled(&self, pk: SubscriptionPk, enabled: bool) -> Result<(), DomainError>;

    async fn with_biz(
        &self,
        _: &BizContext,
    ) -> Result<Arc<dyn SubscriptionToggleCap>, DomainError> {
        Err(DomainError::InvariantViolation(
            "subscription toggle cap does not support biz context",
        ))
    }
}

#[async_trait]
pub trait SubscriptionMatchCap: Send + Sync {
    async fn write_match_result(
        &self,
        pk: SubscriptionPk,
        progress: i64,
        bound_rule: Option<String>,
        enabled: bool,
    ) -> Result<(), DomainError>;

    async fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn SubscriptionMatchCap>, DomainError> {
        Err(DomainError::InvariantViolation(
            "subscription match cap does not support biz context",
        ))
    }
}

#[async_trait]
pub trait SubscriptionSearchCap: Send + Sync {
    async fn write_search_state(
        &self,
        pk: SubscriptionPk,
        state: SubscriptionSearchState,
    ) -> Result<(), DomainError>;

    async fn batch_write_search_state(
        &self,
        pks: &[SubscriptionPk],
        state: SubscriptionSearchState,
    ) -> Result<(), DomainError>;

    async fn with_biz(
        &self,
        _: &BizContext,
    ) -> Result<Arc<dyn SubscriptionSearchCap>, DomainError> {
        Err(DomainError::InvariantViolation(
            "subscription search cap does not support biz context",
        ))
    }
}

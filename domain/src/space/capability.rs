use async_trait::async_trait;

use crate::{shared::error::DomainError, space::SpaceId};

#[async_trait]
pub trait SpaceAutoSubscribeCap: Send + Sync {
    async fn write_auto_subscribe(&self, space_id: SpaceId, auto_subscribe: bool) -> Result<(), DomainError>;
}

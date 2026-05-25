use async_trait::async_trait;

use crate::shared::biz::BizContext;
use crate::shared::error::DomainError;

#[async_trait]
pub trait SystemInfrastructureInitializer: Send + Sync {
    async fn initialize_infrastructure(&self, biz: &BizContext) -> Result<(), DomainError>;
}

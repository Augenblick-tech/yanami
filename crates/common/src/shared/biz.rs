use std::{any::Any, sync::Arc};

use async_trait::async_trait;

use crate::shared::error::Error;

#[async_trait]
pub trait InfraTxProvider: Send + Sync {
    fn as_any(&self) -> &(dyn Any + Send + Sync);

    async fn commit(&self) -> Result<(), Error>;

    async fn rollback(&self) -> Result<(), Error>;
}

#[async_trait]
pub trait BizFactory: Send + Sync {
    async fn open_biz(&self) -> Result<BizContext, Error>;
}

#[derive(Clone)]
pub struct BizContext {
    id: u64,
    provider: Arc<dyn InfraTxProvider>,
}

impl BizContext {
    pub fn new(id: u64, provider: Arc<dyn InfraTxProvider>) -> Self {
        Self { id, provider }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn provider(&self) -> &dyn InfraTxProvider {
        self.provider.as_ref()
    }

    pub async fn commit(&self) -> Result<(), Error> {
        self.provider.commit().await
    }

    pub async fn rollback(&self) -> Result<(), Error> {
        self.provider.rollback().await
    }
}

use domain::shared::error::DomainError;

use crate::entity::cap::SpaceAutoSubcribeCaps;

/// 订阅空间聚合根
#[derive(Debug, Clone)]
pub struct SpaceEntity {
    /// 空间标识
    id: u32,
    /// 是否自动订阅新入库番剧
    auto_subscribe: bool,
}

impl SpaceEntity {
    fn new(id: u32, auto_sub: bool) -> Self {
        Self {
            id,
            auto_subscribe: auto_sub,
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub async fn auto_subscribe(
        &mut self,
        cap: &dyn SpaceAutoSubcribeCaps,
    ) -> Result<(), DomainError> {
        if self.auto_subscribe {
            return Ok(());
        }
        cap.set_auto_subcribe(self.id, true).await?;
        self.auto_subscribe = true;
        Ok(())
    }

    pub async fn disable_auto_subscribe(
        &mut self,
        cap: &dyn SpaceAutoSubcribeCaps,
    ) -> Result<(), DomainError> {
        if !self.auto_subscribe {
            return Ok(());
        }
        cap.set_auto_subcribe(self.id, false).await?;
        self.auto_subscribe = false;
        Ok(())
    }

    pub fn is_auto_subscribe(&self) -> bool {
        self.auto_subscribe
    }
}

use domain::shared::error::DomainError;

use crate::entity::cap::{RegexCaps, UpdateRuleCaps};

/// 订阅空间规则聚合根
#[derive(Debug, Clone)]
pub struct RuleEntity {
    /// 规则标识
    id: u32,
    /// 规则展示名
    name: String,
    /// 规则顺序，值越小优先级越高
    order: u32,
    /// 可用于匹配资源标题的表达式
    pattern: String,
    /// 是否可被新订阅匹配选择
    active: bool,
    /// 所属订阅空间
    space_id: u32,
}

impl RuleEntity {
    pub(crate) fn new(
        id: u32,
        space_id: u32,
        name: String,
        order: u32,
        pattern: String,
        active: bool,
        cap: &dyn RegexCaps,
    ) -> Result<Self, DomainError> {
        cap.verify(&pattern)?;
        Ok(Self {
            id,
            name,
            order,
            pattern,
            active,
            space_id,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn match_title(
        &self,
        regex_provider: &dyn RegexCaps,
        title: &str,
    ) -> Result<bool, DomainError> {
        if regex_provider.is_match(&self.pattern, title)? {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn inactive(&mut self, cap: &dyn UpdateRuleCaps) -> Result<(), DomainError> {
        if !self.active {
            return Ok(());
        }
        cap.inactive(self.id).await?;
        self.active = false;
        Ok(())
    }

    pub async fn set_pattern(
        &mut self,
        pattern: String,
        regex_cap: &dyn RegexCaps,
        update_pattern_cap: &dyn UpdateRuleCaps,
    ) -> Result<(), DomainError> {
        regex_cap.verify(&pattern)?;
        update_pattern_cap.update_pattern(self.id, &pattern, &self.pattern).await?;
        self.pattern = pattern;
        Ok(())
    }

    pub async fn set_order(
        &mut self,
        order: u32,
        cap: &dyn UpdateRuleCaps,
    ) -> Result<(), DomainError> {
        cap.update_order(self.id, order).await?;
        self.order = order;
        Ok(())
    }
}

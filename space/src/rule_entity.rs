use domain::{
    rule::RegexProvider,
    shared::error::DomainError,
};

/// 订阅空间规则聚合根
#[derive(Debug, Clone)]
pub struct RuleEntity {
    /// 规则标识。
    id: u32,
    /// 规则展示名。
    name: String,
    /// 规则顺序，值越小优先级越高。
    order: u32,
    /// 可用于匹配资源标题的表达式。
    pattern: String,
    /// 是否可被新订阅匹配选择。
    active: bool,
}

impl RuleEntity {
    fn new(id: u32, name: String, order: u32, pattern: String, active: bool) -> Self {
        Self { id: id, name: name, order: order, pattern: pattern, active: active }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn match_title(
        &self,
        regex_provider: &dyn RegexProvider,
        title: &str,
    ) -> Result<bool, DomainError> {
        if regex_provider.is_match(&self.pattern, title)? {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

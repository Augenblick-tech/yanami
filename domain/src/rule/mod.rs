use std::sync::Arc;

use crate::{shared::biz::BizContext, shared::error::DomainError, space::SpaceId};

pub mod capability;

/// 规则稳定标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchingRuleId(pub String);

/// 单条匹配规则的共享读写模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchingRule {
    /// 规则标识。
    pub id: MatchingRuleId,
    /// 规则展示名。
    pub name: String,
    /// 规则顺序，值越小优先级越高。
    pub order: u32,
    /// 可用于匹配资源标题的表达式。
    pub pattern: String,
    /// 是否可被新订阅匹配选择。
    pub active: bool,
}

/// 正则表达式编译端口。
/// 由基础设施层提供实现，可内建缓存避免重复编译。
pub trait RegexProvider: Send + Sync {
    fn is_match(&self, pattern: &str, text: &str) -> Result<bool, DomainError>;
}

/// Rule context 下的团队规则仓储端口。
#[async_trait::async_trait]
pub trait SpaceRuleRepository: Send + Sync {
    /// 读取某个空间当前可用于新匹配的规则列表。
    async fn find_active_space_rules(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<MatchingRule>, DomainError>;

    /// 按稳定标识读取规则，包含已失活规则。
    async fn find_space_rule(
        &self,
        space_id: SpaceId,
        rule_id: &MatchingRuleId,
    ) -> Result<Option<MatchingRule>, DomainError>;

    /// 按规则名读取规则，包含已失活规则。
    async fn find_space_rule_by_name(
        &self,
        space_id: SpaceId,
        name: &str,
    ) -> Result<Option<MatchingRule>, DomainError>;

    /// 保存单条空间规则。
    async fn save_space_rule(
        &self,
        space_id: SpaceId,
        rule: &MatchingRule,
    ) -> Result<(), DomainError>;

    fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn SpaceRuleRepository>, DomainError> {
        Err(DomainError::InvariantViolation(
            "space rule repository does not support biz context",
        ))
    }
}

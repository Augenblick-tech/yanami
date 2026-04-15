use anyhow::Error as AnyhowError;
use thiserror::Error;

/// 领域层统一错误类型。
#[derive(Debug, Error)]
pub enum DomainError {
    /// 聚合或值对象未满足不变量。
    #[error("domain invariant violation: {0}")]
    InvariantViolation(&'static str),
    /// 外部端口未满足领域契约。
    #[error("{context}")]
    ExternalContractMismatch {
        /// 边界上下文说明。
        context: &'static str,
        /// 保留原始错误链。
        #[source]
        source: AnyhowError,
    },
}

impl DomainError {
    /// 用于保留外部依赖的真实错误链。
    pub fn external(context: &'static str, source: impl Into<AnyhowError>) -> Self {
        Self::ExternalContractMismatch {
            context,
            source: source.into(),
        }
    }
}

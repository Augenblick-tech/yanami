use anyhow::Error as AnyhowError;
use thiserror::Error;

/// 领域层统一错误类型。
#[derive(Debug, Error)]
pub enum Error {
    /// 聚合或值对象未满足不变量。
    #[error("common invariant violation: {0}")]
    InvariantViolation(String),
    /// 实体未找到。
    #[error("not found: {0}")]
    NotFound(String),
    /// 数据冲突。
    #[error("conflict: {0}")]
    Conflict(String),
    /// 外部端口未满足领域契约。
    #[error("{context}")]
    ExternalContractMismatch {
        /// 边界上下文说明。
        context: String,
        /// 保留原始错误链。
        #[source]
        source: AnyhowError,
    },
}

impl Error {
    /// 用于保留外部依赖的真实错误链。
    pub fn external(msg: impl Into<String>, source: impl Into<AnyhowError>) -> Self {
        Self::ExternalContractMismatch {
            context: msg.into(),
            source: source.into(),
        }
    }

    pub fn invariant(msg: impl Into<String>) -> Self {
        Self::InvariantViolation(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invariant_format() {
        let err = Error::invariant("field X is required");
        let msg = err.to_string();
        assert!(msg.contains("field X is required"));
    }

    #[test]
    fn not_found_format() {
        let err = Error::not_found("entity 42");
        let msg = err.to_string();
        assert!(msg.contains("entity 42"));
    }

    #[test]
    fn conflict_format() {
        let err = Error::conflict("duplicate key");
        let msg = err.to_string();
        assert!(msg.contains("duplicate key"));
    }

    #[test]
    fn external_preserves_context() {
        let err = Error::external("db connect", anyhow::anyhow!("timeout"));
        let msg = err.to_string();
        assert!(msg.contains("db connect"));
    }
}

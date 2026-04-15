use anyhow::Error as AnyhowError;
use thiserror::Error;

use domain::shared::error::DomainError;

/// 应用层统一错误类型。
#[derive(Debug, Error)]
pub enum ApplicationError {
    /// 领域层返回的不变量错误。
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// 基础设施端口返回的错误。
    #[error(transparent)]
    Infrastructure(#[from] AnyhowError),
}

impl From<ApplicationError> for DomainError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::Domain(error) => error,
            ApplicationError::Infrastructure(error) => {
                DomainError::external("download infrastructure failed", error)
            }
        }
    }
}

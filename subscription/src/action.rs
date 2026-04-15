use std::{future::Future, pin::Pin};

use domain::user::UserId;

use crate::shared::error::ApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedResource {
    pub user_id: UserId,
    pub source_url: String,
    pub resource_id: String,
    pub relative_save_path: String,
}

pub type RunMatchedResourceFuture =
    Pin<Box<dyn Future<Output = Result<(), ApplicationError>> + Send>>;
pub type RunMatchedResource = dyn Fn(MatchedResource) -> RunMatchedResourceFuture + Send + Sync;

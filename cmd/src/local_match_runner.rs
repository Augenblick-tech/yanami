use std::sync::Arc;
use std::time::Duration;

use service::subscription::service::SubscriptionService;

/// 纯定时任务，不接触实体，不持有根关联对象，不调任何业务方法。
pub fn spawn_local_match_runner(subscription_service: Arc<SubscriptionService>) {
    tokio::spawn(async move {
        // Crash recovery: drain all available entries without interval
        while let Ok(true) = subscription_service.process_one_local_match().await {}

        // Normal loop: 1s interval
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Err(error) = subscription_service.process_one_local_match().await {
                tracing::warn!(?error, "local_match_runner: process_one_local_match failed");
            }
        }
    });
}

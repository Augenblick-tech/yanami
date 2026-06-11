use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dashmap::DashMap;

use crate::entity::cap::FeedAccessPolicy;

struct Value {
    /// 在此时间之前，对应 feed_id 不可访问
    deadline: Instant,
    /// 当前连续重试次数，用于计算下一次退避延迟
    retries: u32,
}

#[derive(Clone)]
pub struct BackoffPolicy {
    cache: Arc<DashMap<i64, Value>>,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl BackoffPolicy {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
        }
    }

    fn backoff_delay(retries: u32) -> Duration {
        let secs = 30u64
            .saturating_mul(2u64.saturating_pow(retries.saturating_sub(1)))
            .min(3600);
        Duration::from_secs(secs)
    }
}

impl FeedAccessPolicy for BackoffPolicy {
    fn block_feed_ids(&self) -> Vec<i64> {
        let now = Instant::now();
        self.cache
            .iter()
            .filter_map(|entry| {
                if entry.value().deadline > now {
                    Some(*entry.key())
                } else {
                    None
                }
            })
            .collect()
    }

    fn block_feed_details(&self) -> Vec<(i64, i64)> {
        let now = Instant::now();
        let sys_now = std::time::SystemTime::now();
        self.cache
            .iter()
            .filter_map(|entry| {
                if entry.value().deadline > now {
                    let dur = entry.value().deadline.duration_since(now);
                    let sys_deadline = sys_now + dur;
                    let ts = sys_deadline.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
                    Some((*entry.key(), ts))
                } else {
                    None
                }
            })
            .collect()
    }

    fn is_access(&self, feed_id: i64) -> bool {
        match self.cache.get(&feed_id) {
            Some(entry) => entry.deadline <= Instant::now(),
            None => true,
        }
    }

    fn note(&self, feed_id: i64, res: &crate::entity::model::FeedFetchResult) {
        match res {
            crate::entity::model::FeedFetchResult::Success(_) => {
                self.cache.remove(&feed_id);
            }
            crate::entity::model::FeedFetchResult::Retryable(_)
            | crate::entity::model::FeedFetchResult::Failure(_) => {
                let now = Instant::now();
                // 如果已经有有效的退避记录（未到期），则忽略本次调用，不延长时间
                if let Some(entry) = self.cache.get(&feed_id)
                    && entry.deadline > now {
                        return;
                    }
                // 无记录或已过期，执行退避更新
                self.cache
                    .entry(feed_id)
                    .and_modify(|entry| {
                        entry.retries += 1;
                        entry.deadline = now + BackoffPolicy::backoff_delay(entry.retries);
                    })
                    .or_insert_with(|| Value {
                        deadline: now + BackoffPolicy::backoff_delay(1),
                        retries: 1,
                    });
            }
            crate::entity::model::FeedFetchResult::Denied => {}
        }
    }
}

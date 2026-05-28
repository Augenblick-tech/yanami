use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::{feed::FeedSourceId, shared::error::DomainError};
use feed::contracts::SearchPoolEventHandler;
use infra::rss::HttpFeedFetcher;
use subscription::search_pool::SearchPool;

const INITIAL_BACKOFF: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(1800);
const SUCCESS_COOLDOWN: Duration = Duration::from_secs(3);

struct FeedBackoff {
    next_allowed_at: Instant,
    current_interval: Duration,
}

pub fn spawn_pool_consumer(
    search_pool: Arc<SearchPool>,
    http_feed: Arc<HttpFeedFetcher>,
    handlers: Vec<Arc<dyn SearchPoolEventHandler>>,
) {
    tokio::spawn(async move {
        let mut backoff: HashMap<FeedSourceId, FeedBackoff> = HashMap::new();
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let feed_ids = match search_pool.list_distinct_feed_ids().await {
                Ok(ids) => ids,
                Err(error) => {
                    tracing::error!(?error, "pool_consumer: list feed ids failed");
                    continue;
                }
            };

            if feed_ids.is_empty() {
                continue;
            }

            let now = Instant::now();
            let Some(feed_id) = feed_ids
                .iter()
                .find(|fid| {
                    backoff
                        .get(fid)
                        .map(|b| b.next_allowed_at <= now)
                        .unwrap_or(true)
                })
                .cloned()
            else {
                continue;
            };

            let entry = match search_pool.pick_random(&feed_id).await {
                Ok(Some(entry)) => entry,
                Ok(None) => continue,
                Err(error) => {
                    tracing::error!(?feed_id, ?error, "pool_consumer: pick random failed");
                    continue;
                }
            };

            for handler in &handlers {
                handler.on_search_started(entry.anime_id).await;
            }

            match http_feed.fetch_url(&entry.search_url).await {
                Ok(feed_data) => {
                    if let Err(error) = search_pool.consume_entry(&entry).await {
                        tracing::error!(
                            pool_id = entry.id,
                            ?error,
                            "pool_consumer: consume entry failed"
                        );
                    }
                    for handler in &handlers {
                        handler
                            .on_entry_succeeded(entry.anime_id, feed_data.clone())
                            .await;
                    }
                    tracing::debug!(
                        anime_id = %entry.anime_id.0,
                        keyword = %entry.keyword,
                        feed_id = %feed_id.0,
                        "pool_consumer: search completed"
                    );
                    backoff.insert(
                        feed_id,
                        FeedBackoff {
                            next_allowed_at: now + SUCCESS_COOLDOWN,
                            current_interval: INITIAL_BACKOFF,
                        },
                    );
                }
                Err(error) => {
                    let is_fatal = matches!(error, DomainError::InvariantViolation(_));
                    if is_fatal {
                        tracing::warn!(
                            anime_id = %entry.anime_id.0,
                            keyword = %entry.keyword,
                            feed_id = %feed_id.0,
                            ?error,
                            "pool_consumer: fatal error, deleting entry"
                        );
                        if let Err(e) = search_pool.consume_entry(&entry).await {
                            tracing::error!(
                                pool_id = entry.id,
                                ?e,
                                "pool_consumer: consume entry failed"
                            );
                        }
                        for handler in &handlers {
                            handler.on_entry_failed(entry.anime_id).await;
                        }
                    } else {
                        let interval = backoff
                            .get(&feed_id)
                            .map(|b| std::cmp::min(b.current_interval * 2, MAX_BACKOFF))
                            .unwrap_or(INITIAL_BACKOFF);
                        tracing::warn!(
                            anime_id = %entry.anime_id.0,
                            keyword = %entry.keyword,
                            feed_id = %feed_id.0,
                            retry_in_secs = interval.as_secs(),
                            ?error,
                            "pool_consumer: retryable error, will retry"
                        );
                        backoff.insert(
                            feed_id.clone(),
                            FeedBackoff {
                                next_allowed_at: now + interval,
                                current_interval: interval,
                            },
                        );
                    }
                }
            }
        }
    });
}

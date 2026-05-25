use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
};

use async_trait::async_trait;
use domain::{
    download::{
        DownloadRequest, UserDownloadDriver, UserDownloadDriverBindingStore,
        UserQbitDownloadProfile,
    },
    shared::error::DomainError,
    user::UserId,
};

use crate::download::{
    contracts::UserDownloadExecutor,
    shared::error::ApplicationError,
};

pub type ResolveDriverKeyFuture =
    Pin<Box<dyn Future<Output = Result<Option<String>, ApplicationError>> + Send>>;
pub type ResolveUserDownloadDriver = dyn Fn(UserId) -> ResolveDriverKeyFuture + Send + Sync;

pub type VerifyQbitProfileFuture =
    Pin<Box<dyn Future<Output = Result<(), ApplicationError>> + Send>>;
pub type VerifyQbitProfile =
    dyn Fn(UserQbitDownloadProfile) -> VerifyQbitProfileFuture + Send + Sync;

pub type InvalidateUserDownloadRuntime = dyn Fn(UserId) + Send + Sync;

/// 将多个下载运行时缓存失效器合并成一个。
pub struct CompositeUserDownloadRuntimeCacheInvalidator {
    invalidators: Vec<Arc<InvalidateUserDownloadRuntime>>,
}

impl CompositeUserDownloadRuntimeCacheInvalidator {
    pub fn new(invalidators: Vec<Arc<InvalidateUserDownloadRuntime>>) -> Self {
        Self { invalidators }
    }

    pub fn invalidate_user_runtime(&self, user_id: UserId) {
        for invalidator in &self.invalidators {
            invalidator(user_id);
        }
    }
}

/// 按用户配置将下载请求路由到具体驱动。
pub struct RoutingUserDownloadExecutor {
    resolve_driver_key: Arc<ResolveUserDownloadDriver>,
    drivers: HashMap<&'static str, Arc<dyn UserDownloadDriver>>,
}

impl RoutingUserDownloadExecutor {
    /// 返回所有已注册的下载驱动标识。
    pub fn driver_keys(&self) -> Vec<&'static str> {
        self.drivers.keys().copied().collect()
    }

    pub fn new(
        resolve_driver_key: Arc<ResolveUserDownloadDriver>,
        drivers: Vec<Arc<dyn UserDownloadDriver>>,
    ) -> Result<Self, ApplicationError> {
        let mut indexed = HashMap::new();

        for driver in drivers {
            let key = driver.driver_key();
            if indexed.insert(key, driver).is_some() {
                return Err(ApplicationError::Infrastructure(anyhow::anyhow!(
                    "duplicate user download driver registered: {key}"
                )));
            }
        }

        Ok(Self {
            resolve_driver_key,
            drivers: indexed,
        })
    }

    pub async fn download_for_user(
        &self,
        user_id: UserId,
        request: &DownloadRequest,
    ) -> Result<(), DomainError> {
        let Some(driver_key) = (self.resolve_driver_key)(user_id)
            .await
            .map_err(DomainError::from)?
        else {
            return Err(DomainError::InvariantViolation(
                "user download driver not configured",
            ));
        };

        let Some(driver) = self.drivers.get(driver_key.as_str()) else {
            return Err(DomainError::external(
                "user download driver is not supported",
                anyhow::anyhow!("driver_key={driver_key}"),
            ));
        };

        driver
            .download(user_id, request)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl UserDownloadExecutor for RoutingUserDownloadExecutor {
    async fn download_for_user(
        &self,
        user_id: UserId,
        request: &DownloadRequest,
    ) -> Result<(), DomainError> {
        Self::download_for_user(self, user_id, request).await
    }
}

/// 按用户缓存下载器选择，避免每次下载都查询存储。
pub struct CachingUserDownloadDriverResolver {
    store: Arc<dyn UserDownloadDriverBindingStore>,
    capacity: usize,
    access_counter: AtomicU64,
    cache: RwLock<HashMap<UserId, CachedDriverBinding>>,
}

impl CachingUserDownloadDriverResolver {
    pub fn new(store: Arc<dyn UserDownloadDriverBindingStore>, capacity: usize) -> Self {
        Self {
            store,
            capacity,
            access_counter: AtomicU64::new(0),
            cache: RwLock::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn cache_len(&self) -> usize {
        self.cache.read().expect("read driver cache").len()
    }

    fn next_access_order(&self) -> u64 {
        self.access_counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn with_cache_read<R>(&self, f: impl FnOnce(&HashMap<UserId, CachedDriverBinding>) -> R) -> R {
        match self.cache.read() {
            Ok(cache) => f(&cache),
            Err(_) => f(&HashMap::new()),
        }
    }

    fn with_cache_write<R>(
        &self,
        f: impl FnOnce(&mut HashMap<UserId, CachedDriverBinding>) -> R,
    ) -> R {
        match self.cache.write() {
            Ok(mut cache) => f(&mut cache),
            Err(poisoned) => {
                let mut cache = poisoned.into_inner();
                cache.clear();
                f(&mut cache)
            }
        }
    }
}

impl CachingUserDownloadDriverResolver {
    pub async fn resolve_driver_key(
        &self,
        user_id: UserId,
    ) -> Result<Option<String>, ApplicationError> {
        let access_order = self.next_access_order();
        if let Some(driver_key) = self.with_cache_read(|cache| {
            cache
                .get(&user_id)
                .map(|binding| binding.driver_key.clone())
        }) {
            self.with_cache_write(|cache| {
                if let Some(binding) = cache.get_mut(&user_id) {
                    binding.last_used = access_order;
                }
            });
            return Ok(driver_key);
        }

        let driver_key = self.store.find_driver_key(user_id).await?;
        self.with_cache_write(|cache| {
            cache.insert(
                user_id,
                CachedDriverBinding {
                    driver_key: driver_key.clone(),
                    last_used: access_order,
                },
            );
            if cache.len() > self.capacity {
                let evicted = cache
                    .iter()
                    .min_by_key(|(_, binding)| binding.last_used)
                    .map(|(user_id, _)| *user_id);
                if let Some(evicted) = evicted {
                    cache.remove(&evicted);
                }
            }
        });
        Ok(driver_key)
    }

    pub fn invalidate_user_runtime(&self, user_id: UserId) {
        self.with_cache_write(|cache| {
            cache.remove(&user_id);
        });
    }
}

struct CachedDriverBinding {
    driver_key: Option<String>,
    last_used: u64,
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use domain::{
        download::DownloadRequest,
        shared::error::DomainError,
        user::UserId,
    };

    use super::*;
    use crate::download::shared::error::ApplicationError;

    fn fixed_resolver(key: Option<String>) -> Arc<ResolveUserDownloadDriver> {
        Arc::new(move |_user_id| {
            let key = key.clone();
            Box::pin(async move { Ok::<Option<String>, ApplicationError>(key) })
        })
    }

    struct RecordingDriver {
        key: &'static str,
        seen: Mutex<Vec<UserId>>,
    }

    #[derive(Default)]
    struct RecordingInvalidator {
        invalidated: Mutex<Vec<UserId>>,
    }

    #[derive(Default)]
    struct InMemoryDriverBindingStore {
        values: Mutex<HashMap<UserId, Option<String>>>,
        reads: Mutex<u32>,
    }

    #[async_trait]
    impl UserDownloadDriverBindingStore for InMemoryDriverBindingStore {
        async fn find_driver_key(
            &self,
            user_id: UserId,
        ) -> Result<Option<String>, DomainError> {
            *self.reads.lock().expect("lock reads") += 1;
            Ok(self
                .values
                .lock()
                .expect("lock values")
                .get(&user_id)
                .cloned()
                .unwrap_or(None))
        }

        async fn save_driver_key(
            &self,
            user_id: UserId,
            driver_key: &str,
        ) -> Result<(), DomainError> {
            self.values
                .lock()
                .expect("lock values")
                .insert(user_id, Some(driver_key.to_string()));
            Ok(())
        }
    }

    #[async_trait]
    impl UserDownloadDriver for RecordingDriver {
        fn driver_key(&self) -> &'static str {
            self.key
        }

        async fn download(
            &self,
            user_id: UserId,
            _request: &DownloadRequest,
        ) -> Result<(), DomainError> {
            self.seen.lock().expect("lock seen").push(user_id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn routing_executor_downloads_with_selected_driver() {
        let driver = Arc::new(RecordingDriver {
            key: "qbit",
            seen: Mutex::new(vec![]),
        });
        let executor = RoutingUserDownloadExecutor::new(
            fixed_resolver(Some("qbit".to_string())),
            vec![driver.clone()],
        )
        .expect("build routing executor");

        executor
            .download_for_user(
                UserId(7),
                &DownloadRequest {
                    source_url: "magnet:?xt=urn:btih:test".to_string(),
                    resource_id: "hash".to_string(),
                    relative_target_path: "Frieren/S01".to_string(),
                },
            )
            .await
            .expect("download succeeds");

        assert_eq!(
            driver.seen.lock().expect("lock seen").as_slice(),
            &[UserId(7)]
        );
    }

    #[tokio::test]
    async fn routing_executor_rejects_unknown_driver() {
        let executor =
            RoutingUserDownloadExecutor::new(fixed_resolver(Some("missing".to_string())), vec![])
                .expect("build routing executor");

        let error = executor
            .download_for_user(
                UserId(7),
                &DownloadRequest {
                    source_url: "magnet:?xt=urn:btih:test".to_string(),
                    resource_id: "hash".to_string(),
                    relative_target_path: "Frieren/S01".to_string(),
                },
            )
            .await
            .expect_err("download must fail");

        assert!(matches!(
            error,
            DomainError::ExternalContractMismatch { .. }
        ));
        assert_eq!(error.to_string(), "user download driver is not supported");
    }

    #[test]
    fn routing_executor_rejects_duplicate_driver_registration() {
        let first = Arc::new(RecordingDriver {
            key: "qbit",
            seen: Mutex::new(vec![]),
        });
        let second = Arc::new(RecordingDriver {
            key: "qbit",
            seen: Mutex::new(vec![]),
        });

        let result = RoutingUserDownloadExecutor::new(fixed_resolver(None), vec![first, second]);

        assert_eq!(
            result
                .err()
                .expect("duplicate registration must fail")
                .to_string(),
            "duplicate user download driver registered: qbit"
        );
    }

    #[tokio::test]
    async fn caching_driver_resolver_hits_store_once_before_invalidation() {
        let store = Arc::new(InMemoryDriverBindingStore::default());
        store
            .values
            .lock()
            .expect("lock values")
            .insert(UserId(7), Some("qbit".to_string()));
        let resolver = CachingUserDownloadDriverResolver::new(store.clone(), 8);

        let first = resolver
            .resolve_driver_key(UserId(7))
            .await
            .expect("first resolve");
        let second = resolver
            .resolve_driver_key(UserId(7))
            .await
            .expect("second resolve");

        assert_eq!(first.as_deref(), Some("qbit"));
        assert_eq!(second.as_deref(), Some("qbit"));
        assert_eq!(*store.reads.lock().expect("lock reads"), 1);

        resolver.invalidate_user_runtime(UserId(7));
        let third = resolver
            .resolve_driver_key(UserId(7))
            .await
            .expect("third resolve");

        assert_eq!(third.as_deref(), Some("qbit"));
        assert_eq!(*store.reads.lock().expect("lock reads"), 2);
    }

    #[tokio::test]
    async fn caching_driver_resolver_is_bounded() {
        let store = Arc::new(InMemoryDriverBindingStore::default());
        let resolver = CachingUserDownloadDriverResolver::new(store.clone(), 2);

        store
            .values
            .lock()
            .expect("lock values")
            .insert(UserId(1), Some("a".to_string()));
        store
            .values
            .lock()
            .expect("lock values")
            .insert(UserId(2), Some("b".to_string()));
        store
            .values
            .lock()
            .expect("lock values")
            .insert(UserId(3), Some("c".to_string()));

        resolver
            .resolve_driver_key(UserId(1))
            .await
            .expect("resolve a");
        resolver
            .resolve_driver_key(UserId(2))
            .await
            .expect("resolve b");
        resolver
            .resolve_driver_key(UserId(3))
            .await
            .expect("resolve c");

        assert_eq!(resolver.cache_len(), 2);
    }

    #[test]
    fn composite_invalidator_forwards_to_all_children() {
        let first = Arc::new(RecordingInvalidator::default());
        let second = Arc::new(RecordingInvalidator::default());
        let composite = CompositeUserDownloadRuntimeCacheInvalidator::new(vec![
            Arc::new({
                let first = first.clone();
                move |user_id| {
                    first
                        .invalidated
                        .lock()
                        .expect("lock invalidated")
                        .push(user_id);
                }
            }),
            Arc::new({
                let second = second.clone();
                move |user_id| {
                    second
                        .invalidated
                        .lock()
                        .expect("lock invalidated")
                        .push(user_id);
                }
            }),
        ]);

        composite.invalidate_user_runtime(UserId(11));

        assert_eq!(
            first.invalidated.lock().expect("lock first").as_slice(),
            &[UserId(11)]
        );
        assert_eq!(
            second.invalidated.lock().expect("lock second").as_slice(),
            &[UserId(11)]
        );
    }
}

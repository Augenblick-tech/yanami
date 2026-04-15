use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

use async_trait::async_trait;
use domain::{shared::error::DomainError, user::UserId};
use reqwest::{multipart::Form, Client, ClientBuilder, StatusCode, Url};
use service::download::runtime::{UserDownloadDriver, UserQbitDownloadProfile};
use service::download::shared::error::ApplicationError;
use service::download::DownloadRequest;
use tokio::{sync::Mutex as AsyncMutex, time};

use crate::db::{SqliteDb, StoredUserQbitDownloadProfile};

/// qBittorrent 会话池。
pub struct QbitSessionPool {
    session_capacity: usize,
    access_counter: AtomicU64,
    sessions: Mutex<HashMap<u64, Arc<QbitSession>>>,
}

impl Default for QbitSessionPool {
    fn default() -> Self {
        Self::new(32)
    }
}

impl QbitSessionPool {
    pub fn new(session_capacity: usize) -> Self {
        Self {
            session_capacity,
            access_counter: AtomicU64::new(0),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn cache_len(&self) -> usize {
        self.sessions.lock().expect("lock sessions").len()
    }

    fn touch_counter(&self) -> u64 {
        self.access_counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn with_sessions<R>(&self, f: impl FnOnce(&mut HashMap<u64, Arc<QbitSession>>) -> R) -> R {
        match self.sessions.lock() {
            Ok(mut sessions) => f(&mut sessions),
            Err(poisoned) => {
                let mut sessions = poisoned.into_inner();
                sessions.clear();
                f(&mut sessions)
            }
        }
    }

    fn session_for(
        &self,
        profile: &StoredUserQbitDownloadProfile,
    ) -> Result<Arc<QbitSession>, DomainError> {
        let key = profile_fingerprint(profile);
        let access_order = self.touch_counter();
        self.with_sessions(|sessions| {
            if let Some(session) = sessions.get(&key).cloned() {
                session.last_used.store(access_order, Ordering::SeqCst);
                return Ok(session);
            }

            let session = Arc::new(QbitSession::new(profile.clone())?);
            session.last_used.store(access_order, Ordering::SeqCst);
            sessions.insert(key, session.clone());

            if sessions.len() > self.session_capacity {
                let remove_key = sessions
                    .iter()
                    .min_by_key(|(_, session)| session.last_used.load(Ordering::SeqCst))
                    .map(|(key, _)| *key);
                if let Some(remove_key) = remove_key {
                    sessions.remove(&remove_key);
                }
            }

            Ok(session)
        })
    }
}

struct QbitSession {
    profile: StoredUserQbitDownloadProfile,
    client: Client,
    login_lock: AsyncMutex<()>,
    last_used: AtomicU64,
}

impl QbitSession {
    fn new(profile: StoredUserQbitDownloadProfile) -> Result<Self, DomainError> {
        let client = ClientBuilder::new()
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| DomainError::external("qbit client build failed", error))?;

        Ok(Self {
            profile,
            client,
            login_lock: AsyncMutex::new(()),
            last_used: AtomicU64::new(0),
        })
    }

    async fn add_torrent(&self, request: &DownloadRequest) -> Result<(), DomainError> {
        self.check_and_login().await?;
        let response = self.send_add_torrent(request).await?;

        let status = response.status();
        if status == StatusCode::FORBIDDEN {
            self.login().await?;
            let retry = self.send_add_torrent(request).await?;
            if retry.status() != StatusCode::OK {
                let retry_status = retry.status();
                let body = retry.text().await.map_err(|error| {
                    DomainError::external("qbit add torrent retry body read failed", error)
                })?;
                return Err(DomainError::external(
                    "qbit add torrent rejected",
                    anyhow::anyhow!("status={retry_status}, body={body}"),
                ));
            }
        } else if status != StatusCode::OK {
            let body = response.text().await.map_err(|error| {
                DomainError::external("qbit add torrent body read failed", error)
            })?;
            return Err(DomainError::external(
                "qbit add torrent rejected",
                anyhow::anyhow!("status={status}, body={body}"),
            ));
        }

        for _ in 0..5 {
            time::sleep(Duration::from_secs(5)).await;
            if self
                .check_torrent_in_down_record(&request.resource_id)
                .await?
            {
                return Ok(());
            }
        }

        Err(DomainError::InvariantViolation(
            "qbit torrent not visible after submission",
        ))
    }

    async fn send_add_torrent(
        &self,
        request: &DownloadRequest,
    ) -> Result<reqwest::Response, DomainError> {
        self.client
            .post(self.join_url("api/v2/torrents/add")?)
            .multipart(
                Form::new()
                    .text("urls", request.source_url.clone())
                    .text("autoTMM", "false")
                    .text(
                        "savepath",
                        self.build_save_path(&request.relative_target_path)?,
                    )
                    .text("paused", "false")
                    .text("stopCondition", "None")
                    .text("contentLayout", "Original")
                    .text("upLimit", "NaN")
                    .text("downLimit", "NaN"),
            )
            .send()
            .await
            .map_err(|error| DomainError::external("qbit add torrent request failed", error))
    }

    async fn check_and_login(&self) -> Result<(), DomainError> {
        let response = self
            .client
            .get(self.join_url("api/v2/app/version")?)
            .send()
            .await
            .map_err(|error| DomainError::external("qbit version request failed", error))?;
        let body = response
            .text()
            .await
            .map_err(|error| DomainError::external("qbit version response read failed", error))?;

        if body.contains("Forbidden") {
            self.login().await?;
        }

        Ok(())
    }

    async fn login(&self) -> Result<(), DomainError> {
        let guard = self.login_lock.lock().await;
        let response = self
            .client
            .post(self.join_url("api/v2/auth/login")?)
            .form(&[
                ("username", self.profile.username.to_string()),
                ("password", self.profile.secret.to_string()),
            ])
            .send()
            .await
            .map_err(|error| DomainError::external("qbit login request failed", error))?;

        if response.status() != StatusCode::OK {
            return Err(DomainError::external(
                "qbit login rejected",
                anyhow::anyhow!("status={}", response.status()),
            ));
        }

        drop(guard);
        Ok(())
    }

    async fn check_torrent_in_down_record(&self, resource_id: &str) -> Result<bool, DomainError> {
        let response = self
            .client
            .get(self.join_url("api/v2/sync/maindata")?)
            .send()
            .await
            .map_err(|error| DomainError::external("qbit state request failed", error))?;

        if response.status() != StatusCode::OK {
            return Err(DomainError::external(
                "qbit state rejected",
                anyhow::anyhow!("status={}", response.status()),
            ));
        }

        let body = response
            .text()
            .await
            .map_err(|error| DomainError::external("qbit state response read failed", error))?;

        Ok(body.contains(resource_id))
    }

    fn join_url(&self, path: &str) -> Result<Url, DomainError> {
        Url::parse(&self.profile.endpoint)
            .map_err(|error| DomainError::external("qbit endpoint parse failed", error))?
            .join(path)
            .map_err(|error| DomainError::external("qbit endpoint join failed", error))
    }

    fn build_save_path(&self, relative_target_path: &str) -> Result<String, DomainError> {
        Path::new(&self.profile.download_path)
            .join(relative_target_path)
            .to_str()
            .map(ToOwned::to_owned)
            .ok_or(DomainError::InvariantViolation(
                "qbit save path is not valid utf-8",
            ))
    }
}

fn profile_fingerprint(profile: &StoredUserQbitDownloadProfile) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    profile.endpoint.hash(&mut hasher);
    profile.username.hash(&mut hasher);
    profile.secret.hash(&mut hasher);
    profile.download_path.hash(&mut hasher);
    hasher.finish()
}

pub struct QbitDownloadDriver {
    sessions: QbitSessionPool,
}

impl QbitDownloadDriver {
    pub fn new() -> Self {
        Self {
            sessions: QbitSessionPool::default(),
        }
    }

    pub(crate) async fn download_with_profile(
        &self,
        profile: &StoredUserQbitDownloadProfile,
        request: &DownloadRequest,
    ) -> Result<(), DomainError> {
        let session = self.sessions.session_for(profile)?;
        session.add_torrent(request).await
    }
}

#[async_trait]
impl UserDownloadDriver for QbitDownloadDriver {
    fn driver_key(&self) -> &'static str {
        "qbit"
    }

    async fn download(&self, _: UserId, _: &DownloadRequest) -> Result<(), ApplicationError> {
        Err(ApplicationError::Domain(DomainError::InvariantViolation(
            "qbit driver requires profile-aware download",
        )))
    }
}

impl Default for QbitDownloadDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// 使用真实 qbit 服务验证配置可用性。
pub struct LiveQbitProfileVerifier;

impl LiveQbitProfileVerifier {
    pub async fn verify_qbit_profile(
        &self,
        profile: &UserQbitDownloadProfile,
    ) -> Result<(), ApplicationError> {
        let session = QbitSession::new(StoredUserQbitDownloadProfile {
            user_id: 0,
            endpoint: profile.endpoint.clone(),
            username: profile.username.clone(),
            secret: profile.secret.clone(),
            download_path: profile.download_path.clone(),
        })?;
        session.check_and_login().await?;
        Ok(())
    }
}

pub struct CachedQbitProfileLoader {
    db: Arc<SqliteDb>,
    capacity: usize,
    access_counter: AtomicU64,
    cache: RwLock<HashMap<UserId, CachedQbitProfile>>,
}

impl CachedQbitProfileLoader {
    pub fn new(db: Arc<SqliteDb>, capacity: usize) -> Self {
        Self {
            db,
            capacity,
            access_counter: AtomicU64::new(0),
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn with_cache_read<R>(&self, f: impl FnOnce(&HashMap<UserId, CachedQbitProfile>) -> R) -> R {
        match self.cache.read() {
            Ok(cache) => f(&cache),
            Err(_) => f(&HashMap::new()),
        }
    }

    fn with_cache_write<R>(
        &self,
        f: impl FnOnce(&mut HashMap<UserId, CachedQbitProfile>) -> R,
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

    #[cfg(test)]
    fn cache_len(&self) -> usize {
        self.cache.read().expect("read qbit profile cache").len()
    }

    async fn load_qbit_profile(
        &self,
        user_id: UserId,
    ) -> Result<Option<StoredUserQbitDownloadProfile>, ApplicationError> {
        let access_order = self.access_counter.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(profile) =
            self.with_cache_read(|cache| cache.get(&user_id).map(|cached| cached.profile.clone()))
        {
            self.with_cache_write(|cache| {
                if let Some(cached) = cache.get_mut(&user_id) {
                    cached.last_used = access_order;
                }
            });
            return Ok(profile);
        }

        let profile = self
            .db
            .load_user_qbit_download_profile(user_id)
            .await
            .map_err(ApplicationError::from)?;
        self.with_cache_write(|cache| {
            cache.insert(
                user_id,
                CachedQbitProfile {
                    profile: profile.clone(),
                    last_used: access_order,
                },
            );
            if cache.len() > self.capacity {
                let evicted = cache
                    .iter()
                    .min_by_key(|(_, profile)| profile.last_used)
                    .map(|(user_id, _)| *user_id);
                if let Some(evicted) = evicted {
                    cache.remove(&evicted);
                }
            }
        });
        Ok(profile)
    }
}

impl CachedQbitProfileLoader {
    pub fn invalidate_user_runtime(&self, user_id: UserId) {
        self.with_cache_write(|cache| {
            cache.remove(&user_id);
        });
    }
}

struct CachedQbitProfile {
    profile: Option<StoredUserQbitDownloadProfile>,
    last_used: u64,
}

pub struct UserBoundQbitDownloadDriver {
    inner: Arc<QbitDownloadDriver>,
    profile_loader: Arc<CachedQbitProfileLoader>,
}

impl UserBoundQbitDownloadDriver {
    pub fn from_sqlite(
        db: Arc<SqliteDb>,
        inner: Arc<QbitDownloadDriver>,
        profile_cache_capacity: usize,
    ) -> Self {
        Self {
            inner,
            profile_loader: Arc::new(CachedQbitProfileLoader::new(db, profile_cache_capacity)),
        }
    }

    pub fn cache_invalidator(&self) -> Arc<CachedQbitProfileLoader> {
        self.profile_loader.clone()
    }
}

#[async_trait]
impl UserDownloadDriver for UserBoundQbitDownloadDriver {
    fn driver_key(&self) -> &'static str {
        self.inner.driver_key()
    }

    async fn download(
        &self,
        user_id: UserId,
        request: &DownloadRequest,
    ) -> Result<(), ApplicationError> {
        let profile = self
            .profile_loader
            .load_qbit_profile(user_id)
            .await?
            .ok_or(ApplicationError::Domain(DomainError::InvariantViolation(
                "user qbit download profile not found",
            )))?;
        self.inner.download_with_profile(&profile, request).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::user::UserId;

    fn profile(endpoint: &str, username: &str) -> StoredUserQbitDownloadProfile {
        StoredUserQbitDownloadProfile {
            user_id: 1,
            endpoint: endpoint.to_string(),
            username: username.to_string(),
            secret: "secret".to_string(),
            download_path: "/downloads".to_string(),
        }
    }

    #[test]
    fn session_cache_is_bounded() {
        let sessions = QbitSessionPool::new(2);

        sessions
            .session_for(&profile("http://a", "a"))
            .expect("session a");
        sessions
            .session_for(&profile("http://b", "b"))
            .expect("session b");
        sessions
            .session_for(&profile("http://c", "c"))
            .expect("session c");

        assert_eq!(sessions.cache_len(), 2);
    }

    #[tokio::test]
    async fn cached_qbit_profile_loader_is_bounded_and_invalidation_works() {
        let database = Arc::new(
            SqliteDb::new("sqlite::memory:", "test-app-key")
                .await
                .expect("database"),
        );
        database
            .save_user_qbit_download_profile(&StoredUserQbitDownloadProfile {
                user_id: 1,
                endpoint: "http://a".to_string(),
                username: "a".to_string(),
                secret: "secret".to_string(),
                download_path: "/a".to_string(),
            })
            .await
            .expect("save profile a");
        database
            .save_user_qbit_download_profile(&StoredUserQbitDownloadProfile {
                user_id: 2,
                endpoint: "http://b".to_string(),
                username: "b".to_string(),
                secret: "secret".to_string(),
                download_path: "/b".to_string(),
            })
            .await
            .expect("save profile b");
        database
            .save_user_qbit_download_profile(&StoredUserQbitDownloadProfile {
                user_id: 3,
                endpoint: "http://c".to_string(),
                username: "c".to_string(),
                secret: "secret".to_string(),
                download_path: "/c".to_string(),
            })
            .await
            .expect("save profile c");
        let loader = CachedQbitProfileLoader::new(database.clone(), 2);

        loader.load_qbit_profile(UserId(1)).await.expect("load a");
        loader.load_qbit_profile(UserId(2)).await.expect("load b");
        loader.load_qbit_profile(UserId(3)).await.expect("load c");

        assert_eq!(loader.cache_len(), 2);

        loader.invalidate_user_runtime(UserId(2));
        assert_eq!(loader.cache_len(), 1);
    }
}

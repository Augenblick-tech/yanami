use anyhow::anyhow;
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use common::shared::error::Error;

use crate::entity::model::{DownloaderConfig, UserBaseData, UserRole};

use crate::entity::cap::CryptoProvider;
use std::sync::Arc;

#[derive(Clone)]
pub struct UserEntity {
    data: UserBaseData,
    crypto_provider: Arc<dyn CryptoProvider>,
}

impl UserEntity {
    pub(super) fn new(data: UserBaseData, crypto_provider: Arc<dyn CryptoProvider>) -> Self {
        Self {
            data,
            crypto_provider,
        }
    }

    pub(super) fn get_base_data(&self) -> &UserBaseData {
        &self.data
    }

    pub fn id(&self) -> i64 {
        self.data.id
    }

    pub fn username(&self) -> &str {
        &self.data.username
    }

    pub fn role(&self) -> UserRole {
        self.data.role
    }

    pub fn space_id(&self) -> i64 {
        self.data.space_id
    }

    pub fn auto_sub(&self) -> bool {
        self.data.auto_sub
    }

    pub fn get_download_config(&self) -> &Vec<DownloaderConfig> {
        &self.data.download_config
    }

    pub fn download_config(&self) -> Result<Option<DownloaderConfig>, Error> {
        let Some(c) = self.data.download_config.iter().find(|i| i.is_active()) else {
            return Ok(None);
        };
        let mut clone = c.clone();
        clone.decrypt_secrets(self.crypto_provider.as_ref())?;
        Ok(Some(clone))
    }

    pub fn delete_download_config(&mut self, config_name: &str) {
        self.data
            .download_config
            .retain(|c| c.name() != config_name);
    }

    pub fn enable_download_config(&mut self, config_name: &str) -> Result<(), Error> {
        let mut found = false;

        for config in &mut self.data.download_config {
            if config.name() == config_name {
                config.set_active(true);
                found = true;
            } else if config.is_active() {
                config.set_active(false);
            }
        }

        if !found {
            return Err(Error::conflict(format!(
                "not found download config {}",
                config_name
            )));
        }

        Ok(())
    }

    pub fn save_download_config(&mut self, mut config: DownloaderConfig) -> Result<(), Error> {
        let new_is_active = config.is_active();
        let target_name = config.name().to_string();
        let mut target_index = None;

        for (i, existing) in self.data.download_config.iter_mut().enumerate() {
            if existing.name() == target_name {
                target_index = Some(i);
            } else if new_is_active && existing.is_active() {
                existing.set_active(false);
            }
        }

        config.encrypt_secrets(self.crypto_provider.as_ref())?;

        if let Some(index) = target_index {
            self.data.download_config[index] = config;
        } else {
            self.data.download_config.push(config);
        }
        Ok(())
    }

    pub fn enable_auto_sub_anime(&mut self) {
        self.data.auto_sub = true;
    }

    pub fn disable_auto_sub_anime(&mut self) {
        self.data.auto_sub = false;
    }

    pub fn verify_password(&self, plain_password: &str) -> Result<bool, Error> {
        let hash = PasswordHash::new(&self.data.password)
            .map_err(|_| Error::invariant("user verify password failed, unknown password"))?;
        if let Ok(()) = Argon2::default().verify_password(plain_password.as_bytes(), &hash) {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn set_password(&mut self, pwd: &str) -> Result<(), Error> {
        let pwd_hash = Self::hash_password(pwd)?;
        self.data.password = pwd_hash;
        Ok(())
    }

    pub(super) fn hash_password(password: &str) -> Result<String, Error> {
        let salt = SaltString::generate(&mut OsRng);
        Ok(Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| Error::external("hash password failed", anyhow!(e)))?
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::cap::{DownloadProvider, DownloaderManager, UserRepository};
    use crate::entity::model::{DownloadConfig, QbitConfig, UserProps};
    use crate::entity::users::Users;
    use crate::infra::crypto::AesCryptoProvider;
    use anyhow::Result;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// 测试专用固定密钥，与 production 使用的 AesCryptoProvider 完全一致。
    const TEST_KEY: &str = "yanami-unit-test-fixed-secret";

    /// 内存版 UserRepository，只实现 cap.rs 里声明的生产契约。
    #[derive(Default)]
    struct MockUserRepository {
        stored: Mutex<Option<UserProps>>,
    }

    impl MockUserRepository {
        fn stored_props(&self) -> Option<UserProps> {
            self.stored
                .lock()
                .expect("test lock should not be poisoned")
                .clone()
        }
    }

    #[async_trait]
    impl UserRepository for MockUserRepository {
        async fn find(&self, id: i64) -> Result<Option<UserProps>> {
            Ok(self.stored_props().filter(|props| props.data.id == id))
        }

        async fn find_by_username(&self, username: &str) -> Result<Option<UserProps>> {
            Ok(self
                .stored_props()
                .filter(|props| props.data.username == username))
        }

        async fn find_by_space_id(&self, space_id: i64) -> Result<Option<UserProps>> {
            Ok(self
                .stored_props()
                .filter(|props| props.data.space_id == space_id))
        }

        async fn insert(
            &self,
            username: &str,
            password: &str,
            role: UserRole,
            auto_sub: bool,
        ) -> Result<UserProps> {
            let props = UserProps {
                data: UserBaseData {
                    id: 1,
                    username: username.to_string(),
                    password: password.to_string(),
                    role,
                    space_id: 100,
                    auto_sub,
                    download_config: Vec::new(),
                },
            };
            *self
                .stored
                .lock()
                .expect("test lock should not be poisoned") = Some(props.clone());
            Ok(props)
        }

        async fn update(&self, user: &UserBaseData) -> Result<()> {
            *self
                .stored
                .lock()
                .expect("test lock should not be poisoned") =
                Some(UserProps { data: user.clone() });
            Ok(())
        }

        async fn list_auto_sub(&self) -> Result<Vec<UserProps>> {
            Ok(self
                .stored_props()
                .filter(|props| props.data.auto_sub)
                .into_iter()
                .collect())
        }

        async fn count_by_role(&self, role: UserRole) -> Result<i64> {
            let hit = self
                .stored_props()
                .is_some_and(|props| props.data.role == role);
            Ok(if hit { 1 } else { 0 })
        }
    }

    /// 这些测试不依赖下载器，调用即视为用例设计错误。
    struct MockDownloaderManager;

    #[async_trait]
    impl DownloaderManager for MockDownloaderManager {
        async fn get(
            &self,
            user_id: i64,
            config: &DownloaderConfig,
        ) -> Result<Arc<dyn DownloadProvider>> {
            anyhow::bail!(
                "test should not fetch downloader via users: user {} config {}",
                user_id,
                config.name()
            )
        }

        async fn validate_config(&self, config: &DownloaderConfig) -> Result<()> {
            anyhow::bail!(
                "test should not validate downloader config: {}",
                config.name()
            )
        }
    }

    fn crypto() -> Arc<AesCryptoProvider> {
        Arc::new(AesCryptoProvider::new(TEST_KEY))
    }

    fn base_data(password: String, download_config: Vec<DownloaderConfig>) -> UserBaseData {
        UserBaseData {
            id: 1,
            username: "alice".to_string(),
            password,
            role: UserRole::User,
            space_id: 9,
            auto_sub: false,
            download_config,
        }
    }

    fn qbit_download_config(name: &str, active: bool, base_path: &str) -> DownloaderConfig {
        DownloaderConfig::Qbit(DownloadConfig {
            name: name.to_string(),
            active,
            base_path: base_path.to_string(),
            config: QbitConfig {
                username: "admin".to_string(),
                password: "qbit-明文密码-123".to_string(),
                url: "http://127.0.0.1:8080/".to_string(),
            },
        })
    }

    fn stored_qbit_password(config: &DownloaderConfig) -> &str {
        match config {
            DownloaderConfig::Qbit(c) => &c.config.password,
            DownloaderConfig::Default(_) => panic!("test expects qbit config"),
        }
    }

    fn active_config_name(entity: &UserEntity) -> Option<&str> {
        entity
            .get_download_config()
            .iter()
            .find(|config| config.is_active())
            .map(|config| config.name())
    }

    #[test]
    fn verify_password_accepts_correct_password_and_rejects_wrong_one() {
        let hash =
            UserEntity::hash_password("correct-password").expect("argon2 hashing should not fail");
        assert_ne!(hash, "correct-password");
        assert!(hash.starts_with("$argon2"), "actual hash: {hash}");

        let entity = UserEntity::new(base_data(hash, Vec::new()), crypto());
        assert!(
            entity
                .verify_password("correct-password")
                .expect("verifying password should not fail")
        );
        assert!(
            !entity
                .verify_password("wrong-password")
                .expect("verifying password should not fail")
        );
    }

    #[test]
    fn verify_password_rejects_unknown_hash_format() {
        let entity = UserEntity::new(
            base_data("not-an-argon2-hash".to_string(), Vec::new()),
            crypto(),
        );

        let err = entity
            .verify_password("whatever")
            .expect_err("invalid hash format should fail");
        assert!(
            err.to_string().contains("unknown password"),
            "actual error: {err}"
        );
    }

    #[tokio::test]
    async fn users_create_stores_argon2_hash_and_entity_can_verify() {
        let repo = Arc::new(MockUserRepository::default());
        let users = Users::new(repo.clone(), Arc::new(MockDownloaderManager), crypto());

        let entity = users
            .create("alice", "p@ssw0rd!", UserRole::Admin, true)
            .await
            .expect("creating user should not fail");

        assert_eq!(entity.id(), 1);
        assert_eq!(entity.username(), "alice");
        assert_eq!(entity.role(), UserRole::Admin);
        assert_eq!(entity.space_id(), 100);
        assert!(entity.auto_sub());
        assert!(
            entity
                .verify_password("p@ssw0rd!")
                .expect("verifying password should not fail")
        );
        assert!(
            !entity
                .verify_password("p@ssw0rd")
                .expect("verifying password should not fail")
        );

        let stored = repo
            .stored_props()
            .expect("mock repository should have stored the user");
        assert_ne!(stored.data.password, "p@ssw0rd!");
        assert!(
            stored.data.password.starts_with("$argon2"),
            "repository must store argon2 hash, not plaintext, actual: {}",
            stored.data.password
        );
    }

    #[tokio::test]
    async fn users_save_persists_changes_through_repository() {
        let repo = Arc::new(MockUserRepository::default());
        let users = Users::new(repo.clone(), Arc::new(MockDownloaderManager), crypto());
        let mut entity = UserEntity::new(base_data("hash".to_string(), Vec::new()), crypto());

        entity.enable_auto_sub_anime();
        users
            .save(&entity)
            .await
            .expect("saving user should not fail");

        let stored = repo
            .stored_props()
            .expect("mock repository should have stored the user");
        assert!(stored.data.auto_sub);
    }

    #[test]
    fn auto_sub_anime_toggles() {
        let mut entity = UserEntity::new(base_data("hash".to_string(), Vec::new()), crypto());
        assert!(!entity.auto_sub());

        entity.enable_auto_sub_anime();
        assert!(entity.auto_sub());

        entity.disable_auto_sub_anime();
        assert!(!entity.auto_sub());
    }

    #[test]
    fn download_config_returns_none_when_no_config_is_active() {
        let entity = UserEntity::new(
            base_data(
                "hash".to_string(),
                vec![qbit_download_config("main", false, "/data")],
            ),
            crypto(),
        );

        assert!(entity.download_config().expect("should not fail").is_none());
    }

    #[test]
    fn download_config_decrypts_the_active_config_secrets() {
        let provider = crypto();
        let plain_password = "qbit-明文密码-123";
        let mut config = qbit_download_config("main", true, "/data");
        config
            .encrypt_secrets(provider.as_ref())
            .expect("encryption should not fail");
        // 落库形态必须是密文
        assert!(!stored_qbit_password(&config).contains(plain_password));

        let entity = UserEntity::new(base_data("hash".to_string(), vec![config]), provider);
        let got = entity
            .download_config()
            .expect("should not fail")
            .expect("should return the active config");

        assert_eq!(got.name(), "main");
        assert_eq!(got.base_path(), "/data");
        assert_eq!(stored_qbit_password(&got), plain_password);
    }

    #[test]
    fn save_download_config_encrypts_secrets_and_replaces_same_name() {
        let mut entity = UserEntity::new(base_data("hash".to_string(), Vec::new()), crypto());

        entity
            .save_download_config(qbit_download_config("main", true, "/data"))
            .expect("saving config should not fail");
        assert_eq!(entity.get_download_config().len(), 1);
        let first_password = stored_qbit_password(&entity.get_download_config()[0]).to_string();
        assert_ne!(first_password, "qbit-明文密码-123");
        assert!(
            !first_password.contains("qbit-明文密码-123"),
            "stored config must be encrypted, actual: {first_password}"
        );

        entity
            .save_download_config(qbit_download_config("main", true, "/data-v2"))
            .expect("overwriting config should not fail");
        assert_eq!(
            entity.get_download_config().len(),
            1,
            "config with the same name should be replaced, not appended"
        );
        let replaced = &entity.get_download_config()[0];
        assert_eq!(replaced.name(), "main");
        assert_eq!(replaced.base_path(), "/data-v2");
    }

    #[test]
    fn save_active_download_config_deactivates_others() {
        let mut entity = UserEntity::new(
            base_data(
                "hash".to_string(),
                vec![qbit_download_config("a", true, "/a")],
            ),
            crypto(),
        );

        entity
            .save_download_config(qbit_download_config("b", true, "/b"))
            .expect("saving should not fail");
        assert_eq!(entity.get_download_config().len(), 2);
        assert_eq!(active_config_name(&entity), Some("b"));
        assert!(
            entity
                .get_download_config()
                .iter()
                .any(|config| config.name() == "a" && !config.is_active())
        );

        // 保存非激活配置不应影响当前激活项
        entity
            .save_download_config(qbit_download_config("c", false, "/c"))
            .expect("saving should not fail");
        assert_eq!(active_config_name(&entity), Some("b"));
        assert!(
            entity
                .get_download_config()
                .iter()
                .any(|config| config.name() == "c" && !config.is_active())
        );
    }

    #[test]
    fn delete_download_config_removes_only_the_named_config() {
        let mut entity = UserEntity::new(
            base_data(
                "hash".to_string(),
                vec![
                    qbit_download_config("a", true, "/a"),
                    qbit_download_config("b", false, "/b"),
                ],
            ),
            crypto(),
        );

        entity.delete_download_config("a");
        let names: Vec<&str> = entity
            .get_download_config()
            .iter()
            .map(|config| config.name())
            .collect();
        assert_eq!(names, vec!["b"]);

        // 删除不存在的配置是幂等的
        entity.delete_download_config("missing");
        assert_eq!(entity.get_download_config().len(), 1);
    }

    #[test]
    fn enable_download_config_activates_only_the_target() {
        let mut entity = UserEntity::new(
            base_data(
                "hash".to_string(),
                vec![
                    qbit_download_config("a", false, "/a"),
                    qbit_download_config("b", true, "/b"),
                ],
            ),
            crypto(),
        );

        entity
            .enable_download_config("a")
            .expect("enabling config should not fail");
        assert_eq!(active_config_name(&entity), Some("a"));
        assert!(
            entity
                .get_download_config()
                .iter()
                .any(|config| config.name() == "b" && !config.is_active())
        );

        let err = entity
            .enable_download_config("missing")
            .expect_err("enabling a missing config should fail");
        assert!(
            err.to_string()
                .contains("not found download config missing"),
            "actual error: {err}"
        );
    }
}

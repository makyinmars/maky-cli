use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};

use crate::{auth::provider::AuthSession, util::ensure_parent_dir_exists};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTokenStoreBackend {
    Keyring,
    FileFallback,
    FileOnly,
}

impl ActiveTokenStoreBackend {
    pub fn label(&self) -> &'static str {
        match self {
            ActiveTokenStoreBackend::Keyring => "keyring",
            ActiveTokenStoreBackend::FileFallback => "file-fallback",
            ActiveTokenStoreBackend::FileOnly => "file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredToken {
    pub provider_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_unix_secs: Option<u64>,
    pub id_token: Option<String>,
    pub account_id: Option<String>,
}

impl StoredToken {
    pub fn into_session(self) -> AuthSession {
        AuthSession {
            provider_id: self.provider_id,
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at_unix_secs: self.expires_at_unix_secs,
            id_token: self.id_token,
            account_id: self.account_id,
        }
    }

    pub fn from_session(session: &AuthSession) -> Self {
        Self {
            provider_id: session.provider_id.clone(),
            access_token: session.access_token.clone(),
            refresh_token: session.refresh_token.clone(),
            expires_at_unix_secs: session.expires_at_unix_secs,
            id_token: session.id_token.clone(),
            account_id: session.account_id.clone(),
        }
    }
}

pub trait TokenStore: Send + Sync {
    fn load(&self, provider_id: &str) -> anyhow::Result<Option<StoredToken>>;

    fn save(&self, token: &StoredToken) -> anyhow::Result<()>;

    fn clear(&self, provider_id: &str) -> anyhow::Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryTokenStore {
    tokens: Mutex<HashMap<String, StoredToken>>,
}

impl TokenStore for InMemoryTokenStore {
    fn load(&self, provider_id: &str) -> anyhow::Result<Option<StoredToken>> {
        let tokens = lock_tokens(&self.tokens)?;
        Ok(tokens.get(provider_id).cloned())
    }

    fn save(&self, token: &StoredToken) -> anyhow::Result<()> {
        let mut tokens = lock_tokens(&self.tokens)?;
        tokens.insert(token.provider_id.clone(), token.clone());
        Ok(())
    }

    fn clear(&self, provider_id: &str) -> anyhow::Result<()> {
        let mut tokens = lock_tokens(&self.tokens)?;
        tokens.remove(provider_id);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTokenStore {
    path: PathBuf,
}

impl FileTokenStore {
    pub fn new<P>(path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn read_tokens(&self) -> anyhow::Result<HashMap<String, StoredToken>> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }

        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read token file {}", self.path.display()))?;

        if raw.trim().is_empty() {
            return Ok(HashMap::new());
        }

        serde_json::from_str::<HashMap<String, StoredToken>>(&raw)
            .with_context(|| format!("failed to parse token file {}", self.path.display()))
    }

    fn write_tokens(&self, tokens: &HashMap<String, StoredToken>) -> anyhow::Result<()> {
        ensure_parent_dir_exists(&self.path)?;
        let payload = serde_json::to_string_pretty(tokens)
            .context("failed to serialize token payload for file store")?;
        fs::write(&self.path, payload)
            .with_context(|| format!("failed to write token file {}", self.path.display()))
    }
}

impl TokenStore for FileTokenStore {
    fn load(&self, provider_id: &str) -> anyhow::Result<Option<StoredToken>> {
        let tokens = self.read_tokens()?;
        Ok(tokens.get(provider_id).cloned())
    }

    fn save(&self, token: &StoredToken) -> anyhow::Result<()> {
        let mut tokens = self.read_tokens()?;
        tokens.insert(token.provider_id.clone(), token.clone());
        self.write_tokens(&tokens)
    }

    fn clear(&self, provider_id: &str) -> anyhow::Result<()> {
        let mut tokens = self.read_tokens()?;
        tokens.remove(provider_id);
        self.write_tokens(&tokens)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyringUnavailableTokenStore {
    reason: String,
}

impl Default for KeyringUnavailableTokenStore {
    fn default() -> Self {
        Self {
            reason: "OS keyring integration is not wired in Phase 3".to_string(),
        }
    }
}

impl KeyringUnavailableTokenStore {
    fn unavailable_error(&self) -> anyhow::Error {
        anyhow!("keyring unavailable: {}", self.reason)
    }
}

impl TokenStore for KeyringUnavailableTokenStore {
    fn load(&self, _provider_id: &str) -> anyhow::Result<Option<StoredToken>> {
        Err(self.unavailable_error())
    }

    fn save(&self, _token: &StoredToken) -> anyhow::Result<()> {
        Err(self.unavailable_error())
    }

    fn clear(&self, _provider_id: &str) -> anyhow::Result<()> {
        Err(self.unavailable_error())
    }
}

#[derive(Debug)]
pub struct KeyringFirstTokenStore {
    keyring: KeyringUnavailableTokenStore,
    file_fallback: FileTokenStore,
    active_backend: ActiveTokenStoreBackend,
    startup_warning: Option<String>,
}

impl KeyringFirstTokenStore {
    pub fn new<P>(fallback_path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        let keyring = KeyringUnavailableTokenStore::default();
        let file_fallback = FileTokenStore::new(fallback_path);

        let (active_backend, startup_warning) = match keyring.load("__maky_keyring_probe__") {
            Ok(_) => (ActiveTokenStoreBackend::Keyring, None),
            Err(error) => (
                ActiveTokenStoreBackend::FileFallback,
                Some(format!(
                    "Keyring token store is unavailable ({error}). Falling back to file token store at {}",
                    file_fallback.path().display()
                )),
            ),
        };

        Self {
            keyring,
            file_fallback,
            active_backend,
            startup_warning,
        }
    }

    pub fn active_backend(&self) -> ActiveTokenStoreBackend {
        self.active_backend
    }

    pub fn startup_warning(&self) -> Option<&str> {
        self.startup_warning.as_deref()
    }
}

impl TokenStore for KeyringFirstTokenStore {
    fn load(&self, provider_id: &str) -> anyhow::Result<Option<StoredToken>> {
        match self.active_backend {
            ActiveTokenStoreBackend::Keyring => self.keyring.load(provider_id),
            ActiveTokenStoreBackend::FileFallback | ActiveTokenStoreBackend::FileOnly => {
                self.file_fallback.load(provider_id)
            }
        }
    }

    fn save(&self, token: &StoredToken) -> anyhow::Result<()> {
        match self.active_backend {
            ActiveTokenStoreBackend::Keyring => self.keyring.save(token),
            ActiveTokenStoreBackend::FileFallback | ActiveTokenStoreBackend::FileOnly => {
                self.file_fallback.save(token)
            }
        }
    }

    fn clear(&self, provider_id: &str) -> anyhow::Result<()> {
        match self.active_backend {
            ActiveTokenStoreBackend::Keyring => self.keyring.clear(provider_id),
            ActiveTokenStoreBackend::FileFallback | ActiveTokenStoreBackend::FileOnly => {
                self.file_fallback.clear(provider_id)
            }
        }
    }
}

pub struct TokenStoreBootstrap {
    pub store: Box<dyn TokenStore>,
    pub backend: ActiveTokenStoreBackend,
    pub warning: Option<String>,
}

pub fn build_token_store(mode: &str, fallback_path: impl Into<PathBuf>) -> TokenStoreBootstrap {
    let fallback_path = fallback_path.into();

    if mode.eq_ignore_ascii_case("file") {
        return TokenStoreBootstrap {
            store: Box::new(FileTokenStore::new(fallback_path)),
            backend: ActiveTokenStoreBackend::FileOnly,
            warning: None,
        };
    }

    let keyring_first = KeyringFirstTokenStore::new(fallback_path);
    let backend = keyring_first.active_backend();
    let warning = keyring_first.startup_warning().map(ToOwned::to_owned);

    TokenStoreBootstrap {
        store: Box::new(keyring_first),
        backend,
        warning,
    }
}

fn lock_tokens(
    mutex: &Mutex<HashMap<String, StoredToken>>,
) -> anyhow::Result<MutexGuard<'_, HashMap<String, StoredToken>>> {
    mutex
        .lock()
        .map_err(|_| anyhow!("token store lock poisoned"))
        .context("failed to lock token store")
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn sample_token(provider_id: &str) -> StoredToken {
        StoredToken {
            provider_id: provider_id.to_string(),
            access_token: "access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at_unix_secs: Some(1_700_000_000),
            id_token: None,
            account_id: None,
        }
    }

    #[test]
    fn in_memory_store_round_trip() {
        let store = InMemoryTokenStore::default();
        let token = sample_token("chatgpt-oauth");

        store.save(&token).expect("save should work");
        let loaded = store
            .load("chatgpt-oauth")
            .expect("load should work")
            .expect("token should exist");

        assert_eq!(loaded, token);
    }

    #[test]
    fn file_store_persists_tokens_across_instances() {
        let dir = tempdir().expect("temp dir should be created");
        let path = dir.path().join("tokens.json");
        let token = sample_token("chatgpt-oauth");

        let writer = FileTokenStore::new(&path);
        writer.save(&token).expect("save should work");

        let reader = FileTokenStore::new(&path);
        let loaded = reader
            .load("chatgpt-oauth")
            .expect("load should work")
            .expect("token should exist");
        assert_eq!(loaded, token);

        reader.clear("chatgpt-oauth").expect("clear should work");
        let empty = FileTokenStore::new(&path)
            .load("chatgpt-oauth")
            .expect("load should work");
        assert!(empty.is_none());
    }

    #[test]
    fn keyring_first_store_uses_file_fallback_and_reports_warning() {
        let dir = tempdir().expect("temp dir should be created");
        let path = dir.path().join("tokens.json");
        let store = KeyringFirstTokenStore::new(&path);

        assert_eq!(
            store.active_backend(),
            ActiveTokenStoreBackend::FileFallback
        );
        assert!(
            store
                .startup_warning()
                .unwrap_or_default()
                .contains("Keyring token store is unavailable"),
            "warning should explain keyring fallback"
        );

        let token = sample_token("chatgpt-oauth");
        store.save(&token).expect("save should use file fallback");
        let loaded = store
            .load("chatgpt-oauth")
            .expect("load should use file fallback")
            .expect("token should exist");
        assert_eq!(loaded, token);
    }

    #[test]
    fn build_token_store_in_file_mode_selects_file_backend() {
        let dir = tempdir().expect("temp dir should be created");
        let path = dir.path().join("tokens.json");
        let bootstrap = build_token_store("file", &path);

        assert_eq!(bootstrap.backend, ActiveTokenStoreBackend::FileOnly);
        assert!(bootstrap.warning.is_none());
    }
}

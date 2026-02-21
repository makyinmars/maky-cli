use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use anyhow::Context;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredToken {
    pub provider_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_unix_secs: Option<u64>,
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

fn lock_tokens(
    mutex: &Mutex<HashMap<String, StoredToken>>,
) -> anyhow::Result<MutexGuard<'_, HashMap<String, StoredToken>>> {
    mutex
        .lock()
        .map_err(|_| anyhow::anyhow!("token store lock poisoned"))
        .context("failed to lock token store")
}

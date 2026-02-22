pub mod oauth_chatgpt;
pub mod provider;
pub mod token_store;

use std::{env, path::PathBuf};

use anyhow::{Context as AnyhowContext, Result};
use tracing::{info, warn};

use crate::{
    storage::config::AppConfig,
    util::{block_on_future, unix_timestamp_secs},
};

use self::{
    oauth_chatgpt::ChatGptOAuthProvider,
    provider::{AuthLoginMethod, AuthProvider, AuthSession, AuthStatus},
    token_store::{
        ActiveTokenStoreBackend, FileTokenStore, StoredToken, TokenStore, build_token_store,
    },
};

const DEFAULT_TOKEN_FILE_PATH: &str = ".maky/auth_tokens.json";
const TOKEN_FILE_PATH_ENV: &str = "MAKY_AUTH_TOKEN_FILE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialSource {
    OAuthSession,
    ApiKey,
    SignedOut,
}

impl CredentialSource {
    pub fn label(&self) -> &'static str {
        match self {
            CredentialSource::OAuthSession => "oauth-session",
            CredentialSource::ApiKey => "api-key",
            CredentialSource::SignedOut => "signed-out",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSnapshot {
    pub status: AuthStatus,
    pub source: CredentialSource,
    pub provider_id: String,
    pub token_store_backend: ActiveTokenStoreBackend,
    pub token_store_warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedCredential {
    OAuth(AuthSession),
    ApiKey(String),
    SignedOut,
}

pub struct AuthRuntime {
    provider: Box<dyn AuthProvider>,
    token_store: Box<dyn TokenStore>,
    token_store_backend: ActiveTokenStoreBackend,
    token_store_warning: Option<String>,
    token_file_path: PathBuf,
    prefer_file_token_bootstrap: bool,
    credential: ResolvedCredential,
    api_key_env: String,
}

impl AuthRuntime {
    pub fn bootstrap_from_env() -> Result<Self> {
        let config = AppConfig::from_env();
        Self::bootstrap_with_config(&config)
    }

    pub fn bootstrap_with_config(config: &AppConfig) -> Result<Self> {
        let token_file_path = PathBuf::from(
            env::var(TOKEN_FILE_PATH_ENV).unwrap_or_else(|_| DEFAULT_TOKEN_FILE_PATH.to_string()),
        );
        let token_store_bootstrap =
            build_token_store(&config.auth.token_store, token_file_path.clone());

        let mut runtime = Self {
            provider: Box::new(ChatGptOAuthProvider),
            token_store: token_store_bootstrap.store,
            token_store_backend: token_store_bootstrap.backend,
            token_store_warning: token_store_bootstrap.warning,
            token_file_path,
            prefer_file_token_bootstrap: true,
            credential: ResolvedCredential::SignedOut,
            api_key_env: config.openai.api_key_env.clone(),
        };

        runtime.resolve_startup_credentials()?;
        Ok(runtime)
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self::from_parts(
            Box::new(MockAuthProvider),
            Box::new(token_store::InMemoryTokenStore::default()),
            ActiveTokenStoreBackend::FileOnly,
            None,
            PathBuf::from(".maky/__maky_test_unused_auth_tokens.json"),
            false,
            "__MAKY_TEST_API_KEY__".to_string(),
        )
    }

    #[cfg(test)]
    pub fn with_store_for_tests(store: Box<dyn TokenStore>) -> Self {
        Self::from_parts(
            Box::new(MockAuthProvider),
            store,
            ActiveTokenStoreBackend::FileOnly,
            None,
            PathBuf::from(".maky/__maky_test_unused_auth_tokens.json"),
            false,
            "__MAKY_TEST_API_KEY__".to_string(),
        )
    }

    #[cfg(test)]
    fn from_parts(
        provider: Box<dyn AuthProvider>,
        token_store: Box<dyn TokenStore>,
        token_store_backend: ActiveTokenStoreBackend,
        token_store_warning: Option<String>,
        token_file_path: PathBuf,
        prefer_file_token_bootstrap: bool,
        api_key_env: String,
    ) -> Self {
        Self {
            provider,
            token_store,
            token_store_backend,
            token_store_warning,
            token_file_path,
            prefer_file_token_bootstrap,
            credential: ResolvedCredential::SignedOut,
            api_key_env,
        }
    }

    pub fn provider_id(&self) -> &'static str {
        self.provider.id()
    }

    pub fn startup_warning(&self) -> Option<&str> {
        self.token_store_warning.as_deref()
    }

    pub fn status(&self) -> AuthStatus {
        match &self.credential {
            ResolvedCredential::OAuth(session) => session.status_at(unix_timestamp_secs()),
            ResolvedCredential::ApiKey(_) => AuthStatus::SignedIn,
            ResolvedCredential::SignedOut => AuthStatus::SignedOut,
        }
    }

    pub fn source(&self) -> CredentialSource {
        match &self.credential {
            ResolvedCredential::OAuth(_) => CredentialSource::OAuthSession,
            ResolvedCredential::ApiKey(_) => CredentialSource::ApiKey,
            ResolvedCredential::SignedOut => CredentialSource::SignedOut,
        }
    }

    pub fn snapshot(&self) -> AuthSnapshot {
        AuthSnapshot {
            status: self.status(),
            source: self.source(),
            provider_id: self.provider.id().to_string(),
            token_store_backend: self.token_store_backend,
            token_store_warning: self.token_store_warning.clone(),
        }
    }

    pub fn status_report(&self) -> String {
        let snapshot = self.snapshot();
        let mut fields = vec![
            format!("status: {}", snapshot.status.label()),
            format!("source: {}", snapshot.source.label()),
            format!("provider: {}", snapshot.provider_id),
            format!("token-store: {}", snapshot.token_store_backend.label()),
        ];

        if let Some(warning) = snapshot.token_store_warning {
            fields.push(format!("warning: {warning}"));
        }

        format!("Auth report | {}", fields.join(" | "))
    }

    pub fn resolve_startup_credentials(&mut self) -> Result<()> {
        let api_key = env::var(&self.api_key_env)
            .ok()
            .and_then(non_empty_token_string);
        self.resolve_startup_credentials_with_api_key(api_key)
    }

    pub fn login(&mut self, method: AuthLoginMethod) -> Result<AuthStatus> {
        let mut session = block_on_future(self.provider.login(method))
            .context("failed to complete OAuth login")?;
        session.provider_id = self.provider.id().to_string();

        self.persist_session(&session)?;
        self.credential = ResolvedCredential::OAuth(session);

        info!(provider_id = self.provider.id(), "oauth login completed");
        Ok(self.status())
    }

    pub fn login_default(&mut self) -> Result<AuthStatus> {
        self.login(AuthLoginMethod::Browser)
    }

    pub fn logout(&mut self) -> Result<AuthStatus> {
        let api_key = env::var(&self.api_key_env)
            .ok()
            .and_then(non_empty_token_string);
        self.logout_with_api_key(api_key)
    }

    /// Refresh-before-request contract:
    /// every OAuth-backed request should call this first so expired tokens are rotated.
    pub fn resolve_access_token_for_request(&mut self) -> Result<Option<String>> {
        let current = std::mem::replace(&mut self.credential, ResolvedCredential::SignedOut);

        match current {
            ResolvedCredential::OAuth(mut session) => {
                self.refresh_session_if_needed(&mut session)?;
                self.persist_session(&session)?;
                let access_token = session.access_token.clone();
                self.credential = ResolvedCredential::OAuth(session);
                Ok(Some(access_token))
            }
            ResolvedCredential::ApiKey(api_key) => {
                let token = api_key.clone();
                self.credential = ResolvedCredential::ApiKey(api_key);
                Ok(Some(token))
            }
            ResolvedCredential::SignedOut => {
                self.credential = ResolvedCredential::SignedOut;
                Ok(None)
            }
        }
    }

    fn resolve_startup_credentials_with_api_key(&mut self, api_key: Option<String>) -> Result<()> {
        let provider_id = self.provider.id();
        let stored_session = self.load_startup_stored_session(provider_id)?;

        if let Some(stored_token) = stored_session {
            let mut session = stored_token.into_session();
            if session.provider_id.is_empty() {
                session.provider_id = provider_id.to_string();
            }

            match self.refresh_session_if_needed(&mut session) {
                Ok(()) => {
                    self.persist_session(&session)?;
                    self.credential = ResolvedCredential::OAuth(session);
                    info!(
                        provider_id,
                        "resolved startup credentials from persisted OAuth session"
                    );
                    return Ok(());
                }
                Err(error) => {
                    warn!(
                        provider_id,
                        ?error,
                        "stored OAuth session could not be refreshed; clearing session and falling back"
                    );
                    self.clear_startup_file_session(provider_id)?;
                    self.token_store.clear(provider_id).with_context(|| {
                        format!("failed to clear stale token for `{provider_id}`")
                    })?;
                }
            }
        }

        if let Some(api_key) = api_key {
            self.credential = ResolvedCredential::ApiKey(api_key);
            info!(
                env_var = %self.api_key_env,
                "resolved startup credentials from API key fallback"
            );
        } else {
            self.credential = ResolvedCredential::SignedOut;
            info!("no startup credentials found; runtime is signed out");
        }

        Ok(())
    }

    fn load_startup_stored_session(&self, provider_id: &str) -> Result<Option<StoredToken>> {
        let mut token_file_preflight_failed = false;

        if self.prefer_file_token_bootstrap {
            let file_store = FileTokenStore::new(self.token_file_path.clone());
            match file_store.load(provider_id) {
                Ok(Some(stored_token)) => {
                    info!(
                        provider_id,
                        token_file = %self.token_file_path.display(),
                        "resolved startup credentials from token file preflight"
                    );
                    return Ok(Some(stored_token));
                }
                Ok(None) => {}
                Err(error) => {
                    token_file_preflight_failed = true;
                    warn!(
                        provider_id,
                        token_file = %self.token_file_path.display(),
                        ?error,
                        "token file preflight failed; falling back to configured token store"
                    );
                }
            }
        }

        if token_file_preflight_failed
            && !matches!(self.token_store_backend, ActiveTokenStoreBackend::Keyring)
        {
            return Ok(None);
        }

        match self.token_store.load(provider_id) {
            Ok(stored) => Ok(stored),
            Err(error) => {
                warn!(
                    provider_id,
                    token_store_backend = self.token_store_backend.label(),
                    ?error,
                    "configured token store load failed during startup; continuing without stored session"
                );
                Ok(None)
            }
        }
    }

    fn clear_startup_file_session(&self, provider_id: &str) -> Result<()> {
        if !self.prefer_file_token_bootstrap {
            return Ok(());
        }

        let file_store = FileTokenStore::new(self.token_file_path.clone());
        file_store.clear(provider_id).with_context(|| {
            format!(
                "failed to clear stale token for provider `{provider_id}` from {}",
                self.token_file_path.display()
            )
        })
    }

    fn logout_with_api_key(&mut self, api_key: Option<String>) -> Result<AuthStatus> {
        let provider_id = self.provider.id();
        self.token_store
            .clear(provider_id)
            .with_context(|| format!("failed to clear token for provider `{provider_id}`"))?;

        self.credential = match api_key.and_then(non_empty_token_string) {
            Some(api_key) => ResolvedCredential::ApiKey(api_key),
            None => ResolvedCredential::SignedOut,
        };

        info!(provider_id, "oauth session was cleared via logout");
        Ok(self.status())
    }

    fn refresh_session_if_needed(&self, session: &mut AuthSession) -> Result<()> {
        block_on_future(self.provider.refresh_if_needed(session)).with_context(|| {
            format!(
                "failed to refresh OAuth session for `{}`",
                self.provider.id()
            )
        })
    }

    fn persist_session(&self, session: &AuthSession) -> Result<()> {
        let stored = StoredToken::from_session(session);
        self.token_store.save(&stored).with_context(|| {
            format!(
                "failed to persist token for provider `{}`",
                self.provider.id()
            )
        })
    }
}

fn non_empty_token_string(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct MockAuthProvider;

#[cfg(test)]
#[async_trait::async_trait]
impl AuthProvider for MockAuthProvider {
    fn id(&self) -> &'static str {
        "chatgpt-oauth"
    }

    async fn login(&self, method: AuthLoginMethod) -> anyhow::Result<AuthSession> {
        let now = unix_timestamp_secs();
        Ok(AuthSession {
            provider_id: self.id().to_string(),
            access_token: format!("test-access-{}-{now}", method.label()),
            refresh_token: Some(format!("test-refresh-{now}")),
            expires_at_unix_secs: Some(now + 3600),
            id_token: None,
            account_id: Some("test-account".to_string()),
        })
    }

    async fn refresh_if_needed(&self, session: &mut AuthSession) -> anyhow::Result<()> {
        if !session.is_expired(unix_timestamp_secs()) {
            return Ok(());
        }

        let refresh = session.refresh_token.clone().ok_or_else(|| {
            provider::AuthDomainError::MissingRefreshToken {
                provider_id: session.provider_id.clone(),
            }
        })?;

        session.access_token = format!("{refresh}-refreshed");
        session.expires_at_unix_secs = Some(unix_timestamp_secs() + 3600);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use super::*;
    use crate::auth::token_store::{FileTokenStore, InMemoryTokenStore, StoredToken, TokenStore};

    fn oauth_token(provider_id: &str, expires_at_unix_secs: Option<u64>) -> StoredToken {
        StoredToken {
            provider_id: provider_id.to_string(),
            access_token: "stored-access".to_string(),
            refresh_token: Some("stored-refresh".to_string()),
            expires_at_unix_secs,
            id_token: None,
            account_id: None,
        }
    }

    fn runtime_with_store_and_file_bootstrap(
        store: Box<dyn TokenStore>,
        token_file_path: PathBuf,
    ) -> AuthRuntime {
        AuthRuntime::from_parts(
            Box::new(MockAuthProvider),
            store,
            ActiveTokenStoreBackend::Keyring,
            None,
            token_file_path,
            true,
            "__MAKY_TEST_API_KEY__".to_string(),
        )
    }

    #[test]
    fn startup_prefers_oauth_session_over_api_key() {
        let store = InMemoryTokenStore::default();
        store
            .save(&oauth_token("chatgpt-oauth", Some(u64::MAX)))
            .expect("store save should work");

        let mut runtime = AuthRuntime::with_store_for_tests(Box::new(store));
        runtime
            .resolve_startup_credentials_with_api_key(Some("sk-test".to_string()))
            .expect("startup resolution should work");

        assert_eq!(runtime.source(), CredentialSource::OAuthSession);
    }

    #[test]
    fn startup_checks_token_file_first() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let token_file_path = temp_dir.path().join("auth_tokens.json");
        let file_store = FileTokenStore::new(token_file_path.clone());
        file_store
            .save(&oauth_token("chatgpt-oauth", Some(u64::MAX)))
            .expect("file token should be written");

        let mut runtime = runtime_with_store_and_file_bootstrap(
            Box::new(InMemoryTokenStore::default()),
            token_file_path,
        );

        runtime
            .resolve_startup_credentials_with_api_key(None)
            .expect("startup resolution should work");

        assert_eq!(runtime.source(), CredentialSource::OAuthSession);
    }

    #[test]
    fn startup_falls_back_to_configured_store_when_token_file_is_invalid() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let token_file_path = temp_dir.path().join("auth_tokens.json");
        fs::write(&token_file_path, "{invalid-json")
            .expect("invalid token file should be written for test");

        let store = InMemoryTokenStore::default();
        store
            .save(&oauth_token("chatgpt-oauth", Some(u64::MAX)))
            .expect("store save should work");

        let mut runtime = runtime_with_store_and_file_bootstrap(Box::new(store), token_file_path);
        runtime
            .resolve_startup_credentials_with_api_key(None)
            .expect("startup resolution should work");

        assert_eq!(runtime.source(), CredentialSource::OAuthSession);
    }

    #[test]
    fn startup_falls_back_to_api_key_when_oauth_missing() {
        let store = InMemoryTokenStore::default();
        let mut runtime = AuthRuntime::with_store_for_tests(Box::new(store));

        runtime
            .resolve_startup_credentials_with_api_key(Some("sk-test".to_string()))
            .expect("startup resolution should work");

        assert_eq!(runtime.source(), CredentialSource::ApiKey);
        assert_eq!(runtime.status(), AuthStatus::SignedIn);
    }

    #[test]
    fn startup_with_invalid_token_file_can_still_use_api_key_fallback() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let token_file_path = temp_dir.path().join("auth_tokens.json");
        fs::write(&token_file_path, "{invalid-json")
            .expect("invalid token file should be written for test");

        let mut runtime = runtime_with_store_and_file_bootstrap(
            Box::new(FileTokenStore::new(token_file_path.clone())),
            token_file_path,
        );

        runtime
            .resolve_startup_credentials_with_api_key(Some("sk-test".to_string()))
            .expect("startup resolution should work");

        assert_eq!(runtime.source(), CredentialSource::ApiKey);
        assert_eq!(runtime.status(), AuthStatus::SignedIn);
    }

    #[test]
    fn refresh_happens_before_request_when_session_is_expired() {
        let store = InMemoryTokenStore::default();
        store
            .save(&oauth_token("chatgpt-oauth", Some(0)))
            .expect("store save should work");

        let mut runtime = AuthRuntime::with_store_for_tests(Box::new(store));
        runtime
            .resolve_startup_credentials_with_api_key(None)
            .expect("startup resolution should work");

        let access_token = runtime
            .resolve_access_token_for_request()
            .expect("access token resolution should work")
            .expect("oauth token should be present");

        assert!(
            access_token.contains("refreshed"),
            "expired token should be refreshed before request"
        );
    }

    #[test]
    fn logout_clears_oauth_session() {
        let mut runtime = AuthRuntime::for_tests();
        runtime
            .login(AuthLoginMethod::Browser)
            .expect("login should succeed");

        let status = runtime
            .logout_with_api_key(None)
            .expect("logout should succeed");

        assert_eq!(status, AuthStatus::SignedOut);
        assert_eq!(runtime.source(), CredentialSource::SignedOut);
    }
}

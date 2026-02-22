use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthLoginMethod {
    Browser,
    Headless,
}

impl AuthLoginMethod {
    pub fn label(&self) -> &'static str {
        match self {
            AuthLoginMethod::Browser => "browser",
            AuthLoginMethod::Headless => "headless",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStatus {
    SignedOut,
    SignedIn,
    Expired,
    Refreshing,
}

impl AuthStatus {
    pub fn label(&self) -> &'static str {
        match self {
            AuthStatus::SignedOut => "signed-out",
            AuthStatus::SignedIn => "signed-in",
            AuthStatus::Expired => "expired",
            AuthStatus::Refreshing => "refreshing",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthSession {
    pub provider_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_unix_secs: Option<u64>,
    pub id_token: Option<String>,
    pub account_id: Option<String>,
}

impl AuthSession {
    pub fn is_expired(&self, now_unix_secs: u64) -> bool {
        self.expires_at_unix_secs
            .map(|expires_at| now_unix_secs >= expires_at)
            .unwrap_or(false)
    }

    pub fn status_at(&self, now_unix_secs: u64) -> AuthStatus {
        if self.is_expired(now_unix_secs) {
            AuthStatus::Expired
        } else {
            AuthStatus::SignedIn
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuthDomainError {
    #[error("oauth session for provider `{provider_id}` is expired and has no refresh token")]
    MissingRefreshToken { provider_id: String },
    #[error("oauth session for provider `{provider_id}` is invalid")]
    InvalidSession { provider_id: String },
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    fn id(&self) -> &'static str;

    async fn login(&self, method: AuthLoginMethod) -> anyhow::Result<AuthSession>;

    /// Refresh contract:
    /// call this immediately before model requests that rely on OAuth sessions.
    /// Implementations should no-op when the session is still valid.
    async fn refresh_if_needed(&self, session: &mut AuthSession) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_status_is_signed_in_when_not_expired() {
        let session = AuthSession {
            provider_id: "chatgpt-oauth".to_string(),
            access_token: "token".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at_unix_secs: Some(1_000),
            id_token: None,
            account_id: None,
        };

        assert_eq!(session.status_at(999), AuthStatus::SignedIn);
    }

    #[test]
    fn session_status_is_expired_at_or_after_expiry() {
        let session = AuthSession {
            provider_id: "chatgpt-oauth".to_string(),
            access_token: "token".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at_unix_secs: Some(1_000),
            id_token: None,
            account_id: None,
        };

        assert_eq!(session.status_at(1_000), AuthStatus::Expired);
        assert_eq!(session.status_at(1_001), AuthStatus::Expired);
    }
}

use async_trait::async_trait;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_unix_secs: Option<u64>,
}

impl AuthSession {
    pub fn is_expired(&self, now_unix_secs: u64) -> bool {
        self.expires_at_unix_secs
            .map(|expires_at| now_unix_secs >= expires_at)
            .unwrap_or(false)
    }
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    fn id(&self) -> &'static str;

    async fn login(&self) -> anyhow::Result<AuthSession>;

    async fn refresh_if_needed(&self, session: &mut AuthSession) -> anyhow::Result<()>;
}

use anyhow::bail;
use async_trait::async_trait;

use crate::auth::provider::{AuthProvider, AuthSession};

#[derive(Debug, Default)]
pub struct ChatGptOAuthProvider;

#[async_trait]
impl AuthProvider for ChatGptOAuthProvider {
    fn id(&self) -> &'static str {
        "chatgpt-oauth"
    }

    async fn login(&self) -> anyhow::Result<AuthSession> {
        bail!("OAuth login is not implemented in Phase 2")
    }

    async fn refresh_if_needed(&self, _session: &mut AuthSession) -> anyhow::Result<()> {
        Ok(())
    }
}

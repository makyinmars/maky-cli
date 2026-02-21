use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthConfig {
    pub mode: String,
    pub token_store: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub provider: String,
    pub model: String,
    pub auth: AuthConfig,
    pub openai: OpenAiConfig,
    pub session_db_path: String,
    pub approval_policy: String,
    pub ui_alt_screen: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            model: "gpt-4.1-mini".to_string(),
            auth: AuthConfig {
                mode: "oauth".to_string(),
                token_store: "keyring".to_string(),
            },
            openai: OpenAiConfig {
                base_url: "https://api.openai.com/v1".to_string(),
                api_key_env: "OPENAI_API_KEY".to_string(),
            },
            session_db_path: ".maky/sessions.db".to_string(),
            approval_policy: "on-request".to_string(),
            ui_alt_screen: true,
        }
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(value) = env::var("MAKY_PROVIDER") {
            config.provider = value;
        }
        if let Ok(value) = env::var("MAKY_MODEL") {
            config.model = value;
        }
        if let Ok(value) = env::var("MAKY_AUTH_MODE") {
            config.auth.mode = value;
        }
        if let Ok(value) = env::var("MAKY_SESSION_DB_PATH") {
            config.session_db_path = value;
        }
        if let Ok(value) = env::var("MAKY_APPROVAL_POLICY") {
            config.approval_policy = value;
        }

        config
    }
}

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
    pub variant: String,
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
            model: "openai/gpt-5.3-codex".to_string(),
            auth: AuthConfig {
                mode: "oauth".to_string(),
                token_store: "keyring".to_string(),
            },
            openai: OpenAiConfig {
                base_url: "https://api.openai.com/v1".to_string(),
                api_key_env: "OPENAI_API_KEY".to_string(),
                variant: "medium".to_string(),
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
        if let Ok(value) = env::var("MAKY_AUTH_TOKEN_STORE") {
            config.auth.token_store = value;
        }
        if let Ok(value) = env::var("MAKY_OPENAI_API_KEY_ENV") {
            config.openai.api_key_env = value;
        }
        if let Ok(value) = env::var("MAKY_OPENAI_VARIANT")
            && let Some(variant) = normalize_openai_variant(&value)
        {
            config.openai.variant = variant;
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

fn normalize_openai_variant(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "low" | "medium" | "high" | "xhigh" => Some(normalized),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_openai_model_and_variant_are_set() {
        let config = AppConfig::default();

        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "openai/gpt-5.3-codex");
        assert_eq!(config.openai.variant, "medium");
    }

    #[test]
    fn normalizes_supported_openai_variants() {
        assert_eq!(normalize_openai_variant("low"), Some("low".to_string()));
        assert_eq!(
            normalize_openai_variant("Medium"),
            Some("medium".to_string())
        );
        assert_eq!(normalize_openai_variant("HIGH"), Some("high".to_string()));
        assert_eq!(
            normalize_openai_variant(" xhigh "),
            Some("xhigh".to_string())
        );
    }

    #[test]
    fn rejects_unsupported_openai_variants() {
        assert_eq!(normalize_openai_variant(""), None);
        assert_eq!(normalize_openai_variant("ultra"), None);
        assert_eq!(normalize_openai_variant("x-high"), None);
    }
}

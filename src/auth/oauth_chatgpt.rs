use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    net::TcpListener,
    sync::OnceLock,
    time::{Duration, Instant},
};

use anyhow::{Context, bail, ensure};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::info;
use url::Url;

use crate::{
    auth::provider::{AuthDomainError, AuthLoginMethod, AuthProvider, AuthSession},
    util::unix_timestamp_secs,
};

const DEFAULT_SESSION_LIFETIME_SECS: u64 = 60 * 60;
const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_ISSUER: &str = "https://auth.openai.com";
const DEFAULT_CALLBACK_PORT: u16 = 1455;
const OAUTH_CALLBACK_PATH: &str = "/auth/callback";
const OAUTH_CALLBACK_TIMEOUT_SECS: u64 = 5 * 60;
const DEVICE_LOGIN_TIMEOUT_SECS: u64 = 10 * 60;
const DEVICE_POLLING_SAFETY_MARGIN_MS: u64 = 3_000;
const USER_AGENT: &str = "maky-cli/0.1.0";

#[derive(Debug, Default)]
pub struct ChatGptOAuthProvider;

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct DeviceUserCodeResponse {
    device_auth_id: String,
    user_code: String,
    interval: String,
}

#[derive(Debug, Deserialize)]
struct DeviceAuthTokenResponse {
    authorization_code: String,
    code_verifier: String,
}

#[derive(Debug, Deserialize)]
struct JwtClaims {
    chatgpt_account_id: Option<String>,
    organizations: Option<Vec<JwtOrganization>>,
    #[serde(rename = "https://api.openai.com/auth")]
    openai_auth: Option<OpenAiAuthClaims>,
}

#[derive(Debug, Deserialize)]
struct JwtOrganization {
    id: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiAuthClaims {
    chatgpt_account_id: Option<String>,
}

#[async_trait]
impl AuthProvider for ChatGptOAuthProvider {
    fn id(&self) -> &'static str {
        "chatgpt-oauth"
    }

    async fn login(&self, method: AuthLoginMethod) -> anyhow::Result<AuthSession> {
        let tokens = match method {
            AuthLoginMethod::Browser => self.login_with_browser()?,
            AuthLoginMethod::Headless => self.login_with_device_code()?,
        };

        let refresh_token = tokens
            .refresh_token
            .clone()
            .filter(|value| !value.trim().is_empty())
            .context("oauth login did not return a refresh token")?;

        let now_unix_secs = unix_timestamp_secs();
        Ok(AuthSession {
            provider_id: self.id().to_string(),
            access_token: tokens.access_token.clone(),
            refresh_token: Some(refresh_token),
            expires_at_unix_secs: Some(
                now_unix_secs + tokens.expires_in.unwrap_or(DEFAULT_SESSION_LIFETIME_SECS),
            ),
            id_token: tokens.id_token.clone(),
            account_id: extract_account_id(&tokens),
        })
    }

    async fn refresh_if_needed(&self, session: &mut AuthSession) -> anyhow::Result<()> {
        ensure!(
            session.provider_id == self.id(),
            "provider mismatch during refresh: session belongs to `{}`, refresher is `{}`",
            session.provider_id,
            self.id()
        );

        let now_unix_secs = unix_timestamp_secs();
        if !session.is_expired(now_unix_secs) {
            return Ok(());
        }

        let refresh_token =
            session
                .refresh_token
                .clone()
                .ok_or_else(|| AuthDomainError::MissingRefreshToken {
                    provider_id: session.provider_id.clone(),
                })?;

        if refresh_token.trim().is_empty() {
            return Err(AuthDomainError::InvalidSession {
                provider_id: session.provider_id.clone(),
            })
            .context("refresh token cannot be empty");
        }

        let tokens = refresh_access_token(&refresh_token)?;
        let account_id = extract_account_id(&tokens);
        session.access_token = tokens.access_token;
        if let Some(next_refresh_token) = tokens.refresh_token
            && !next_refresh_token.trim().is_empty()
        {
            session.refresh_token = Some(next_refresh_token);
        }
        session.id_token = tokens.id_token.or_else(|| session.id_token.take());
        session.account_id = account_id.or_else(|| session.account_id.take());
        session.expires_at_unix_secs =
            Some(now_unix_secs + tokens.expires_in.unwrap_or(DEFAULT_SESSION_LIFETIME_SECS));
        Ok(())
    }
}

impl ChatGptOAuthProvider {
    fn login_with_browser(&self) -> anyhow::Result<TokenResponse> {
        let callback_port = oauth_callback_port();
        let redirect_uri = format!("http://localhost:{callback_port}{OAUTH_CALLBACK_PATH}");
        let (verifier, challenge) = generate_pkce_pair();
        let state = generate_state_token();
        let authorize_url = build_authorize_url(&redirect_uri, &challenge, &state);

        info!(
            method = "browser",
            "starting chatgpt plus/pro oauth login; opening browser"
        );
        webbrowser::open(&authorize_url).with_context(|| {
            format!(
                "failed to open browser for oauth login. open this URL manually: {authorize_url}"
            )
        })?;

        let authorization_code = wait_for_oauth_callback(
            callback_port,
            &state,
            Duration::from_secs(OAUTH_CALLBACK_TIMEOUT_SECS),
        )?;
        exchange_authorization_code(&authorization_code, &redirect_uri, &verifier)
    }

    fn login_with_device_code(&self) -> anyhow::Result<TokenResponse> {
        let client = http_client()?;
        let issuer = oauth_issuer();
        let client_id = oauth_client_id();
        let interval_and_code = request_device_code(client, &issuer, &client_id)?;
        let interval_millis = interval_and_code.interval;
        let device_auth_id = interval_and_code.device_auth_id;
        let user_code = interval_and_code.user_code;

        // This path is useful in non-browser environments and mirrors the flow in opencode.
        eprintln!("Open {issuer}/codex/device and enter code: {user_code}");

        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(DEVICE_LOGIN_TIMEOUT_SECS) {
                bail!("headless oauth login timed out after waiting for device authorization");
            }

            let poll_url = format!("{issuer}/api/accounts/deviceauth/token");
            let response = client
                .post(poll_url)
                .header("Content-Type", "application/json")
                .header("User-Agent", USER_AGENT)
                .json(&serde_json::json!({
                    "device_auth_id": device_auth_id,
                    "user_code": user_code,
                }))
                .send()
                .context("failed to poll device authorization status")?;

            if response.status().is_success() {
                let payload: DeviceAuthTokenResponse = response
                    .json()
                    .context("failed to parse device authorization response payload")?;
                return exchange_authorization_code(
                    &payload.authorization_code,
                    &format!("{issuer}/deviceauth/callback"),
                    &payload.code_verifier,
                );
            }

            let status = response.status().as_u16();
            if status != 403 && status != 404 {
                let body = response.text().unwrap_or_default();
                bail!("device authorization failed with status {status}: {body}");
            }

            std::thread::sleep(Duration::from_millis(
                interval_millis + DEVICE_POLLING_SAFETY_MARGIN_MS,
            ));
        }
    }
}

fn request_device_code(
    client: &Client,
    issuer: &str,
    client_id: &str,
) -> anyhow::Result<DevicePollingBootstrap> {
    let response = client
        .post(format!("{issuer}/api/accounts/deviceauth/usercode"))
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
        .json(&serde_json::json!({ "client_id": client_id }))
        .send()
        .context("failed to request device authorization code")?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().unwrap_or_default();
        bail!("device authorization start failed with status {status}: {body}");
    }

    let body: DeviceUserCodeResponse = response
        .json()
        .context("failed to parse device authorization code payload")?;
    let interval_seconds = body.interval.parse::<u64>().unwrap_or(5).max(1);

    Ok(DevicePollingBootstrap {
        device_auth_id: body.device_auth_id,
        user_code: body.user_code,
        interval: interval_seconds * 1000,
    })
}

struct DevicePollingBootstrap {
    device_auth_id: String,
    user_code: String,
    interval: u64,
}

fn exchange_authorization_code(
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> anyhow::Result<TokenResponse> {
    let client = http_client()?;
    let issuer = oauth_issuer();
    let client_id = oauth_client_id();

    let response = client
        .post(format!("{issuer}/oauth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id.as_str()),
            ("code_verifier", code_verifier),
        ])
        .send()
        .context("failed to exchange authorization code for oauth tokens")?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().unwrap_or_default();
        bail!("token exchange failed with status {status}: {body}");
    }

    response
        .json()
        .context("failed to parse oauth token exchange payload")
}

fn refresh_access_token(refresh_token: &str) -> anyhow::Result<TokenResponse> {
    let client = http_client()?;
    let issuer = oauth_issuer();
    let client_id = oauth_client_id();

    let response = client
        .post(format!("{issuer}/oauth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id.as_str()),
        ])
        .send()
        .context("failed to refresh oauth access token")?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().unwrap_or_default();
        bail!("token refresh failed with status {status}: {body}");
    }

    response
        .json()
        .context("failed to parse oauth token refresh payload")
}

fn wait_for_oauth_callback(
    port: u16,
    expected_state: &str,
    timeout: Duration,
) -> anyhow::Result<String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("failed to bind oauth callback server on port {port}"))?;
    listener
        .set_nonblocking(true)
        .context("failed to set oauth callback listener to nonblocking mode")?;

    let start = Instant::now();
    loop {
        if start.elapsed() >= timeout {
            bail!(
                "oauth callback timed out after {} seconds",
                timeout.as_secs()
            );
        }

        match listener.accept() {
            Ok((mut stream, _address)) => {
                if let Some(code) = handle_callback_request(&mut stream, expected_state)? {
                    return Ok(code);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                return Err(error).context("failed while accepting oauth callback request");
            }
        }
    }
}

fn handle_callback_request(
    stream: &mut std::net::TcpStream,
    expected_state: &str,
) -> anyhow::Result<Option<String>> {
    let mut buffer = [0_u8; 8192];
    let bytes_read = stream
        .read(&mut buffer)
        .context("failed to read callback request")?;
    if bytes_read == 0 {
        return Ok(None);
    }

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let first_line = request.lines().next().unwrap_or_default();
    let mut first_line_parts = first_line.split_whitespace();
    let method = first_line_parts.next().unwrap_or_default();
    let target = first_line_parts.next().unwrap_or("/");

    if method != "GET" {
        write_http_response(
            stream,
            405,
            "Method Not Allowed",
            "Unsupported request method",
        )?;
        return Ok(None);
    }

    let parsed_url = Url::parse(&format!("http://localhost{target}"))
        .context("failed to parse callback request URL")?;

    if parsed_url.path() != OAUTH_CALLBACK_PATH {
        write_http_response(
            stream,
            404,
            "Not Found",
            "OAuth callback endpoint not found",
        )?;
        return Ok(None);
    }

    let query: HashMap<String, String> = parsed_url.query_pairs().into_owned().collect();
    if let Some(error) = query.get("error") {
        let description = query
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| error.clone());
        write_http_response(
            stream,
            400,
            "OAuth Failed",
            &format!("OAuth failed: {description}"),
        )?;
        bail!("oauth callback returned error: {description}");
    }

    let Some(state) = query.get("state") else {
        write_http_response(stream, 400, "OAuth Failed", "Missing callback state value")?;
        bail!("oauth callback missing state parameter");
    };
    if state != expected_state {
        write_http_response(
            stream,
            400,
            "OAuth Failed",
            "Invalid callback state. Please retry login.",
        )?;
        bail!("oauth callback state mismatch");
    }

    let Some(code) = query.get("code").cloned() else {
        write_http_response(
            stream,
            400,
            "OAuth Failed",
            "Missing authorization code from callback",
        )?;
        bail!("oauth callback missing authorization code");
    };

    write_http_response(
        stream,
        200,
        "Authorization Successful",
        "Authorization completed. You can close this tab and return to maky-cli.",
    )?;
    Ok(Some(code))
}

fn write_http_response(
    stream: &mut std::net::TcpStream,
    status_code: u16,
    title: &str,
    body: &str,
) -> anyhow::Result<()> {
    let html = format!(
        "<!doctype html><html><head><title>{title}</title></head><body><h1>{title}</h1><p>{body}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status_code}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );

    stream
        .write_all(response.as_bytes())
        .context("failed to write callback response")?;
    stream.flush().context("failed to flush callback response")
}

fn build_authorize_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    let issuer = oauth_issuer();
    let client_id = oauth_client_id();

    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", "openid profile email offline_access")
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", state)
        .append_pair("originator", "maky-cli")
        .finish();

    format!("{issuer}/oauth/authorize?{query}")
}

fn generate_pkce_pair() -> (String, String) {
    let verifier = random_urlsafe_token();
    let challenge_hash = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(challenge_hash);
    (verifier, challenge)
}

fn generate_state_token() -> String {
    random_urlsafe_token()
}

fn random_urlsafe_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn parse_jwt_claims(token: &str) -> Option<JwtClaims> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<JwtClaims>(&decoded).ok()
}

fn extract_account_id(tokens: &TokenResponse) -> Option<String> {
    for token in [
        tokens.id_token.as_deref(),
        Some(tokens.access_token.as_str()),
    ] {
        let Some(token) = token else { continue };
        let Some(claims) = parse_jwt_claims(token) else {
            continue;
        };

        if let Some(account_id) = claims.chatgpt_account_id {
            return Some(account_id);
        }

        if let Some(auth_claims) = claims.openai_auth
            && let Some(account_id) = auth_claims.chatgpt_account_id
        {
            return Some(account_id);
        }

        if let Some(organizations) = claims.organizations
            && let Some(first) = organizations.first()
        {
            return Some(first.id.clone());
        }
    }

    None
}

fn http_client() -> anyhow::Result<&'static Client> {
    static CLIENT: OnceLock<Client> = OnceLock::new();

    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to initialize oauth http client")?;

    let _ = CLIENT.set(client);
    CLIENT
        .get()
        .context("failed to access initialized oauth http client")
}

fn oauth_issuer() -> String {
    env::var("MAKY_OPENAI_OAUTH_ISSUER").unwrap_or_else(|_| DEFAULT_ISSUER.to_string())
}

fn oauth_client_id() -> String {
    env::var("MAKY_OPENAI_OAUTH_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string())
}

fn oauth_callback_port() -> u16 {
    env::var("MAKY_OPENAI_OAUTH_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port > 0)
        .unwrap_or(DEFAULT_CALLBACK_PORT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt_from_payload(payload: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let body =
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).expect("payload should serialize"));
        format!("{header}.{body}.signature")
    }

    #[test]
    fn parse_jwt_claims_extracts_root_account_id() {
        let token = jwt_from_payload(serde_json::json!({
            "chatgpt_account_id": "acc-123",
        }));
        let claims = parse_jwt_claims(&token).expect("claims should parse");
        assert_eq!(claims.chatgpt_account_id.as_deref(), Some("acc-123"));
    }

    #[test]
    fn extract_account_id_reads_nested_openai_claim() {
        let token = jwt_from_payload(serde_json::json!({
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acc-nested"
            }
        }));
        let tokens = TokenResponse {
            access_token: token,
            refresh_token: Some("refresh".to_string()),
            id_token: None,
            expires_in: Some(3600),
        };

        assert_eq!(extract_account_id(&tokens).as_deref(), Some("acc-nested"));
    }

    #[test]
    fn authorize_url_contains_expected_parameters() {
        let url = build_authorize_url(
            "http://localhost:1455/auth/callback",
            "challenge",
            "state123",
        );
        let parsed = Url::parse(&url).expect("url should parse");
        let query: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

        assert_eq!(parsed.path(), "/oauth/authorize");
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some("openid profile email offline_access")
        );
        assert_eq!(
            query.get("code_challenge").map(String::as_str),
            Some("challenge")
        );
        assert_eq!(query.get("state").map(String::as_str), Some("state123"));
    }

    #[test]
    fn http_client_is_singleton() {
        let first = http_client().expect("client builds cleanly");
        let second = http_client().expect("second call reuses client");
        assert!(
            std::ptr::eq(first, second),
            "multiple calls should return the same client handle"
        );
    }

    #[tokio::test]
    async fn refresh_fails_when_refresh_token_missing() {
        let provider = ChatGptOAuthProvider;
        let mut session = AuthSession {
            provider_id: provider.id().to_string(),
            access_token: "old-access".to_string(),
            refresh_token: None,
            expires_at_unix_secs: Some(0),
            id_token: None,
            account_id: None,
        };

        let error = provider
            .refresh_if_needed(&mut session)
            .await
            .expect_err("missing refresh token should fail");

        assert!(error.to_string().contains("has no refresh token"));
    }

    #[tokio::test]
    async fn refresh_noops_when_session_is_not_expired() {
        let provider = ChatGptOAuthProvider;
        let mut session = AuthSession {
            provider_id: provider.id().to_string(),
            access_token: "valid-access".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at_unix_secs: Some(u64::MAX),
            id_token: None,
            account_id: None,
        };

        provider
            .refresh_if_needed(&mut session)
            .await
            .expect("refresh should noop");

        assert_eq!(session.access_token, "valid-access");
    }
}

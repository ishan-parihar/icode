use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::config::McpOAuthConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthMetadata {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
    pub scopes_supported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientRegistration {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerCredentials {
    pub server_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}

#[derive(Debug)]
pub enum McpOAuthError {
    DiscoveryFailed(String),
    RegistrationFailed(String),
    AuthorizationFailed(String),
    TokenExchangeFailed(String),
    Timeout,
    Io(io::Error),
}

impl Display for McpOAuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DiscoveryFailed(msg) => write!(f, "OAuth discovery failed: {msg}"),
            Self::RegistrationFailed(msg) => write!(f, "OAuth registration failed: {msg}"),
            Self::AuthorizationFailed(msg) => write!(f, "OAuth authorization failed: {msg}"),
            Self::TokenExchangeFailed(msg) => write!(f, "OAuth token exchange failed: {msg}"),
            Self::Timeout => write!(f, "OAuth operation timed out"),
            Self::Io(error) => write!(f, "IO error: {error}"),
        }
    }
}

impl std::error::Error for McpOAuthError {}

impl From<io::Error> for McpOAuthError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoredCredentialsMap {
    servers: BTreeMap<String, McpServerCredentials>,
}

pub struct McpOAuthStore {
    path: PathBuf,
}

impl McpOAuthStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    #[must_use]
    pub fn default_path() -> PathBuf {
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
        home.join(".icode").join("mcp_oauth.json")
    }

    #[must_use]
    pub fn default_store() -> Self {
        Self::new(Self::default_path())
    }

    fn read_all(&self) -> io::Result<StoredCredentialsMap> {
        match std::fs::read_to_string(&self.path) {
            Ok(contents) => {
                if contents.trim().is_empty() {
                    return Ok(StoredCredentialsMap::default());
                }
                serde_json::from_str(&contents)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Ok(StoredCredentialsMap::default())
            }
            Err(error) => Err(error),
        }
    }

    fn write_all(&self, map: &StoredCredentialsMap) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let rendered = serde_json::to_string_pretty(&map)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let temp_path = self.path.with_extension("json.tmp");
        std::fs::write(&temp_path, format!("{rendered}\n"))?;
        std::fs::rename(temp_path, &self.path)
    }

    pub fn save_credentials(&self, credentials: &McpServerCredentials) -> io::Result<()> {
        let mut map = self.read_all()?;
        map.servers
            .insert(credentials.server_url.clone(), credentials.clone());
        self.write_all(&map)
    }

    pub fn load_credentials(&self, server_url: &str) -> io::Result<Option<McpServerCredentials>> {
        let map = self.read_all()?;
        Ok(map.servers.get(server_url).cloned())
    }

    pub fn remove_credentials(&self, server_url: &str) -> io::Result<()> {
        let mut map = self.read_all()?;
        map.servers.remove(server_url);
        self.write_all(&map)
    }
}

pub struct McpOAuthFlow {
    store: McpOAuthStore,
    config: McpOAuthConfig,
    server_url: String,
}

impl McpOAuthFlow {
    #[must_use]
    pub fn new(store: McpOAuthStore, config: McpOAuthConfig, server_url: String) -> Self {
        Self {
            store,
            config,
            server_url,
        }
    }

    pub async fn discover_metadata(&self) -> Result<OAuthMetadata, McpOAuthError> {
        let metadata_url = self
            .config
            .auth_server_metadata_url
            .clone()
            .unwrap_or_else(|| {
                format!("{}/.well-known/oauth-authorization-server", self.server_url)
            });

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("failed to build HTTP client");
        let response = client
            .get(&metadata_url)
            .send()
            .await
            .map_err(|error| McpOAuthError::DiscoveryFailed(error.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| McpOAuthError::DiscoveryFailed(error.to_string()))?;

        if !status.is_success() {
            return Err(McpOAuthError::DiscoveryFailed(format!(
                "HTTP {status}: {body}"
            )));
        }

        let raw: serde_json::Value = serde_json::from_str(&body)
            .map_err(|error| McpOAuthError::DiscoveryFailed(format!("invalid JSON: {error}")))?;

        let authorization_endpoint = raw
            .get("authorization_endpoint")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpOAuthError::DiscoveryFailed(
                    "missing authorization_endpoint in metadata".to_string(),
                )
            })?
            .to_string();

        let token_endpoint = raw
            .get("token_endpoint")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpOAuthError::DiscoveryFailed("missing token_endpoint in metadata".to_string())
            })?
            .to_string();

        let registration_endpoint = raw
            .get("registration_endpoint")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let scopes_supported = raw
            .get("scopes_supported")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        Ok(OAuthMetadata {
            authorization_endpoint,
            token_endpoint,
            registration_endpoint,
            scopes_supported,
        })
    }

    pub async fn register_client(
        &self,
        metadata: &OAuthMetadata,
        redirect_uri: &str,
    ) -> Result<ClientRegistration, McpOAuthError> {
        let registration_endpoint = metadata.registration_endpoint.as_ref().ok_or_else(|| {
            McpOAuthError::RegistrationFailed(
                "no registration_endpoint in OAuth metadata".to_string(),
            )
        })?;

        let registration_body = build_registration_body(redirect_uri);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("failed to build HTTP client");
        let response = client
            .post(registration_endpoint)
            .header("Content-Type", "application/json")
            .json(&registration_body)
            .send()
            .await
            .map_err(|error| McpOAuthError::RegistrationFailed(error.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| McpOAuthError::RegistrationFailed(error.to_string()))?;

        if !status.is_success() {
            return Err(McpOAuthError::RegistrationFailed(format!(
                "HTTP {status}: {body}"
            )));
        }

        let parsed: serde_json::Value = serde_json::from_str(&body)
            .map_err(|error| McpOAuthError::RegistrationFailed(format!("invalid JSON: {error}")))?;

        let client_id = parsed
            .get("client_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpOAuthError::RegistrationFailed(
                    "missing client_id in registration response".to_string(),
                )
            })?
            .to_string();

        let client_secret = parsed
            .get("client_secret")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        Ok(ClientRegistration {
            client_id,
            client_secret,
            redirect_uris: vec![redirect_uri.to_string()],
            grant_types: vec![
                "authorization_code".to_string(),
                "refresh_token".to_string(),
            ],
        })
    }

    pub fn start_authorization(
        &self,
        metadata: &OAuthMetadata,
        client_id: &str,
    ) -> Result<(String, CallbackHandle), McpOAuthError> {
        let callback_port = self.config.callback_port.unwrap_or(19876);
        let redirect_uri = format!("http://localhost:{callback_port}/callback");

        let code_verifier = generate_random_token(32)?;
        let code_challenge = code_challenge_s256(&code_verifier);
        let state = generate_state()?;

        let verifier = code_verifier.clone();
        let state_clone = state.clone();

        let auth_url = build_authorization_url(
            &metadata.authorization_endpoint,
            client_id,
            &redirect_uri,
            &metadata.scopes_supported,
            &state,
            &code_challenge,
            self.config.xaa.unwrap_or(false),
        );

        let handle = start_callback_server(callback_port, state_clone, verifier);

        Ok((auth_url, handle))
    }

    pub async fn exchange_token(
        &self,
        metadata: &OAuthMetadata,
        client_id: &str,
        client_secret: Option<&str>,
        auth_code: &str,
        _state: &str,
        code_verifier: &str,
    ) -> Result<McpServerCredentials, McpOAuthError> {
        let callback_port = self.config.callback_port.unwrap_or(19876);
        let redirect_uri = format!("http://localhost:{callback_port}/callback");

        let mut form_params: BTreeMap<&str, String> = BTreeMap::from([
            ("grant_type", "authorization_code".to_string()),
            ("code", auth_code.to_string()),
            ("redirect_uri", redirect_uri.clone()),
            ("client_id", client_id.to_string()),
            ("code_verifier", code_verifier.to_string()),
        ]);

        if let Some(secret) = client_secret {
            form_params.insert("client_secret", secret.to_string());
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("failed to build HTTP client");
        let response = client
            .post(&metadata.token_endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&form_params)
            .send()
            .await
            .map_err(|error| McpOAuthError::TokenExchangeFailed(error.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| McpOAuthError::TokenExchangeFailed(error.to_string()))?;

        if !status.is_success() {
            return Err(McpOAuthError::TokenExchangeFailed(format!(
                "HTTP {status}: {body}"
            )));
        }

        let token_response: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
            McpOAuthError::TokenExchangeFailed(format!("invalid JSON: {error}"))
        })?;

        let access_token = token_response
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpOAuthError::TokenExchangeFailed(
                    "missing access_token in token response".to_string(),
                )
            })?
            .to_string();

        let refresh_token = token_response
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let expires_in = token_response
            .get("expires_in")
            .and_then(serde_json::Value::as_u64);

        let expires_at = expires_in.map(|secs| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() + secs)
                .unwrap_or(0)
        });

        let scopes = token_response
            .get("scope")
            .and_then(|v| v.as_str())
            .map(|s| s.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();

        let credentials = McpServerCredentials {
            server_url: self.server_url.clone(),
            client_id: client_id.to_string(),
            client_secret: client_secret.map(str::to_string),
            access_token,
            refresh_token,
            expires_at,
            scopes,
            redirect_uri,
        };

        self.store
            .save_credentials(&credentials)
            .map_err(McpOAuthError::Io)?;

        Ok(credentials)
    }

    pub async fn refresh_token(
        &self,
        metadata: &OAuthMetadata,
        credentials: &McpServerCredentials,
    ) -> Result<McpServerCredentials, McpOAuthError> {
        let refresh_token = credentials.refresh_token.as_ref().ok_or_else(|| {
            McpOAuthError::TokenExchangeFailed("no refresh_token available for refresh".to_string())
        })?;

        let mut form_params: BTreeMap<&str, String> = BTreeMap::from([
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.clone()),
            ("client_id", credentials.client_id.clone()),
            ("scope", credentials.scopes.join(" ")),
        ]);

        if let Some(secret) = &credentials.client_secret {
            form_params.insert("client_secret", secret.clone());
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("failed to build HTTP client");
        let response = client
            .post(&metadata.token_endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&form_params)
            .send()
            .await
            .map_err(|error| McpOAuthError::TokenExchangeFailed(error.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| McpOAuthError::TokenExchangeFailed(error.to_string()))?;

        if !status.is_success() {
            return Err(McpOAuthError::TokenExchangeFailed(format!(
                "HTTP {status}: {body}"
            )));
        }

        let token_response: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
            McpOAuthError::TokenExchangeFailed(format!("invalid JSON: {error}"))
        })?;

        let access_token = token_response
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpOAuthError::TokenExchangeFailed(
                    "missing access_token in token response".to_string(),
                )
            })?
            .to_string();

        let new_refresh_token = token_response
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let expires_in = token_response
            .get("expires_in")
            .and_then(serde_json::Value::as_u64);

        let expires_at = expires_in.map(|secs| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() + secs)
                .unwrap_or(0)
        });

        let scopes = token_response
            .get("scope")
            .and_then(|v| v.as_str())
            .map_or_else(|| credentials.scopes.clone(), |s| s.split_whitespace().map(str::to_string).collect());

        let refreshed = McpServerCredentials {
            server_url: credentials.server_url.clone(),
            client_id: credentials.client_id.clone(),
            client_secret: credentials.client_secret.clone(),
            access_token,
            refresh_token: new_refresh_token.or_else(|| credentials.refresh_token.clone()),
            expires_at,
            scopes,
            redirect_uri: credentials.redirect_uri.clone(),
        };

        self.store
            .save_credentials(&refreshed)
            .map_err(McpOAuthError::Io)?;

        Ok(refreshed)
    }

    #[must_use]
    pub fn store(&self) -> &McpOAuthStore {
        &self.store
    }
}

pub struct CallbackHandle {
    port: u16,
    rx: oneshot::Receiver<Result<(String, String), McpOAuthError>>,
}

impl CallbackHandle {
    pub async fn wait_for_callback(self) -> Result<(String, String), McpOAuthError> {
        self.rx.await.map_err(|_| {
            McpOAuthError::AuthorizationFailed("callback channel closed".to_string())
        })?
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }
}

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><title>Authentication Successful</title>
<style>
body{font-family:system-ui,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#0a0a0a;color:#fff}
.card{text-align:center;padding:2rem;border:1px solid #333;border-radius:12px;background:#111}
h1{font-size:1.25rem;font-weight:600;margin:0 0 0.5rem}
p{color:#888;margin:0}
</style>
</head>
<body><div class="card"><h1>Authentication successful</h1><p>You can close this tab and return to the application.</p></div></body>
</html>"#;

fn start_callback_server(
    port: u16,
    expected_state: String,
    _code_verifier: String,
) -> CallbackHandle {
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let listener = match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => l,
            Err(error) => {
                let _ = tx.send(Err(McpOAuthError::AuthorizationFailed(format!(
                    "failed to bind callback port {port}: {error}"
                ))));
                return;
            }
        };

        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buffer = vec![0u8; 8192];
            match stream.read(&mut buffer).await {
                Ok(n) => {
                    let request = String::from_utf8_lossy(&buffer[..n]);
                    let (method, path) = parse_request_line(&request);

                    let (query, _is_post) = if method == "POST" {
                        let body = extract_post_body(&request, n);
                        (body, true)
                    } else {
                        let query = path
                            .split_once('?')
                            .map(|(_, q)| q.to_string())
                            .unwrap_or_default();
                        (query, false)
                    };

                    let params = parse_callback_params(&query);
                    let _ = tx.send(process_callback(&params, &expected_state));

                    let response_body = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        SUCCESS_HTML.len(),
                        SUCCESS_HTML
                    );
                    let _ = stream.write_all(response_body.as_bytes()).await;
                    let _ = stream.flush().await;
                }
                Err(error) => {
                    let _ = tx.send(Err(McpOAuthError::AuthorizationFailed(format!(
                        "failed to read callback: {error}"
                    ))));
                }
            }
        }
    });

    CallbackHandle { port, rx }
}

fn process_callback(
    params: &BTreeMap<String, String>,
    expected_state: &str,
) -> Result<(String, String), McpOAuthError> {
    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .map_or("unknown error", |s| s.as_str());
        return Err(McpOAuthError::AuthorizationFailed(format!(
            "{error}: {description}"
        )));
    }

    let code = params.get("code").cloned().ok_or_else(|| {
        McpOAuthError::AuthorizationFailed("no authorization code in callback".to_string())
    })?;

    let state = params
        .get("state")
        .cloned()
        .ok_or_else(|| McpOAuthError::AuthorizationFailed("no state in callback".to_string()))?;

    if state != expected_state {
        return Err(McpOAuthError::AuthorizationFailed(format!(
            "state mismatch: expected {expected_state}, got {state}"
        )));
    }

    Ok((code, state))
}

fn parse_request_line(request: &str) -> (&str, &str) {
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() >= 2 {
        (parts[0], parts[1])
    } else {
        ("GET", "/")
    }
}

fn extract_post_body(request: &str, n: usize) -> String {
    if let Some(pos) = request[..n].find("\r\n\r\n") {
        request[pos + 4..n].to_string()
    } else if let Some(pos) = request[..n].find("\n\n") {
        request[pos + 2..n].to_string()
    } else {
        String::new()
    }
}

fn parse_callback_params(query: &str) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair
            .split_once('=')
            .map_or((pair, ""), |(key, value)| (key, value));
        if let (Ok(k), Ok(v)) = (percent_decode(key), percent_decode(value)) {
            params.insert(k, v);
        }
    }
    params
}

fn percent_decode(value: &str) -> Result<String, String> {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hi = decode_hex(bytes[index + 1])?;
                let lo = decode_hex(bytes[index + 2])?;
                decoded.push((hi << 4) | lo);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|error| error.to_string())
}

fn decode_hex(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid percent byte: {byte}")),
    }
}

fn generate_random_token(byte_count: usize) -> io::Result<String> {
    use std::fs::File;
    use std::io::Read;

    let mut buffer = vec![0u8; byte_count];
    File::open("/dev/urandom")?.read_exact(&mut buffer)?;
    Ok(base64url_encode(&buffer))
}

fn generate_state() -> io::Result<String> {
    generate_random_token(32)
}

#[must_use]
fn code_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64url_encode(&digest)
}

fn base64url_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let block = (u32::from(bytes[index]) << 16)
            | (u32::from(bytes[index + 1]) << 8)
            | u32::from(bytes[index + 2]);
        output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
        output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
        output.push(TABLE[((block >> 6) & 0x3F) as usize] as char);
        output.push(TABLE[(block & 0x3F) as usize] as char);
        index += 3;
    }
    match bytes.len().saturating_sub(index) {
        1 => {
            let block = u32::from(bytes[index]) << 16;
            output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
        }
        2 => {
            let block = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
            output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 6) & 0x3F) as usize] as char);
        }
        _ => {}
    }
    output
}

#[must_use]
fn build_authorization_url(
    authorize_url: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    state: &str,
    code_challenge: &str,
    xaa: bool,
) -> String {
    let mut params = vec![
        ("response_type", "code".to_string()),
        ("client_id", client_id.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("scope", scopes.join(" ")),
        ("state", state.to_string()),
        ("code_challenge", code_challenge.to_string()),
        ("code_challenge_method", "S256".to_string()),
    ];

    if xaa {
        params.push(("prompt", "consent".to_string()));
    }

    let query = params
        .into_iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                percent_encode_param(key),
                percent_encode_param(&value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");

    format!(
        "{}{}{}",
        authorize_url,
        if authorize_url.contains('?') {
            '&'
        } else {
            '?'
        },
        query
    )
}

fn percent_encode_param(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

#[must_use]
pub fn build_token_exchange_form(
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    code_verifier: &str,
) -> String {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
    ];
    params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode_param(k), percent_encode_param(v)))
        .collect::<Vec<_>>()
        .join("&")
}

#[must_use]
pub fn build_registration_body(redirect_uri: &str) -> serde_json::Value {
    serde_json::json!({
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_method": "none",
        "response_types": ["code"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metadata() -> OAuthMetadata {
        OAuthMetadata {
            authorization_endpoint: "https://auth.example.com/authorize".to_string(),
            token_endpoint: "https://auth.example.com/token".to_string(),
            registration_endpoint: Some("https://auth.example.com/register".to_string()),
            scopes_supported: vec![
                "openid".to_string(),
                "profile".to_string(),
                "mcp:read".to_string(),
            ],
        }
    }

    #[test]
    fn parses_oauth_metadata_from_json() {
        let json = r#"{
            "authorization_endpoint": "https://auth.example.com/authorize",
            "token_endpoint": "https://auth.example.com/token",
            "registration_endpoint": "https://auth.example.com/register",
            "scopes_supported": ["openid", "profile", "mcp:read"]
        }"#;

        let raw: serde_json::Value = serde_json::from_str(json).expect("valid JSON");

        let authorization_endpoint = raw
            .get("authorization_endpoint")
            .and_then(|v| v.as_str())
            .expect("authorization_endpoint")
            .to_string();
        let token_endpoint = raw
            .get("token_endpoint")
            .and_then(|v| v.as_str())
            .expect("token_endpoint")
            .to_string();
        let registration_endpoint = raw
            .get("registration_endpoint")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let scopes_supported = raw
            .get("scopes_supported")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let metadata = OAuthMetadata {
            authorization_endpoint,
            token_endpoint,
            registration_endpoint,
            scopes_supported,
        };

        assert_eq!(metadata, sample_metadata());
    }

    #[test]
    fn handles_metadata_missing_optional_fields() {
        let json = r#"{
            "authorization_endpoint": "https://auth.example.com/authorize",
            "token_endpoint": "https://auth.example.com/token"
        }"#;

        let raw: serde_json::Value = serde_json::from_str(json).expect("valid JSON");

        let authorization_endpoint = raw
            .get("authorization_endpoint")
            .and_then(|v| v.as_str())
            .expect("authorization_endpoint")
            .to_string();
        let token_endpoint = raw
            .get("token_endpoint")
            .and_then(|v| v.as_str())
            .expect("token_endpoint")
            .to_string();
        let registration_endpoint = raw
            .get("registration_endpoint")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let scopes_supported: Vec<String> = raw
            .get("scopes_supported")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let metadata = OAuthMetadata {
            authorization_endpoint,
            token_endpoint,
            registration_endpoint,
            scopes_supported,
        };

        assert!(metadata.registration_endpoint.is_none());
        assert!(metadata.scopes_supported.is_empty());
    }

    #[test]
    fn formats_client_registration_request_body() {
        let body = build_registration_body("http://localhost:19876/callback");

        let redirect_uris = body["redirect_uris"].as_array().expect("redirect_uris");
        assert_eq!(redirect_uris.len(), 1);
        assert_eq!(
            redirect_uris[0].as_str().expect("string"),
            "http://localhost:19876/callback"
        );

        let grant_types = body["grant_types"].as_array().expect("grant_types");
        assert_eq!(grant_types.len(), 2);
        assert_eq!(
            grant_types[0].as_str().expect("string"),
            "authorization_code"
        );
        assert_eq!(grant_types[1].as_str().expect("string"), "refresh_token");

        assert_eq!(
            body["token_endpoint_auth_method"].as_str().expect("string"),
            "none"
        );
        assert_eq!(
            body["response_types"].as_array().expect("response_types")[0]
                .as_str()
                .expect("string"),
            "code"
        );
    }

    #[test]
    fn formats_token_exchange_request_body() {
        let form = build_token_exchange_form(
            "auth-code-123",
            "http://localhost:19876/callback",
            "test-client-id",
            "test-code-verifier",
        );

        assert!(form.contains("grant_type=authorization_code"));
        assert!(form.contains("code=auth-code-123"));
        assert!(form.contains("redirect_uri=http%3A%2F%2Flocalhost%3A19876%2Fcallback"));
        assert!(form.contains("client_id=test-client-id"));
        assert!(form.contains("code_verifier=test-code-verifier"));
    }

    #[test]
    fn generates_pkce_pair_and_state() {
        let verifier = generate_random_token(32).expect("verifier");
        let challenge = code_challenge_s256(&verifier);
        let state = generate_state().expect("state");

        assert!(!verifier.is_empty());
        assert!(!challenge.is_empty());
        assert!(!state.is_empty());
        assert_ne!(verifier, challenge);
    }

    #[test]
    fn builds_authorization_url_correctly() {
        let url = build_authorization_url(
            "https://auth.example.com/authorize",
            "client-123",
            "http://localhost:19876/callback",
            &["openid".to_string(), "profile".to_string()],
            "state-xyz",
            "challenge-abc",
            false,
        );

        assert!(url.starts_with("https://auth.example.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=client-123"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A19876%2Fcallback"));
        assert!(url.contains("scope=openid%20profile"));
        assert!(url.contains("state=state-xyz"));
        assert!(url.contains("code_challenge=challenge-abc"));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn builds_authorization_url_with_xaa_prompt() {
        let url = build_authorization_url(
            "https://auth.example.com/authorize",
            "client-123",
            "http://localhost:19876/callback",
            &[],
            "state-xyz",
            "challenge-abc",
            true,
        );

        assert!(url.contains("prompt=consent"));
    }

    #[test]
    fn parses_callback_params() {
        let query = "code=abc123&state=xyz789";
        let params = parse_callback_params(query);

        assert_eq!(params.get("code").map(String::as_str), Some("abc123"));
        assert_eq!(params.get("state").map(String::as_str), Some("xyz789"));
    }

    #[test]
    fn parses_callback_params_with_percent_encoding() {
        let query = "code=abc%2B123&state=xyz%20789&error_description=access%20denied";
        let params = parse_callback_params(query);

        assert_eq!(params.get("code").map(String::as_str), Some("abc+123"));
        assert_eq!(params.get("state").map(String::as_str), Some("xyz 789"));
        assert_eq!(
            params.get("error_description").map(String::as_str),
            Some("access denied")
        );
    }

    #[test]
    fn callback_processing_rejects_state_mismatch() {
        let params = BTreeMap::from([
            ("code".to_string(), "abc".to_string()),
            ("state".to_string(), "wrong".to_string()),
        ]);
        let result = process_callback(&params, "expected");
        assert!(result.is_err());
        match result {
            Err(McpOAuthError::AuthorizationFailed(msg)) => {
                assert!(msg.contains("state mismatch"));
            }
            _ => panic!("expected AuthorizationFailed"),
        }
    }

    #[test]
    fn callback_processing_rejects_error_response() {
        let params = BTreeMap::from([
            ("error".to_string(), "access_denied".to_string()),
            (
                "error_description".to_string(),
                "User denied consent".to_string(),
            ),
        ]);
        let result = process_callback(&params, "state");
        assert!(result.is_err());
        match result {
            Err(McpOAuthError::AuthorizationFailed(msg)) => {
                assert!(msg.contains("access_denied"));
            }
            _ => panic!("expected AuthorizationFailed"),
        }
    }

    #[test]
    fn callback_processing_accepts_valid_code() {
        let params = BTreeMap::from([
            ("code".to_string(), "auth-code".to_string()),
            ("state".to_string(), "matching-state".to_string()),
        ]);
        let result = process_callback(&params, "matching-state");
        assert!(result.is_ok());
        let (code, state) = result.expect("valid");
        assert_eq!(code, "auth-code");
        assert_eq!(state, "matching-state");
    }

    #[test]
    fn credential_store_round_trip() {
        let temp_dir = std::env::temp_dir().join(format!("mcp-oauth-test-{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let store_path = temp_dir.join("mcp_oauth.json");

        let store = McpOAuthStore::new(store_path);
        let creds = McpServerCredentials {
            server_url: "https://mcp.example.com".to_string(),
            client_id: "client-123".to_string(),
            client_secret: Some("secret".to_string()),
            access_token: "access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Some(1_700_000_000),
            scopes: vec!["openid".to_string()],
            redirect_uri: "http://localhost:19876/callback".to_string(),
        };

        store.save_credentials(&creds).expect("save credentials");
        let loaded = store
            .load_credentials("https://mcp.example.com")
            .expect("load credentials")
            .expect("credentials should exist");
        assert_eq!(loaded, creds);

        store
            .remove_credentials("https://mcp.example.com")
            .expect("remove credentials");
        let removed = store
            .load_credentials("https://mcp.example.com")
            .expect("load after remove");
        assert!(removed.is_none());

        std::fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }
}

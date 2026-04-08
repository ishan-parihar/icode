use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub token_type: String,
}

/// Initiate a device code flow (RFC 8628).
pub async fn initiate_device_flow(
    client_id: &str,
    scope: &str,
    device_authorization_url: &str,
) -> Result<DeviceCodeResponse, reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("failed to build HTTP client");
    let params = [("client_id", client_id), ("scope", scope)];
    let resp = client
        .post(device_authorization_url)
        .form(&params)
        .send()
        .await?;
    resp.json::<DeviceCodeResponse>().await
}

/// Poll for token using device code (RFC 8628 Section 3.5).
/// Retries every `interval_secs` until `max_attempts` or success.
pub async fn poll_for_token(
    client_id: &str,
    device_code: &str,
    token_url: &str,
    interval_secs: u64,
    max_attempts: u32,
) -> Result<TokenResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("failed to build HTTP client");
    let params = [
        ("client_id", client_id),
        ("device_code", device_code),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
    ];
    for _attempt in 0..max_attempts {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        let resp = client
            .post(token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;
        let status = resp.status();
        if status.is_success() {
            let token = resp
                .json::<TokenResponse>()
                .await
                .map_err(|e| format!("Failed to parse token response: {e}"))?;
            return Ok(token);
        }
        // Handle authorization_pending (continue polling) vs other errors
        if status == reqwest::StatusCode::BAD_REQUEST {
            // Could parse error body to check for authorization_pending vs slow_down
            // For now, just continue polling
            continue;
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            // slow_down - increase interval
            continue;
        }
        return Err(format!("Token request failed: HTTP {}", status.as_u16()));
    }
    Err(format!("Device code expired after {max_attempts} attempts"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_code_response_deserializes() {
        let json = r#"{
            "device_code": "abc123",
            "user_code": "XYZ-123",
            "verification_uri": "https://example.com/device",
            "expires_in": 900,
            "interval": 5
        }"#;
        let resp: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.device_code, "abc123");
        assert_eq!(resp.user_code, "XYZ-123");
        assert_eq!(resp.expires_in, 900);
        assert_eq!(resp.interval, 5);
    }

    #[test]
    fn token_response_deserializes() {
        let json = r#"{
            "access_token": "at-123",
            "refresh_token": "rt-456",
            "expires_in": 3600,
            "token_type": "Bearer"
        }"#;
        let resp: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "at-123");
        assert_eq!(resp.refresh_token.as_deref(), Some("rt-456"));
        assert_eq!(resp.token_type, "Bearer");
    }
}

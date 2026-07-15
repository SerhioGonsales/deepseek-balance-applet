// SPDX-License-Identifier: MPL-2.0 (Mozilla Public License 2.0)

//! `DeepSeek` API client for fetching account balance.

use serde::Deserialize;

/// Top-level response from `GET /user/balance`.
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceResponse {
    #[serde(default)]
    pub is_available: bool,
    #[serde(default)]
    pub balance_infos: Vec<BalanceInfo>,
}

/// Per-currency balance information.
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceInfo {
    pub currency: String,
    pub total_balance: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub topped_up_balance: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub granted_balance: String,
}

/// Fetch the current account balance from the `DeepSeek` API.
///
/// # Errors
///
/// Returns a human-readable error string on network failure, non-2xx status,
/// or JSON deserialization error.
pub async fn fetch_balance(api_key: &str) -> Result<BalanceResponse, String> {
    let client = reqwest::Client::builder()
        .user_agent(format!(
            "{}/{}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?;

    let response = client
        .get("https://api.deepseek.com/user/balance")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err("AUTH_ERROR".to_string());
        }
        return Err(format!(
            "API error ({}): {}",
            status.as_u16(),
            body.trim()
        ));
    }

    response
        .json::<BalanceResponse>()
        .await
        .map_err(|e| format!("failed to parse response: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_balance_response() {
        let json = r#"{
            "is_available": true,
            "balance_infos": [
                {
                    "currency": "USD",
                    "total_balance": "14.00",
                    "topped_up_balance": "10.00",
                    "granted_balance": "4.00"
                }
            ]
        }"#;
        let resp: BalanceResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_available);
        assert_eq!(resp.balance_infos.len(), 1);
        assert_eq!(resp.balance_infos[0].currency, "USD");
        assert_eq!(resp.balance_infos[0].total_balance, "14.00");
    }

    #[test]
    fn parses_balance_with_missing_fields() {
        let json = r#"{
            "is_available": true,
            "balance_infos": [
                {
                    "currency": "CNY",
                    "total_balance": "50.00"
                }
            ]
        }"#;
        let resp: BalanceResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_available);
        assert_eq!(resp.balance_infos[0].currency, "CNY");
        // topped_up and granted should default to empty
        assert_eq!(resp.balance_infos[0].topped_up_balance, "");
        assert_eq!(resp.balance_infos[0].granted_balance, "");
    }

    #[test]
    fn default_config_has_no_api_key() {
        let config = crate::config::Config::default();
        assert!(config.api_key.is_empty());
        assert_eq!(config.refresh_interval_secs, 180);
    }
}

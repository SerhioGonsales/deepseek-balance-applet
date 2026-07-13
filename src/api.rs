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
    pub topped_up_balance: String,
    #[serde(default)]
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

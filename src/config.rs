// SPDX-License-Identifier: MPL-2.0 (Mozilla Public License 2.0)

use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};

#[derive(Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 2]
pub struct Config {
    /// `DeepSeek` API key for authentication.
    pub api_key: String,
    /// Balance refresh interval in seconds (minimum 30s enforced in app logic).
    pub refresh_interval_secs: u64,
    /// Date (`YYYY-MM-DD`, local time) the current spend-tracking baseline
    /// was recorded on.
    pub spend_day: String,
    /// Balance recorded at the start of `spend_day`, as a decimal string.
    pub spend_day_start_balance: String,
    /// UI language: "en" or "ru".
    pub language: String,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field(
                "api_key",
                &if self.api_key.is_empty() {
                    "<empty>"
                } else {
                    "<redacted>"
                },
            )
            .field("refresh_interval_secs", &self.refresh_interval_secs)
            .field("spend_day", &self.spend_day)
            .field("spend_day_start_balance", &self.spend_day_start_balance)
            .field("language", &self.language)
            .finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            refresh_interval_secs: 180,
            spend_day: String::new(),
            spend_day_start_balance: String::new(),
            language: String::from("en"),
        }
    }
}

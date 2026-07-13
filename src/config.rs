// SPDX-License-Identifier: MPL-2.0 (Mozilla Public License 2.0)

use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    /// `DeepSeek` API key for authentication.
    pub api_key: String,
    /// Balance refresh interval in seconds (minimum 30s enforced in app logic).
    pub refresh_interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            refresh_interval_secs: 180, // 3 minutes
        }
    }
}

//! The persisted auto settings (ADR-0009): what is switched on, and which saved
//! profiles take part in it.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{AutoSettings, ConnectivitySettings};

/// Auto connect / auto switch for one kind of setting (proxy or DNS).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoKind {
    /// Connect when something answers; disconnect when nothing does.
    #[serde(default)]
    pub connect: bool,
    /// Stay on the fastest profile of the pool.
    #[serde(default)]
    pub switch: bool,
    /// Ids of the profiles that take part, in priority order.
    #[serde(default)]
    pub pool: Vec<String>,
}

impl AutoKind {
    /// Whether either option is on (if not, no probes are sent at all).
    pub fn is_enabled(&self) -> bool {
        self.connect || self.switch
    }

    /// The decision-engine settings for this kind.
    pub fn settings(&self) -> AutoSettings {
        AutoSettings::new(self.connect, self.switch)
    }

    /// Adds `id` to the end of the pool; false if it was already there.
    pub fn add_to_pool(&mut self, id: &str) -> bool {
        if self.pool.iter().any(|existing| existing == id) {
            return false;
        }
        self.pool.push(id.to_owned());
        true
    }

    /// Removes `id` from the pool; false if it was not there.
    pub fn remove_from_pool(&mut self, id: &str) -> bool {
        let before = self.pool.len();
        self.pool.retain(|existing| existing != id);
        self.pool.len() != before
    }
}

/// Everything the auto modes need to know.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoConfig {
    /// Auto settings for the proxy.
    #[serde(default)]
    pub proxy: AutoKind,
    /// Auto settings for DNS.
    #[serde(default)]
    pub dns: AutoKind,
    /// What the connectivity tests talk to, and how often they run.
    #[serde(default)]
    pub connectivity: ConnectivitySettings,
}

/// Why the auto settings could not be loaded or saved.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AutoConfigError {
    /// The stored data is unreadable or invalid.
    #[error("stored auto settings are corrupt: {0}")]
    Corrupt(String),
    /// The data was written by a newer version of Hopswitch.
    #[error("auto settings were saved by a newer version (file v{found}, supported v{supported})")]
    UnsupportedVersion {
        /// Version found in storage.
        found: u32,
        /// Highest version this build understands.
        supported: u32,
    },
    /// The storage could not be accessed.
    #[error("could not access stored auto settings: {0}")]
    Io(String),
}

/// Where the auto settings live between runs.
pub trait AutoConfigStore: Send + Sync {
    /// The saved settings; the defaults if nothing was ever saved.
    fn load(&self) -> Result<AutoConfig, AutoConfigError>;
    /// Durably replaces the saved settings.
    fn save(&self, config: &AutoConfig) -> Result<(), AutoConfigError>;
}

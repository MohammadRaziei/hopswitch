//! The port for loading and saving the user's profiles.

use thiserror::Error;

use crate::Profiles;

/// Why loading or saving profiles failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepositoryError {
    /// The stored data is unreadable or invalid.
    #[error("stored profiles are corrupt: {0}")]
    Corrupt(String),
    /// The data was written by a newer version of Hopswitch.
    #[error("profiles were saved by a newer version (file v{found}, supported v{supported})")]
    UnsupportedVersion {
        /// Version found in storage.
        found: u32,
        /// Highest version this build understands.
        supported: u32,
    },
    /// The storage could not be accessed.
    #[error("could not access stored profiles: {0}")]
    Io(String),
}

/// Where profiles are persisted (a JSON file in `hopswitch-storage`).
pub trait ProfileRepository: Send + Sync {
    /// Loads all profiles; an empty collection if nothing was ever saved.
    fn load(&self) -> Result<Profiles, RepositoryError>;
    /// Durably replaces the stored profiles with `profiles`.
    fn save(&self, profiles: &Profiles) -> Result<(), RepositoryError>;
}

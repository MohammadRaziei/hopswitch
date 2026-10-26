//! A proxy auto-config (PAC) script address as a first-class system proxy state.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ValidationError;

const MAX_URL_LEN: usize = 2048;
const SCHEMES: [&str; 3] = ["http://", "https://", "file://"];

/// The address of a PAC script: `http`, `https` or `file`, no whitespace, at most 2048 characters.
///
/// The URL is kept exactly as given, so reading a setting and writing it back changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PacConfigDto")]
pub struct PacConfig {
    url: String,
}

/// Untrusted wire representation; converted through [`PacConfig::new`].
#[derive(Deserialize)]
struct PacConfigDto {
    url: String,
}

impl TryFrom<PacConfigDto> for PacConfig {
    type Error = ValidationError;

    fn try_from(dto: PacConfigDto) -> Result<Self, Self::Error> {
        Self::new(&dto.url)
    }
}

impl PacConfig {
    /// Validates a PAC script address.
    ///
    /// # Errors
    /// [`ValidationError::InvalidPacUrl`] when the scheme is not `http`, `https` or `file`, nothing
    /// follows the scheme, the text contains whitespace or control characters, or it is longer
    /// than 2048 characters.
    pub fn new(url: &str) -> Result<Self, ValidationError> {
        let invalid = || ValidationError::InvalidPacUrl(url.to_owned());
        if url.len() > MAX_URL_LEN || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(invalid());
        }
        let lower = url.to_ascii_lowercase();
        let has_target = SCHEMES
            .iter()
            .find_map(|scheme| lower.strip_prefix(scheme))
            .is_some_and(|rest| !rest.is_empty());
        if !has_target {
            return Err(invalid());
        }
        Ok(Self {
            url: url.to_owned(),
        })
    }

    /// The script address, as given.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl fmt::Display for PacConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url)
    }
}

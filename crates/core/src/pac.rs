//! A proxy auto-config (PAC) script address as a first-class system proxy state.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::ValidationError;

/// Longest address kept. A cap of our own choosing, far above any real PAC address; not a limit of
/// any particular client.
const MAX_URL_LEN: usize = 2048;
/// Where Hopswitch's own PAC server serves its script; the whole address is
/// `http://127.0.0.1:<port>` followed by this path.
pub const HOPSWITCH_PAC_PATH: &str = "/hopswitch.pac";
/// The address schemes a PAC script can be loaded from.
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

    /// The address of Hopswitch's own PAC server on `port` (loopback, [`HOPSWITCH_PAC_PATH`]).
    #[must_use]
    pub fn loopback(port: u16) -> Self {
        Self {
            url: format!("http://127.0.0.1:{port}{HOPSWITCH_PAC_PATH}"),
        }
    }

    /// The port, when this is exactly the address of Hopswitch's own PAC server
    /// (`http://127.0.0.1:<port>/hopswitch.pac`, nothing else), so that a setting left behind by a
    /// crashed Hopswitch can be told apart from a PAC script an organisation set.
    #[must_use]
    pub fn hopswitch_port(&self) -> Option<u16> {
        let rest = self.url.get(7..).filter(|_| {
            self.url
                .get(..7)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"))
        })?;
        let (authority, path) = rest.split_once('/')?;
        if path != &HOPSWITCH_PAC_PATH[1..] {
            return None;
        }
        let digits = authority.strip_prefix("127.0.0.1:")?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse::<u16>().ok().filter(|port| *port != 0)
    }

    /// The script address, as given.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl FromStr for PacConfig {
    type Err = ValidationError;

    fn from_str(url: &str) -> Result<Self, Self::Err> {
        Self::new(url)
    }
}

impl fmt::Display for PacConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url)
    }
}

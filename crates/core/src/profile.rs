//! Profile aggregate: a named bundle of proxy and/or DNS settings.

use serde::{Deserialize, Serialize};

use crate::{DnsConfig, ProxyConfig, ValidationError};

/// Maximum profile name length, counted in characters (not bytes).
pub const MAX_NAME_CHARS: usize = 64;

/// A named bundle of proxy and/or DNS settings that can be applied together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProfileDto")]
pub struct Profile {
    id: String,
    name: String,
    proxy: Option<ProxyConfig>,
    dns: Option<DnsConfig>,
}

/// Untrusted wire representation; converted through [`Profile::new`].
#[derive(Deserialize)]
struct ProfileDto {
    id: String,
    name: String,
    #[serde(default)]
    proxy: Option<ProxyConfig>,
    #[serde(default)]
    dns: Option<DnsConfig>,
}

impl TryFrom<ProfileDto> for Profile {
    type Error = ValidationError;

    fn try_from(dto: ProfileDto) -> Result<Self, Self::Error> {
        Self::new(dto.id, dto.name, dto.proxy, dto.dns)
    }
}

impl Profile {
    /// Builds a validated profile. At least one of `proxy` / `dns` is required.
    pub fn new(
        id: impl Into<String>,
        name: impl AsRef<str>,
        proxy: Option<ProxyConfig>,
        dns: Option<DnsConfig>,
    ) -> Result<Self, ValidationError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(ValidationError::EmptyId);
        }

        let name = name.as_ref().trim();
        if name.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_CHARS {
            return Err(ValidationError::NameTooLong {
                max: MAX_NAME_CHARS,
            });
        }

        if proxy.is_none() && dns.is_none() {
            return Err(ValidationError::EmptyProfile);
        }

        Ok(Self {
            id,
            name: name.to_owned(),
            proxy,
            dns,
        })
    }

    /// Stable identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name (trimmed).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Proxy settings, if any.
    pub fn proxy(&self) -> Option<&ProxyConfig> {
        self.proxy.as_ref()
    }

    /// DNS settings, if any.
    pub fn dns(&self) -> Option<&DnsConfig> {
        self.dns.as_ref()
    }
}

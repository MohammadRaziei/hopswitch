//! Saved entries: a [`ProxyProfile`] and a [`DnsProfile`] are independent
//! aggregates (ADR-0004). Activating, snapshotting and rolling back one never
//! touches the other.

use serde::{Deserialize, Serialize};

use crate::{DnsConfig, ProxyConfig, ValidationError};

/// Maximum profile name length, counted in characters (not bytes).
pub const MAX_NAME_CHARS: usize = 64;

/// Validates the identity shared by every profile kind; returns `(id, trimmed name)`.
pub(crate) fn validate_identity(
    id: impl Into<String>,
    name: &str,
) -> Result<(String, String), ValidationError> {
    let id = id.into();
    if id.trim().is_empty() {
        return Err(ValidationError::EmptyId);
    }

    let name = name.trim();
    if name.is_empty() {
        return Err(ValidationError::EmptyName);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ValidationError::NameTooLong {
            max: MAX_NAME_CHARS,
        });
    }
    Ok((id, name.to_owned()))
}

/// A named, saved system-proxy setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProxyProfileDto")]
pub struct ProxyProfile {
    id: String,
    name: String,
    config: ProxyConfig,
}

/// Untrusted wire representation; converted through [`ProxyProfile::new`].
#[derive(Deserialize)]
struct ProxyProfileDto {
    id: String,
    name: String,
    config: ProxyConfig,
}

impl TryFrom<ProxyProfileDto> for ProxyProfile {
    type Error = ValidationError;

    fn try_from(dto: ProxyProfileDto) -> Result<Self, Self::Error> {
        Self::new(dto.id, dto.name, dto.config)
    }
}

impl ProxyProfile {
    /// Builds a validated profile (non-empty id, trimmed name of at most
    /// [`MAX_NAME_CHARS`] characters).
    pub fn new(
        id: impl Into<String>,
        name: impl AsRef<str>,
        config: ProxyConfig,
    ) -> Result<Self, ValidationError> {
        let (id, name) = validate_identity(id, name.as_ref())?;
        Ok(Self { id, name, config })
    }

    /// Stable identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name (trimmed).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The settings this profile applies.
    pub fn config(&self) -> &ProxyConfig {
        &self.config
    }
}

/// A named, saved system-DNS setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DnsProfileDto")]
pub struct DnsProfile {
    id: String,
    name: String,
    config: DnsConfig,
}

/// Untrusted wire representation; converted through [`DnsProfile::new`].
#[derive(Deserialize)]
struct DnsProfileDto {
    id: String,
    name: String,
    config: DnsConfig,
}

impl TryFrom<DnsProfileDto> for DnsProfile {
    type Error = ValidationError;

    fn try_from(dto: DnsProfileDto) -> Result<Self, Self::Error> {
        Self::new(dto.id, dto.name, dto.config)
    }
}

impl DnsProfile {
    /// Builds a validated profile (non-empty id, trimmed name of at most
    /// [`MAX_NAME_CHARS`] characters).
    pub fn new(
        id: impl Into<String>,
        name: impl AsRef<str>,
        config: DnsConfig,
    ) -> Result<Self, ValidationError> {
        let (id, name) = validate_identity(id, name.as_ref())?;
        Ok(Self { id, name, config })
    }

    /// Stable identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name (trimmed).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The settings this profile applies.
    pub fn config(&self) -> &DnsConfig {
        &self.config
    }
}

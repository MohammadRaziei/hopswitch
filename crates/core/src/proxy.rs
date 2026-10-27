//! Proxy configuration value object.
//!
//! A [`ProxyConfig`] can only be built through validating constructors (or by
//! deserialising, which goes through the same validation), so holding one is
//! proof that it is well-formed.

use std::{fmt, net::IpAddr, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::ValidationError;

/// Longest host name in text form (RFC 1035).
const MAX_HOSTNAME_LEN: usize = 253;
/// Longest single label of a host name (RFC 1035).
const MAX_LABEL_LEN: usize = 63;

/// Wire protocol of the upstream proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyScheme {
    /// Plain HTTP proxy.
    Http,
    /// HTTPS proxy.
    Https,
    /// SOCKS4 proxy.
    Socks4,
    /// SOCKS5 proxy.
    Socks5,
}

impl ProxyScheme {
    /// Lowercase name used in URLs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks4 => "socks4",
            Self::Socks5 => "socks5",
        }
    }
}

impl fmt::Display for ProxyScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProxyScheme {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "http" => Ok(Self::Http),
            "https" => Ok(Self::Https),
            "socks4" => Ok(Self::Socks4),
            "socks5" => Ok(Self::Socks5),
            _ => Err(ValidationError::UnsupportedScheme(s.to_owned())),
        }
    }
}

/// A validated proxy endpoint with an optional bypass list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProxyConfigDto")]
pub struct ProxyConfig {
    scheme: ProxyScheme,
    host: String,
    port: u16,
    bypass: Vec<String>,
}

/// Untrusted wire representation; converted through [`ProxyConfig::new`].
#[derive(Deserialize)]
struct ProxyConfigDto {
    scheme: ProxyScheme,
    host: String,
    port: u16,
    #[serde(default)]
    bypass: Vec<String>,
}

impl TryFrom<ProxyConfigDto> for ProxyConfig {
    type Error = ValidationError;

    fn try_from(dto: ProxyConfigDto) -> Result<Self, Self::Error> {
        Self::new(dto.scheme, &dto.host, dto.port)?.with_bypass(dto.bypass)
    }
}

impl ProxyConfig {
    /// Builds a validated proxy. Hostnames are normalised to lowercase.
    pub fn new(scheme: ProxyScheme, host: &str, port: u16) -> Result<Self, ValidationError> {
        if port == 0 {
            return Err(ValidationError::InvalidPort);
        }
        Ok(Self {
            scheme,
            host: normalise_host(host)?,
            port,
            bypass: Vec::new(),
        })
    }

    /// Replaces the bypass list. Entries are trimmed; empty entries are rejected.
    pub fn with_bypass(mut self, entries: Vec<String>) -> Result<Self, ValidationError> {
        let mut cleaned = Vec::with_capacity(entries.len());
        for entry in entries {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                return Err(ValidationError::InvalidBypassEntry);
            }
            cleaned.push(trimmed.to_owned());
        }
        self.bypass = cleaned;
        Ok(self)
    }

    /// Proxy protocol.
    pub fn scheme(&self) -> ProxyScheme {
        self.scheme
    }

    /// Normalised host (hostname or IP literal, IPv6 without brackets).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Port, always in `1..=65535`.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Hosts that must not go through the proxy.
    pub fn bypass(&self) -> &[String] {
        &self.bypass
    }
}

impl FromStr for ProxyConfig {
    type Err = ValidationError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let malformed = || ValidationError::MalformedUrl(input.to_owned());

        let (scheme_raw, rest) = input.trim().split_once("://").ok_or_else(malformed)?;
        let scheme: ProxyScheme = scheme_raw.parse()?;
        let rest = rest.strip_suffix('/').unwrap_or(rest);

        let (host, port_raw) = split_host_port(rest).ok_or_else(malformed)?;
        if port_raw.is_empty() || !port_raw.bytes().all(|b| b.is_ascii_digit()) {
            return Err(malformed());
        }
        let port = port_raw
            .parse::<u16>()
            .map_err(|_| ValidationError::InvalidPort)?;

        Self::new(scheme, host, port)
    }
}

impl fmt::Display for ProxyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "{}://[{}]:{}", self.scheme, self.host, self.port)
        } else {
            write!(f, "{}://{}:{}", self.scheme, self.host, self.port)
        }
    }
}

/// Splits `host:port` or `[ipv6]:port`. Bare IPv6 literals are rejected as ambiguous.
fn split_host_port(rest: &str) -> Option<(&str, &str)> {
    if let Some(inner) = rest.strip_prefix('[') {
        let (host, tail) = inner.split_once(']')?;
        return Some((host, tail.strip_prefix(':')?));
    }
    let (host, port) = rest.rsplit_once(':')?;
    if host.contains(':') {
        return None;
    }
    Some((host, port))
}

fn normalise_host(raw: &str) -> Result<String, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::EmptyHost);
    }
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return Ok(ip.to_string());
    }

    let lowered = trimmed.to_ascii_lowercase();
    let name = lowered.strip_suffix('.').unwrap_or(&lowered);
    let valid =
        !name.is_empty() && name.len() <= MAX_HOSTNAME_LEN && name.split('.').all(is_valid_label);

    if valid {
        Ok(name.to_owned())
    } else {
        Err(ValidationError::InvalidHost(trimmed.to_owned()))
    }
}

fn is_valid_label(label: &str) -> bool {
    (1..=MAX_LABEL_LEN).contains(&label.len())
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

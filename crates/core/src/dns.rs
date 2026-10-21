//! DNS configuration value object.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::ValidationError;

/// Maximum number of DNS servers per profile.
pub const MAX_DNS_SERVERS: usize = 4;

/// A validated, ordered list of DNS servers (priority = position).
///
/// Serialises as a plain JSON array of IP strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<IpAddr>", into = "Vec<IpAddr>")]
pub struct DnsConfig {
    servers: Vec<IpAddr>,
}

impl DnsConfig {
    /// Builds a validated configuration.
    ///
    /// Rejects empty lists, more than [`MAX_DNS_SERVERS`] entries, duplicates
    /// and unspecified addresses (`0.0.0.0`, `::`).
    pub fn new(servers: Vec<IpAddr>) -> Result<Self, ValidationError> {
        if servers.is_empty() {
            return Err(ValidationError::NoDnsServers);
        }
        if servers.len() > MAX_DNS_SERVERS {
            return Err(ValidationError::TooManyDnsServers {
                max: MAX_DNS_SERVERS,
            });
        }
        for (index, ip) in servers.iter().enumerate() {
            if ip.is_unspecified() {
                return Err(ValidationError::InvalidDnsServer(ip.to_string()));
            }
            if servers[..index].contains(ip) {
                return Err(ValidationError::DuplicateDnsServer(*ip));
            }
        }
        Ok(Self { servers })
    }

    /// Parses textual IP addresses (surrounding whitespace is ignored).
    pub fn from_strs<I, S>(items: I) -> Result<Self, ValidationError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let servers = items
            .into_iter()
            .map(|item| {
                let raw = item.as_ref().trim();
                raw.parse::<IpAddr>()
                    .map_err(|_| ValidationError::InvalidDnsServer(raw.to_owned()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(servers)
    }

    /// Servers in priority order.
    pub fn servers(&self) -> &[IpAddr] {
        &self.servers
    }
}

impl TryFrom<Vec<IpAddr>> for DnsConfig {
    type Error = ValidationError;

    fn try_from(servers: Vec<IpAddr>) -> Result<Self, Self::Error> {
        Self::new(servers)
    }
}

impl From<DnsConfig> for Vec<IpAddr> {
    fn from(config: DnsConfig) -> Self {
        config.servers
    }
}

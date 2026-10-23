//! Settings for the connectivity test: what to probe, how long to wait, how often.
//!
//! Probes are *real* tests (an HTTP request through a proxy, a DNS query sent to
//! a server), never ICMP ping: a proxy may ignore ping and still work (ADR-0009).

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MIN_TIMEOUT_MS: u32 = 500;
const MAX_TIMEOUT_MS: u32 = 10_000;
const MIN_INTERVAL_SECS: u32 = 5;
const MAX_INTERVAL_SECS: u32 = 3600;
const MAX_DOMAIN_CHARS: usize = 253;

/// Why connectivity or auto-switch settings were rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsError {
    /// The proxy test address is not a plain `http://` or `https://` URL.
    #[error("proxy test address must be an http:// or https:// URL without spaces: {0:?}")]
    InvalidProbeTarget(String),
    /// The DNS test domain is not a valid hostname.
    #[error("DNS test domain is not a valid hostname: {0:?}")]
    InvalidProbeDomain(String),
    /// A number is outside its allowed range.
    #[error("{what} must be between {min} and {max}")]
    OutOfRange {
        /// Which setting.
        what: &'static str,
        /// Smallest allowed value.
        min: u32,
        /// Largest allowed value.
        max: u32,
    },
}

/// What the connectivity test talks to. Can only be built valid, including via serde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ConnectivityDto")]
pub struct ConnectivitySettings {
    proxy_target: String,
    dns_domain: String,
    timeout_ms: u32,
    interval_secs: u32,
}

/// Untrusted wire representation; converted through [`ConnectivitySettings::new`].
#[derive(Deserialize)]
struct ConnectivityDto {
    proxy_target: String,
    dns_domain: String,
    timeout_ms: u32,
    interval_secs: u32,
}

impl TryFrom<ConnectivityDto> for ConnectivitySettings {
    type Error = SettingsError;

    fn try_from(dto: ConnectivityDto) -> Result<Self, Self::Error> {
        Self::new(
            &dto.proxy_target,
            &dto.dns_domain,
            dto.timeout_ms,
            dto.interval_secs,
        )
    }
}

impl Default for ConnectivitySettings {
    /// A light, cookie-free endpoint over plain HTTP, and a stable domain.
    fn default() -> Self {
        Self {
            proxy_target: "http://cp.cloudflare.com/generate_204".to_owned(),
            dns_domain: "example.com".to_owned(),
            timeout_ms: 3000,
            interval_secs: 30,
        }
    }
}

impl ConnectivitySettings {
    /// Builds validated settings (surrounding whitespace is ignored).
    pub fn new(
        proxy_target: &str,
        dns_domain: &str,
        timeout_ms: u32,
        interval_secs: u32,
    ) -> Result<Self, SettingsError> {
        let proxy_target = proxy_target.trim();
        if !is_http_url(proxy_target) {
            return Err(SettingsError::InvalidProbeTarget(proxy_target.to_owned()));
        }
        let dns_domain = dns_domain.trim();
        if !is_hostname(dns_domain) {
            return Err(SettingsError::InvalidProbeDomain(dns_domain.to_owned()));
        }
        check_range("timeout", timeout_ms, MIN_TIMEOUT_MS, MAX_TIMEOUT_MS)?;
        check_range(
            "test interval (seconds)",
            interval_secs,
            MIN_INTERVAL_SECS,
            MAX_INTERVAL_SECS,
        )?;
        Ok(Self {
            proxy_target: proxy_target.to_owned(),
            dns_domain: dns_domain.to_owned(),
            timeout_ms,
            interval_secs,
        })
    }

    /// The URL fetched through each proxy.
    pub fn proxy_target(&self) -> &str {
        &self.proxy_target
    }

    /// The domain resolved against each DNS server.
    pub fn dns_domain(&self) -> &str {
        &self.dns_domain
    }

    /// How long one probe may take before it counts as failed.
    pub fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }

    /// Seconds between automatic rounds of probes.
    pub fn interval_secs(&self) -> u32 {
        self.interval_secs
    }
}

pub(crate) fn check_range(
    what: &'static str,
    value: u32,
    min: u32,
    max: u32,
) -> Result<(), SettingsError> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(SettingsError::OutOfRange { what, min, max })
    }
}

fn is_http_url(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"));
    rest.is_some_and(|rest| {
        !rest.is_empty() && !text.chars().any(|c| c.is_whitespace() || c.is_control())
    })
}

fn is_hostname(text: &str) -> bool {
    let name = text.strip_suffix('.').unwrap_or(text);
    !name.is_empty()
        && name.chars().count() <= MAX_DOMAIN_CHARS
        && name.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

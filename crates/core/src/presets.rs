//! Built-in, read-only catalog of ready-made DNS and proxy settings (ADR-0006).
//!
//! Presets are *templates*: turning one into a profile copies its values, after
//! which the profile belongs to the user and can be edited freely.

use crate::{
    DnsConfig, DnsProfile, Profiles, ProxyConfig, ProxyProfile, ProxyScheme, ValidationError,
};

/// How a DNS preset is grouped in the "Add DNS" dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DnsPresetGroup {
    /// Well-known general-purpose resolvers.
    Popular,
    /// Resolvers that block ads, trackers or malware.
    Filtering,
    /// Resolvers aimed at one country or region.
    Regional,
}

/// A ready-made DNS setting.
#[derive(Debug, Clone, Copy)]
pub struct DnsPreset {
    id: &'static str,
    name: &'static str,
    group: DnsPresetGroup,
    tag: Option<&'static str>,
    servers: &'static [&'static str],
}

impl DnsPreset {
    /// Stable catalog id (lowercase slug).
    pub fn id(&self) -> &'static str {
        self.id
    }

    /// Display name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Dialog group.
    pub fn group(&self) -> DnsPresetGroup {
        self.group
    }

    /// Short label such as "Blocks ads".
    pub fn tag(&self) -> Option<&'static str> {
        self.tag
    }

    /// The server addresses as written in the catalog, in priority order.
    pub fn servers(&self) -> &'static [&'static str] {
        self.servers
    }

    /// The validated settings this preset stands for.
    pub fn config(&self) -> Result<DnsConfig, ValidationError> {
        DnsConfig::from_strs(self.servers.iter().copied())
    }

    /// Copies the preset into a new profile with the caller's `id`.
    pub fn to_profile(&self, id: impl Into<String>) -> Result<DnsProfile, ValidationError> {
        DnsProfile::new(id, self.name, self.config()?)
    }

    /// Whether any saved DNS profile already has exactly these settings.
    pub fn is_saved_in(&self, profiles: &Profiles) -> bool {
        self.config().is_ok_and(|config| {
            profiles
                .dns_profiles()
                .iter()
                .any(|p| p.config() == &config)
        })
    }
}

/// A ready-made proxy setting: the default local port of a common client app.
#[derive(Debug, Clone, Copy)]
pub struct ProxyPreset {
    id: &'static str,
    name: &'static str,
    scheme: ProxyScheme,
    port: u16,
    tag: Option<&'static str>,
}

/// Every proxy preset points at this machine: they are local client apps.
const LOCALHOST: &str = "127.0.0.1";

impl ProxyPreset {
    /// Stable catalog id (lowercase slug).
    pub fn id(&self) -> &'static str {
        self.id
    }

    /// Display name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Short label such as "Default".
    pub fn tag(&self) -> Option<&'static str> {
        self.tag
    }

    /// The validated settings this preset stands for.
    pub fn config(&self) -> Result<ProxyConfig, ValidationError> {
        ProxyConfig::new(self.scheme, LOCALHOST, self.port)
    }

    /// Copies the preset into a new profile with the caller's `id`.
    pub fn to_profile(&self, id: impl Into<String>) -> Result<ProxyProfile, ValidationError> {
        ProxyProfile::new(id, self.name, self.config()?)
    }

    /// Whether any saved proxy profile already has exactly these settings.
    pub fn is_saved_in(&self, profiles: &Profiles) -> bool {
        self.config().is_ok_and(|config| {
            profiles
                .proxy_profiles()
                .iter()
                .any(|p| p.config() == &config)
        })
    }
}

const fn dns(
    id: &'static str,
    name: &'static str,
    group: DnsPresetGroup,
    tag: Option<&'static str>,
    servers: &'static [&'static str],
) -> DnsPreset {
    DnsPreset {
        id,
        name,
        group,
        tag,
        servers,
    }
}

const fn proxy(
    id: &'static str,
    name: &'static str,
    scheme: ProxyScheme,
    port: u16,
    tag: Option<&'static str>,
) -> ProxyPreset {
    ProxyPreset {
        id,
        name,
        scheme,
        port,
        tag,
    }
}

// Values: providers' public documentation / well-known app defaults.
// Re-verify before each release (CHECKLIST: Step 3b).
static DNS_PRESETS: [DnsPreset; 6] = [
    dns(
        "cloudflare",
        "Cloudflare",
        DnsPresetGroup::Popular,
        Some("Fast"),
        &["1.1.1.1", "1.0.0.1"],
    ),
    dns(
        "google",
        "Google",
        DnsPresetGroup::Popular,
        None,
        &["8.8.8.8", "8.8.4.4"],
    ),
    dns(
        "quad9",
        "Quad9",
        DnsPresetGroup::Popular,
        Some("Security"),
        &["9.9.9.9", "149.112.112.112"],
    ),
    dns(
        "opendns",
        "OpenDNS",
        DnsPresetGroup::Popular,
        None,
        &["208.67.222.222", "208.67.220.220"],
    ),
    dns(
        "adguard",
        "AdGuard",
        DnsPresetGroup::Filtering,
        Some("Blocks ads"),
        &["94.140.14.14", "94.140.15.15"],
    ),
    dns(
        "shecan",
        "Shecan",
        DnsPresetGroup::Regional,
        Some("Iran"),
        &["178.22.122.100", "185.51.200.2"],
    ),
];

static PROXY_PRESETS: [ProxyPreset; 7] = [
    proxy(
        "v2rayn-socks",
        "v2rayN · SOCKS",
        ProxyScheme::Socks5,
        10808,
        Some("Default"),
    ),
    proxy(
        "v2rayn-http",
        "v2rayN · HTTP",
        ProxyScheme::Http,
        10809,
        None,
    ),
    proxy("clash", "Clash / Mihomo", ProxyScheme::Http, 7890, None),
    proxy("clash-verge", "Clash Verge", ProxyScheme::Http, 7897, None),
    proxy(
        "shadowsocks",
        "Shadowsocks",
        ProxyScheme::Socks5,
        1080,
        None,
    ),
    proxy("tor", "Tor service", ProxyScheme::Socks5, 9050, None),
    proxy(
        "tor-browser",
        "Tor Browser",
        ProxyScheme::Socks5,
        9150,
        None,
    ),
];

/// All DNS presets, in display order.
pub fn dns_presets() -> &'static [DnsPreset] {
    &DNS_PRESETS
}

/// All proxy presets, in display order.
pub fn proxy_presets() -> &'static [ProxyPreset] {
    &PROXY_PRESETS
}

/// The DNS preset with this id.
pub fn dns_preset(id: &str) -> Option<&'static DnsPreset> {
    DNS_PRESETS.iter().find(|p| p.id == id)
}

/// The proxy preset with this id.
pub fn proxy_preset(id: &str) -> Option<&'static ProxyPreset> {
    PROXY_PRESETS.iter().find(|p| p.id == id)
}

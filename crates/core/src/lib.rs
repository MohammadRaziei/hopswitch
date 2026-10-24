//! `Hopswitch` core: platform-independent domain model and logic.
//!
//! This crate has no dependency on Tauri or on any operating system API,
//! which keeps it fast to compile and trivial to unit-test.

pub mod managed_block;
pub mod testing;

mod autopilot;
mod connectivity;
mod dns;
mod error;
mod network;
mod presets;
mod prober;
mod profile;
mod profiles;
mod proxy;
mod repository;
mod service;
mod session;

pub use autopilot::{AutoSettings, Autopilot, Decision, Health, Probe};
pub use connectivity::{ConnectivitySettings, SettingsError};
pub use dns::{DnsConfig, MAX_DNS_SERVERS};
pub use error::ValidationError;
pub use network::{CommandOutput, CommandRunner, DnsState, ProxyState, SystemError, SystemNetwork};
pub use presets::{
    dns_preset, dns_presets, proxy_preset, proxy_presets, DnsPreset, DnsPresetGroup, ProxyPreset,
};
pub use prober::{probe_dns_profiles, probe_proxies, Prober};
pub use profile::{DnsProfile, ProxyProfile, MAX_NAME_CHARS};
pub use profiles::{Profiles, ProfilesError};
pub use proxy::{ProxyConfig, ProxyScheme};
pub use repository::{ProfileRepository, RepositoryError};
pub use service::{
    restore_all, DnsKind, DnsService, Kind, NetworkService, ProxyKind, ProxyService,
    RestoreAllError, ServiceError,
};
pub use session::{Session, SessionStore, StoreError};

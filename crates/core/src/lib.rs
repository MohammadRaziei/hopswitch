//! `Hopswitch` core: platform-independent domain model and logic.
//!
//! This crate has no dependency on Tauri or on any operating system API,
//! which keeps it fast to compile and trivial to unit-test.

mod dns;
mod error;
mod profile;
mod proxy;

pub use dns::{DnsConfig, MAX_DNS_SERVERS};
pub use error::ValidationError;
pub use profile::{Profile, MAX_NAME_CHARS};
pub use proxy::{ProxyConfig, ProxyScheme};

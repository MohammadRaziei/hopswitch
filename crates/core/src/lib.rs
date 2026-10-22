//! `Hopswitch` core: platform-independent domain model and logic.
//!
//! This crate has no dependency on Tauri or on any operating system API,
//! which keeps it fast to compile and trivial to unit-test.

pub mod managed_block;
pub mod testing;

mod dns;
mod error;
mod network;
mod profile;
mod proxy;
mod service;
mod session;

pub use dns::{DnsConfig, MAX_DNS_SERVERS};
pub use error::ValidationError;
pub use network::{DnsState, ProxyState, SystemError, SystemNetwork};
pub use profile::{DnsProfile, ProxyProfile, MAX_NAME_CHARS};
pub use proxy::{ProxyConfig, ProxyScheme};
pub use service::{
    restore_all, DnsKind, DnsService, Kind, NetworkService, ProxyKind, ProxyService,
    RestoreAllError, ServiceError,
};
pub use session::{Session, SessionStore, StoreError};

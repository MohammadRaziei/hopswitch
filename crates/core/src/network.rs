//! The port through which `core` talks to the operating system.
//!
//! Adapters (`platform-*` crates) implement [`SystemNetwork`]; services depend
//! only on this trait and are tested against [`crate::testing::InMemoryNetwork`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DnsConfig, PacConfig, ProxyConfig};

/// What the system proxy looked like (or should look like).
///
/// Represents *what was really there*, not only what Hopswitch can create, so a
/// snapshot can always be restored faithfully.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyState {
    /// No system proxy settings at all: nothing is switched on and nothing is stored.
    ///
    /// Settings that are stored but switched off (an address Windows keeps after the proxy is
    /// turned off) are *not* this: they are `Other { enabled: false, .. }`, so restoring a
    /// snapshot brings them back too.
    Disabled,
    /// A manual proxy that Hopswitch understands.
    Manual(ProxyConfig),
    /// A PAC script and nothing else: no manual proxy in use or stored, no auto-detect.
    ///
    /// Adapters report this only when the setting is exactly this and no more, so that writing the
    /// state back reproduces the original. Anything that mixes a PAC script with other settings is
    /// [`ProxyState::Other`].
    Pac(PacConfig),
    /// Anything else (auto-detect, per-protocol proxies, a PAC script combined with other
    /// settings, ...).
    ///
    /// `raw` is an adapter-defined payload that the same adapter can write back
    /// to reproduce the original state exactly; core treats it as opaque.
    Other {
        /// Opaque, adapter-defined description of the foreign state.
        raw: String,
        /// Whether this state routes traffic (a PAC script, a proxy that is switched on).
        /// `false` means settings are stored but not in use. Sessions saved before this
        /// field existed read as `true`: assume it was in use.
        #[serde(default = "in_use_by_default")]
        enabled: bool,
    },
}

fn in_use_by_default() -> bool {
    true
}

/// What the system DNS looked like (or should look like).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnsState {
    /// DNS servers come from DHCP / the network (nothing set manually).
    Automatic,
    /// Manually configured servers that Hopswitch understands.
    Manual(DnsConfig),
    /// Anything else (e.g. more servers than we model, search domains, ...).
    ///
    /// `raw` is an adapter-defined payload that the same adapter can write back
    /// to reproduce the original state exactly; core treats it as opaque.
    Other {
        /// Opaque, adapter-defined description of the foreign state.
        raw: String,
    },
}

/// A failure talking to the operating system.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SystemError {
    /// The operation needs rights the process does not have.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// The platform or configuration does not support the operation.
    #[error("not supported on this system: {0}")]
    Unsupported(String),
    /// An external command or API call failed.
    #[error("`{command}` failed: {details}")]
    CommandFailed {
        /// The command or API that failed.
        command: String,
        /// Output or error text explaining the failure.
        details: String,
    },
}

/// Reads and writes the real system proxy and DNS settings.
///
/// Proxy and DNS are separate on purpose: touching one must never touch the
/// other. "Apply" and "restore" are the same primitive (`write_*`), so restoring
/// a snapshot takes exactly the code path that applying does.
pub trait SystemNetwork: Send + Sync {
    /// Reads the current system proxy state.
    fn read_proxy(&self) -> Result<ProxyState, SystemError>;
    /// Makes the system proxy match `state`.
    fn write_proxy(&self, state: &ProxyState) -> Result<(), SystemError>;
    /// Reads the current system DNS state.
    fn read_dns(&self) -> Result<DnsState, SystemError>;
    /// Makes the system DNS match `state`.
    fn write_dns(&self, state: &DnsState) -> Result<(), SystemError>;
}

/// What a finished external command printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Whether the command exited successfully.
    pub success: bool,
    /// Everything written to standard output.
    pub stdout: String,
    /// Everything written to standard error.
    pub stderr: String,
}

/// Runs external programs for OS adapters (`powershell`, `networksetup`, ...).
///
/// A port so adapter logic can be tested on any machine with
/// [`crate::testing::FakeRunner`]. `Err` means the program could not be run at
/// all; a program that ran and failed is `Ok` with `success == false`.
pub trait CommandRunner: Send + Sync {
    /// Runs `program` with `args` and waits for it to finish.
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, SystemError>;
}

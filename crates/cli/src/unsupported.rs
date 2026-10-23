//! The stand-in used where no adapter exists yet.

use hopswitch_core::{DnsState, ProxyState, SystemError, SystemNetwork};

/// A [`SystemNetwork`] whose every operation explains that this operating
/// system has no adapter yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedNetwork;

fn unsupported() -> SystemError {
    SystemError::Unsupported("no adapter for this operating system yet".to_owned())
}

impl SystemNetwork for UnsupportedNetwork {
    fn read_proxy(&self) -> Result<ProxyState, SystemError> {
        Err(unsupported())
    }

    fn write_proxy(&self, _state: &ProxyState) -> Result<(), SystemError> {
        Err(unsupported())
    }

    fn read_dns(&self) -> Result<DnsState, SystemError> {
        Err(unsupported())
    }

    fn write_dns(&self, _state: &DnsState) -> Result<(), SystemError> {
        Err(unsupported())
    }
}

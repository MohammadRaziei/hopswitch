//! The port through which `core` and the CLI ask for a running PAC server (ADR-0011 decision 6).
//!
//! Serving a script needs a socket, which `core` must not touch, so the code that applies a rules
//! profile only sees these traits. [`crate::pacserver::PacServerHost`] is the real implementation and
//! [`crate::core::testing::FakePacHost`] the test double.

use crate::core::{PacConfig, SystemError};

/// A PAC script that is being served. Dropping it stops serving.
pub trait PacEndpoint: Send {
    /// The address to give the system as its PAC script address.
    fn url(&self) -> PacConfig;
    /// Replaces the script later requests receive; the address stays the same.
    fn set_script(&self, script: &str);
}

/// Starts PAC servers.
pub trait PacHost: Send + Sync {
    /// Starts serving `script` and returns the running server.
    ///
    /// # Errors
    /// [`SystemError`] when nothing can be served (no free loopback port, no thread).
    fn start(&self, script: &str) -> Result<Box<dyn PacEndpoint>, SystemError>;
}

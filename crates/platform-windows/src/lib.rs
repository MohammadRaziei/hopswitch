//! Windows adapter: the system proxy through the registry and `WinINet`.
//!
//! The logic is split so it can be tested on any OS: [`registry`] is pure
//! (parse / plan / render), [`WindowsNetwork`] talks to `PowerShell` through the
//! `CommandRunner` port, and only [`ProcessRunner`] starts real processes.

mod network;
mod registry;
mod runner;

pub use network::WindowsNetwork;
pub use runner::ProcessRunner;

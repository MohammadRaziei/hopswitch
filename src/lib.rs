//! Hopswitch: switch the system proxy and DNS, with profiles, rule lists and automatic switching.
//!
//! One package, several modules. Who may use whom is the dependency graph of the crates this was
//! before (ADR-0013), and `tests/architecture.rs` keeps it that way:
//!
//! * [`core`]: the domain, with no operating-system access; it only defines ports.
//! * [`storage`]: files on disk (profiles, sessions, the auto settings); uses `core`.
//! * [`http`]: a tiny loopback HTTP server, std only; uses nothing else of this package.
//! * [`probe`]: real connectivity probes; uses `core`.
//! * [`platform_windows`]: the Windows adapter; uses `core` and `storage`.
//! * [`pacserver`]: serves a PAC script; uses `core` and `http`.
//! * [`cli`]: the command line and `serve`; uses all of them.

pub mod cli;
pub mod core;
pub mod http;
pub mod pacserver;
pub mod platform_windows;
pub mod probe;
pub mod storage;

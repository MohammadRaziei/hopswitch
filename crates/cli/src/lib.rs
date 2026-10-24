//! The `hopswitch` command line: parsing, and running commands against injected
//! dependencies so everything is testable without touching a real system.

mod app;
mod command;
mod dirs;
mod error;
mod unsupported;

pub use app::{run, Context};
pub use command::{parse, Command, DnsCommand, PingOptions, PingScope, ProxyCommand};
pub use dirs::{default_dir, Platform};
pub use error::CliError;
pub use unsupported::UnsupportedNetwork;

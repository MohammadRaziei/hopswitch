//! The `hopswitch` command line: parsing, and running commands against injected
//! dependencies so everything is testable without touching a real system.

mod app;
mod command;
mod dirs;
mod error;
mod launch;
mod rules;
mod serve;
mod unsupported;

pub use app::{run, usage, Context};
pub use command::{
    parse, AutoCommand, AutoOption, Command, DnsAutoCommand, DnsCommand, PingOptions, PingScope,
    ProxyCommand, RulesCommand, ServeOptions, DEFAULT_SERVE_PORT,
};
pub use dirs::{default_dir, Platform};
pub use error::CliError;
pub use launch::{decide_launch, no_gui_hint, Launch, NoGuiReason};
pub use rules::{start_rules, RulesRun};
pub use serve::{start_web, WebServer};
pub use unsupported::UnsupportedNetwork;

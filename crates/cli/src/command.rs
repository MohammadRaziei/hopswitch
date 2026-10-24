//! Parsing the command line into a [`Command`].

use crate::CliError;

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Show usage.
    Help,
    /// Show the proxy and DNS state.
    Status,
    /// List the built-in presets.
    Presets,
    /// Restore proxy and DNS to the original settings.
    Off,
    /// Test whether the saved profiles work, and how fast.
    Ping(PingOptions),
    /// A proxy subcommand.
    Proxy(ProxyCommand),
    /// A DNS subcommand.
    Dns(DnsCommand),
}

/// Which saved profiles `ping` tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingScope {
    /// Proxies and DNS.
    All,
    /// Only proxies.
    Proxy,
    /// Only DNS.
    Dns,
}

/// `hopswitch ping [proxy|dns] [--url U] [--domain D] [--timeout MS]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingOptions {
    /// What to test.
    pub scope: PingScope,
    /// Overrides the URL fetched through each proxy.
    pub url: Option<String>,
    /// Overrides the domain asked of each DNS server.
    pub domain: Option<String>,
    /// Overrides the per-probe timeout, in milliseconds.
    pub timeout_ms: Option<u32>,
}

/// `hopswitch proxy ...`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyCommand {
    /// List saved proxy profiles.
    List,
    /// Activate a saved profile or preset (by id or name).
    Use(String),
    /// Restore the original proxy.
    Off,
    /// Save a new profile.
    Add {
        /// Display name.
        name: String,
        /// `scheme://host:port`.
        spec: String,
        /// Hosts that skip the proxy.
        bypass: Vec<String>,
    },
    /// Delete a saved profile.
    Remove(String),
}

/// `hopswitch dns ...`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsCommand {
    /// List saved DNS profiles.
    List,
    /// Activate a saved profile or preset (by id or name).
    Use(String),
    /// Restore the original DNS.
    Off,
    /// Save a new profile.
    Add {
        /// Display name.
        name: String,
        /// Server addresses in priority order.
        servers: Vec<String>,
    },
    /// Delete a saved profile.
    Remove(String),
}

/// Parses the arguments after the program name.
pub fn parse(args: &[String]) -> Result<Command, CliError> {
    let Some((first, rest)) = args.split_first() else {
        return Ok(Command::Help);
    };
    match first.as_str() {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "status" => no_args(rest, Command::Status),
        "presets" => no_args(rest, Command::Presets),
        "off" => no_args(rest, Command::Off),
        "ping" => parse_ping(rest).map(Command::Ping),
        "proxy" => parse_proxy(rest).map(Command::Proxy),
        "dns" => parse_dns(rest).map(Command::Dns),
        other => Err(CliError::usage(format!(
            "unknown command `{other}`; try `hopswitch help`"
        ))),
    }
}

fn no_args<T>(rest: &[String], value: T) -> Result<T, CliError> {
    match rest.first() {
        None => Ok(value),
        Some(extra) => Err(CliError::usage(format!("unexpected argument `{extra}`"))),
    }
}

fn one_arg(rest: &[String], what: &str) -> Result<String, CliError> {
    match rest {
        [one] => Ok(one.clone()),
        [] => Err(CliError::usage(format!("missing {what}"))),
        [_, extra, ..] => Err(CliError::usage(format!("unexpected argument `{extra}`"))),
    }
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_proxy(rest: &[String]) -> Result<ProxyCommand, CliError> {
    let Some((sub, args)) = rest.split_first() else {
        return Err(CliError::usage(
            "expected: proxy list | use <name> | off | add <name> <url> | remove <name>",
        ));
    };
    match sub.as_str() {
        "list" => no_args(args, ProxyCommand::List),
        "off" => no_args(args, ProxyCommand::Off),
        "use" => one_arg(args, "profile or preset name").map(ProxyCommand::Use),
        "remove" => one_arg(args, "profile name").map(ProxyCommand::Remove),
        "add" => parse_proxy_add(args),
        other => Err(CliError::usage(format!("unknown proxy command `{other}`"))),
    }
}

fn parse_proxy_add(args: &[String]) -> Result<ProxyCommand, CliError> {
    const USAGE: &str = "usage: hopswitch proxy add <name> <scheme://host:port> [--bypass a,b]";
    let mut positional = Vec::new();
    let mut bypass = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--bypass" {
            let value = iter
                .next()
                .ok_or_else(|| CliError::usage("--bypass needs a comma-separated list"))?;
            bypass = split_list(value);
        } else if arg.starts_with("--") {
            return Err(CliError::usage(format!("unknown option `{arg}`")));
        } else {
            positional.push(arg.clone());
        }
    }
    let [name, spec] = <[String; 2]>::try_from(positional).map_err(|_| CliError::usage(USAGE))?;
    Ok(ProxyCommand::Add { name, spec, bypass })
}

fn parse_dns(rest: &[String]) -> Result<DnsCommand, CliError> {
    let Some((sub, args)) = rest.split_first() else {
        return Err(CliError::usage(
            "expected: dns list | use <name> | off | add <name> <ip,ip> | remove <name>",
        ));
    };
    match sub.as_str() {
        "list" => no_args(args, DnsCommand::List),
        "off" => no_args(args, DnsCommand::Off),
        "use" => one_arg(args, "profile or preset name").map(DnsCommand::Use),
        "remove" => one_arg(args, "profile name").map(DnsCommand::Remove),
        "add" => {
            let [name, servers] = <[String; 2]>::try_from(args.to_vec())
                .map_err(|_| CliError::usage("usage: hopswitch dns add <name> <ip[,ip...]>"))?;
            Ok(DnsCommand::Add {
                name,
                servers: split_list(&servers),
            })
        }
        other => Err(CliError::usage(format!("unknown dns command `{other}`"))),
    }
}

fn parse_ping(args: &[String]) -> Result<PingOptions, CliError> {
    let mut options = PingOptions {
        scope: PingScope::All,
        url: None,
        domain: None,
        timeout_ms: None,
    };
    let mut scope_given = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--url" => options.url = Some(option_value(&mut iter, "--url")?),
            "--domain" => options.domain = Some(option_value(&mut iter, "--domain")?),
            "--timeout" => {
                let value = option_value(&mut iter, "--timeout")?;
                options.timeout_ms = Some(value.parse().map_err(|_| {
                    CliError::usage(format!(
                        "--timeout needs a number of milliseconds, got `{value}`"
                    ))
                })?);
            }
            flag if flag.starts_with("--") => {
                return Err(CliError::usage(format!("unknown option `{flag}`")));
            }
            word => {
                if scope_given {
                    return Err(CliError::usage(format!("unexpected argument `{word}`")));
                }
                scope_given = true;
                options.scope = match word {
                    "proxy" => PingScope::Proxy,
                    "dns" => PingScope::Dns,
                    other => {
                        return Err(CliError::usage(format!(
                            "ping what? expected `proxy` or `dns`, got `{other}`"
                        )))
                    }
                };
            }
        }
    }
    Ok(options)
}

fn option_value<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<String, CliError> {
    iter.next()
        .cloned()
        .ok_or_else(|| CliError::usage(format!("{flag} needs a value")))
}

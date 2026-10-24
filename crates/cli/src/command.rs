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
    /// Automatic connect / switch settings and runner.
    Auto(AutoCommand),
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

/// Which kind of setting an `auto` command is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoTarget {
    /// The system proxy.
    Proxy,
    /// The system DNS.
    Dns,
}

/// One of the two automatic behaviours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoOption {
    /// Connect when something answers, disconnect when nothing does.
    Connect,
    /// Stay on the fastest profile of the pool.
    Switch,
}

/// `hopswitch auto ...`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoCommand {
    /// Show what is on, the pools and the test settings.
    Status,
    /// Change the connectivity test settings.
    Set {
        /// URL fetched through each proxy.
        url: Option<String>,
        /// Domain asked of each DNS server.
        domain: Option<String>,
        /// Per-probe timeout, in milliseconds.
        timeout_ms: Option<u32>,
        /// Seconds between rounds of `auto run`.
        interval_secs: Option<u32>,
    },
    /// Turn one option on or off for one kind.
    Toggle {
        /// Proxy or DNS.
        target: AutoTarget,
        /// Connect or switch.
        option: AutoOption,
        /// The new state.
        on: bool,
    },
    /// Add a saved profile to a pool.
    PoolAdd {
        /// Proxy or DNS.
        target: AutoTarget,
        /// Profile id or name.
        name: String,
    },
    /// Remove a profile from a pool.
    PoolRemove {
        /// Proxy or DNS.
        target: AutoTarget,
        /// Profile id or name.
        name: String,
    },
    /// Run the automatic modes (forever, or one round with `--once`).
    Run {
        /// Do a single round and exit.
        once: bool,
    },
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
        "auto" => parse_auto(rest).map(Command::Auto),
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

const AUTO_USAGE: &str = "expected: auto status | set [--url U] [--domain D] [--timeout MS] \
[--interval SECS] | run [--once] | <proxy|dns> connect|switch on|off | <proxy|dns> pool add|remove <name>";

fn parse_auto(args: &[String]) -> Result<AutoCommand, CliError> {
    let Some((sub, rest)) = args.split_first() else {
        return Err(CliError::usage(AUTO_USAGE));
    };
    match sub.as_str() {
        "status" => no_args(rest, AutoCommand::Status),
        "run" => match rest {
            [] => Ok(AutoCommand::Run { once: false }),
            [flag] if flag == "--once" => Ok(AutoCommand::Run { once: true }),
            [other, ..] => Err(CliError::usage(format!("unexpected argument `{other}`"))),
        },
        "set" => parse_auto_set(rest),
        "proxy" => parse_auto_kind(AutoTarget::Proxy, rest),
        "dns" => parse_auto_kind(AutoTarget::Dns, rest),
        other => Err(CliError::usage(format!(
            "unknown auto command `{other}`; {AUTO_USAGE}"
        ))),
    }
}

fn parse_auto_set(args: &[String]) -> Result<AutoCommand, CliError> {
    let (mut url, mut domain, mut timeout_ms, mut interval_secs) = (None, None, None, None);
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--url" => url = Some(option_value(&mut iter, "--url")?),
            "--domain" => domain = Some(option_value(&mut iter, "--domain")?),
            "--timeout" => timeout_ms = Some(number_option(&mut iter, "--timeout")?),
            "--interval" => interval_secs = Some(number_option(&mut iter, "--interval")?),
            other => return Err(CliError::usage(format!("unexpected argument `{other}`"))),
        }
    }
    if url.is_none() && domain.is_none() && timeout_ms.is_none() && interval_secs.is_none() {
        return Err(CliError::usage(
            "nothing to set; use --url, --domain, --timeout or --interval",
        ));
    }
    Ok(AutoCommand::Set {
        url,
        domain,
        timeout_ms,
        interval_secs,
    })
}

fn number_option<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<u32, CliError> {
    let value = option_value(iter, flag)?;
    value
        .parse()
        .map_err(|_| CliError::usage(format!("{flag} needs a whole number, got `{value}`")))
}

fn parse_auto_kind(target: AutoTarget, args: &[String]) -> Result<AutoCommand, CliError> {
    let texts: Vec<&str> = args.iter().map(String::as_str).collect();
    match texts.as_slice() {
        [option @ ("connect" | "switch"), state] => {
            let on = match *state {
                "on" => true,
                "off" => false,
                other => {
                    return Err(CliError::usage(format!(
                        "expected `on` or `off`, got `{other}`"
                    )))
                }
            };
            let option = if *option == "connect" {
                AutoOption::Connect
            } else {
                AutoOption::Switch
            };
            Ok(AutoCommand::Toggle { target, option, on })
        }
        ["pool", "add", name] => Ok(AutoCommand::PoolAdd {
            target,
            name: (*name).to_owned(),
        }),
        ["pool", "remove", name] => Ok(AutoCommand::PoolRemove {
            target,
            name: (*name).to_owned(),
        }),
        _ => Err(CliError::usage(AUTO_USAGE)),
    }
}

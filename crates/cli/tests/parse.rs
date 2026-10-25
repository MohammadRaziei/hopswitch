//! Tests for command-line parsing.

use hopswitch_cli::{
    parse, AutoCommand, AutoOption, CliError, Command, DnsAutoCommand, DnsCommand, PingOptions,
    PingScope, ProxyCommand,
};

fn p(args: &[&str]) -> Result<Command, CliError> {
    let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    parse(&owned)
}

fn is_usage(result: &Result<Command, CliError>) -> bool {
    matches!(*result, Err(CliError::Usage(_)))
}

#[test]
fn no_arguments_or_a_help_flag_shows_help() {
    for args in [&[][..], &["help"], &["--help"], &["-h"]] {
        assert_eq!(p(args).unwrap(), Command::Help, "{args:?}");
    }
}

#[test]
fn simple_commands() {
    assert_eq!(p(&["status"]).unwrap(), Command::Status);
    assert_eq!(p(&["presets"]).unwrap(), Command::Presets);
    assert_eq!(p(&["off"]).unwrap(), Command::Off);
}

#[test]
fn proxy_subcommands() {
    assert_eq!(
        p(&["proxy", "list"]).unwrap(),
        Command::Proxy(ProxyCommand::List)
    );
    assert_eq!(
        p(&["proxy", "off"]).unwrap(),
        Command::Proxy(ProxyCommand::Off)
    );
    assert_eq!(
        p(&["proxy", "use", "v2rayn-socks"]).unwrap(),
        Command::Proxy(ProxyCommand::Use("v2rayn-socks".into()))
    );
    assert_eq!(
        p(&["proxy", "remove", "work"]).unwrap(),
        Command::Proxy(ProxyCommand::Remove("work".into()))
    );
}

#[test]
fn proxy_add_with_and_without_a_bypass_list() {
    assert_eq!(
        p(&["proxy", "add", "Work", "socks5://10.0.0.5:1080"]).unwrap(),
        Command::Proxy(ProxyCommand::Add {
            name: "Work".into(),
            spec: "socks5://10.0.0.5:1080".into(),
            bypass: vec![],
        })
    );
    assert_eq!(
        p(&[
            "proxy",
            "add",
            "Work",
            "http://h:1",
            "--bypass",
            "localhost,*.local"
        ])
        .unwrap(),
        Command::Proxy(ProxyCommand::Add {
            name: "Work".into(),
            spec: "http://h:1".into(),
            bypass: vec!["localhost".into(), "*.local".into()],
        })
    );
}

#[test]
fn dns_subcommands() {
    assert_eq!(p(&["dns", "list"]).unwrap(), Command::Dns(DnsCommand::List));
    assert_eq!(p(&["dns", "off"]).unwrap(), Command::Dns(DnsCommand::Off));
    assert_eq!(
        p(&["dns", "use", "cloudflare"]).unwrap(),
        Command::Dns(DnsCommand::Use("cloudflare".into()))
    );
    assert_eq!(
        p(&["dns", "remove", "home"]).unwrap(),
        Command::Dns(DnsCommand::Remove("home".into()))
    );
    assert_eq!(
        p(&["dns", "add", "Home", "9.9.9.9,149.112.112.112"]).unwrap(),
        Command::Dns(DnsCommand::Add {
            name: "Home".into(),
            servers: vec!["9.9.9.9".into(), "149.112.112.112".into()],
        })
    );
}

#[test]
fn mistakes_are_usage_errors() {
    for args in [
        &["bogus"][..],
        &["status", "extra"],
        &["off", "now"],
        &["proxy"],
        &["proxy", "bogus"],
        &["proxy", "use"],
        &["proxy", "use", "a", "b"],
        &["proxy", "list", "x"],
        &["proxy", "add", "OnlyName"],
        &["proxy", "add", "N", "http://h:1", "--bypass"],
        &["proxy", "add", "N", "http://h:1", "--nope", "x"],
        &["dns"],
        &["dns", "add", "OnlyName"],
        &["dns", "remove"],
    ] {
        assert!(is_usage(&p(args)), "{args:?}");
    }
}

#[test]
fn usage_errors_exit_with_2_and_failures_with_1() {
    assert_eq!(CliError::Usage("x".into()).exit_code(), 2);
    assert_eq!(CliError::Failure("x".into()).exit_code(), 1);
}

// ---------- ping ----------

fn ping(
    scope: PingScope,
    url: Option<&str>,
    domain: Option<&str>,
    timeout_ms: Option<u32>,
) -> Command {
    Command::Ping(PingOptions {
        scope,
        url: url.map(str::to_owned),
        domain: domain.map(str::to_owned),
        timeout_ms,
    })
}

#[test]
fn ping_defaults_to_everything() {
    assert_eq!(
        p(&["ping"]).unwrap(),
        ping(PingScope::All, None, None, None)
    );
}

#[test]
fn ping_can_be_limited_to_one_kind() {
    assert_eq!(
        p(&["ping", "proxy"]).unwrap(),
        ping(PingScope::Proxy, None, None, None)
    );
    assert_eq!(
        p(&["ping", "dns"]).unwrap(),
        ping(PingScope::Dns, None, None, None)
    );
}

#[test]
fn ping_options_can_come_in_any_order() {
    let expected = ping(
        PingScope::Dns,
        Some("http://x.test/"),
        Some("x.test"),
        Some(2000),
    );

    assert_eq!(
        p(&[
            "ping",
            "--timeout",
            "2000",
            "dns",
            "--domain",
            "x.test",
            "--url",
            "http://x.test/"
        ])
        .unwrap(),
        expected
    );
}

#[test]
fn ping_rejects_malformed_input() {
    for args in [
        &["ping", "both"][..],
        &["ping", "proxy", "dns"],
        &["ping", "--timeout"],
        &["ping", "--timeout", "soon"],
        &["ping", "--timeout", "-5"],
        &["ping", "--url"],
        &["ping", "--color", "red"],
    ] {
        assert!(is_usage(&p(args)), "{args:?}");
    }
}

// ---------- groups ----------

#[test]
fn a_group_is_added_with_a_name_and_at_least_one_member() {
    assert_eq!(
        p(&["proxy", "add-group", "Smart", "a", "b"]).unwrap(),
        Command::Proxy(ProxyCommand::AddGroup {
            name: "Smart".into(),
            members: vec!["a".into(), "b".into()]
        })
    );
    assert!(is_usage(&p(&["proxy", "add-group", "Smart"])));
    assert!(is_usage(&p(&["proxy", "add-group"])));
}

// ---------- auto ----------

fn auto(args: &[&str]) -> Result<AutoCommand, CliError> {
    let mut full = vec!["auto"];
    full.extend_from_slice(args);
    match p(&full)? {
        Command::Auto(command) => Ok(command),
        other => panic!("{other:?}"),
    }
}

#[test]
fn auto_status_and_run() {
    assert_eq!(auto(&["status"]).unwrap(), AutoCommand::Status);
    assert_eq!(auto(&["run"]).unwrap(), AutoCommand::Run { once: false });
    assert_eq!(
        auto(&["run", "--once"]).unwrap(),
        AutoCommand::Run { once: true }
    );
}

#[test]
fn the_proxy_only_has_an_auto_connect_flag() {
    assert_eq!(
        auto(&["proxy", "connect", "on"]).unwrap(),
        AutoCommand::ProxyConnect { on: true }
    );
    assert_eq!(
        auto(&["proxy", "connect", "off"]).unwrap(),
        AutoCommand::ProxyConnect { on: false }
    );
}

#[test]
fn dns_keeps_its_options_and_pool() {
    assert_eq!(
        auto(&["dns", "connect", "on"]).unwrap(),
        AutoCommand::Dns(DnsAutoCommand::Toggle {
            option: AutoOption::Connect,
            on: true
        })
    );
    assert_eq!(
        auto(&["dns", "switch", "off"]).unwrap(),
        AutoCommand::Dns(DnsAutoCommand::Toggle {
            option: AutoOption::Switch,
            on: false
        })
    );
    assert_eq!(
        auto(&["dns", "pool", "add", "Cf"]).unwrap(),
        AutoCommand::Dns(DnsAutoCommand::PoolAdd("Cf".into()))
    );
    assert_eq!(
        auto(&["dns", "pool", "remove", "cf"]).unwrap(),
        AutoCommand::Dns(DnsAutoCommand::PoolRemove("cf".into()))
    );
}

#[test]
fn auto_set_takes_any_mix_of_options() {
    assert_eq!(
        auto(&[
            "set",
            "--interval",
            "10",
            "--url",
            "http://x.test/",
            "--domain",
            "x.test",
            "--timeout",
            "900"
        ])
        .unwrap(),
        AutoCommand::Set {
            url: Some("http://x.test/".into()),
            domain: Some("x.test".into()),
            timeout_ms: Some(900),
            interval_secs: Some(10),
        }
    );
}

#[test]
fn auto_rejects_malformed_input_and_the_removed_proxy_options() {
    for args in [
        &[][..],
        &["status", "now"],
        &["run", "--twice"],
        &["set"],
        &["set", "--timeout", "soon"],
        &["set", "--interval"],
        &["proxy"],
        &["proxy", "connect"],
        &["proxy", "connect", "maybe"],
        // the proxy follows the selected profile now: no switch, no pool
        &["proxy", "switch", "on"],
        &["proxy", "pool", "add", "x"],
        &["proxy", "pool", "remove", "x"],
        &["dns"],
        &["dns", "speed", "on"],
        &["dns", "pool"],
        &["dns", "pool", "add"],
        &["dns", "pool", "list", "x"],
        &["tor", "connect", "on"],
    ] {
        assert!(is_usage(&p(&[&["auto"][..], args].concat())), "{args:?}");
    }
}

#[test]
fn the_message_for_a_removed_proxy_option_points_to_the_new_way() {
    let message = match p(&["auto", "proxy", "switch", "on"]) {
        Err(CliError::Usage(message)) => message,
        other => panic!("{other:?}"),
    };

    assert!(message.contains("proxy use"), "{message}");
}

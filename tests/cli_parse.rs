//! Tests for command-line parsing.

use hopswitch::cli::{
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

// ---- serve (ADR-0012 step b) ----

use hopswitch::cli::ServeOptions;

fn serve_options(args: &[&str]) -> ServeOptions {
    match p(args).unwrap() {
        Command::Serve(options) => options,
        other => panic!("not serve: {other:?}"),
    }
}

#[test]
fn serve_defaults_to_loopback_port_47831_with_auto_connect_on() {
    let options = serve_options(&["serve"]);
    assert_eq!(options.ip.to_string(), "127.0.0.1");
    assert_eq!(options.port, 47831);
    assert!(options.auto);
}

#[test]
fn serve_takes_an_ip_a_port_and_can_turn_auto_connect_off() {
    let options = serve_options(&["serve", "--ip", "::1", "--port", "0", "--no-auto"]);
    assert_eq!(options.ip.to_string(), "::1");
    assert_eq!(options.port, 0);
    assert!(!options.auto);
    assert_eq!(
        serve_options(&["serve", "--ip", "127.0.0.2"])
            .ip
            .to_string(),
        "127.0.0.2"
    );
}

#[test]
fn serve_refuses_an_address_that_is_not_loopback_and_points_to_ssh() {
    for ip in ["0.0.0.0", "192.168.1.10", "::"] {
        match p(&["serve", "--ip", ip]) {
            Err(CliError::Usage(message)) => {
                assert!(message.contains("ssh -L"), "{ip}: {message}");
            }
            other => panic!("{ip}: {other:?}"),
        }
    }
}

#[test]
fn serve_has_no_allow_remote_option_and_rejects_bad_values() {
    for args in [
        &["serve", "--allow-remote"][..],
        &["serve", "--ip"],
        &["serve", "--ip", "not-an-ip"],
        &["serve", "--port"],
        &["serve", "--port", "70000"],
        &["serve", "--port", "abc"],
        &["serve", "extra"],
    ] {
        assert!(is_usage(&p(args)), "{args:?}");
    }
}

// ---------- rules ----------

mod rules {
    use hopswitch::cli::RulesCommand;

    use super::*;

    fn rules(args: &[&str]) -> Result<Command, CliError> {
        let mut all = vec!["rules"];
        all.extend_from_slice(args);
        p(&all)
    }

    #[test]
    fn the_profile_commands() {
        assert_eq!(
            rules(&["list"]).unwrap(),
            Command::Rules(RulesCommand::List)
        );
        assert_eq!(
            rules(&["show", "work"]).unwrap(),
            Command::Rules(RulesCommand::Show("work".into()))
        );
        assert_eq!(
            rules(&["use", "work"]).unwrap(),
            Command::Rules(RulesCommand::Use("work".into()))
        );
        assert_eq!(
            rules(&["remove", "work"]).unwrap(),
            Command::Rules(RulesCommand::Remove("work".into()))
        );
    }

    #[test]
    fn add_takes_a_name_and_an_optional_default() {
        assert_eq!(
            rules(&["add", "Work"]).unwrap(),
            Command::Rules(RulesCommand::Add {
                name: "Work".into(),
                default: None
            })
        );
        assert_eq!(
            rules(&["add", "Work", "--default", "corp"]).unwrap(),
            Command::Rules(RulesCommand::Add {
                name: "Work".into(),
                default: Some("corp".into())
            })
        );
        assert!(is_usage(&rules(&["add"])));
        assert!(is_usage(&rules(&["add", "Work", "--default"])));
        assert!(is_usage(&rules(&["add", "Work", "--bogus", "x"])));
    }

    #[test]
    fn the_default_and_the_rules_inside_a_profile() {
        assert_eq!(
            rules(&["default", "work", "direct"]).unwrap(),
            Command::Rules(RulesCommand::SetDefault {
                profile: "work".into(),
                target: "direct".into()
            })
        );
        assert_eq!(
            rules(&["rule", "add", "work", "example.com", "corp"]).unwrap(),
            Command::Rules(RulesCommand::AddRule {
                profile: "work".into(),
                domain: "example.com".into(),
                target: "corp".into()
            })
        );
        assert_eq!(
            rules(&["rule", "remove", "work", "example.com"]).unwrap(),
            Command::Rules(RulesCommand::RemoveRule {
                profile: "work".into(),
                domain: "example.com".into()
            })
        );
    }

    #[test]
    fn missing_or_extra_words_are_usage_errors() {
        for args in [
            &[][..],
            &["frobnicate"],
            &["show"],
            &["use"],
            &["use", "a", "b"],
            &["list", "extra"],
            &["default", "work"],
            &["rule"],
            &["rule", "add", "work", "example.com"],
            &["rule", "remove", "work"],
            &["rule", "move", "work", "a", "b"],
        ] {
            assert!(is_usage(&rules(args)), "{args:?}");
        }
    }
}

//! Tests for command-line parsing.

use hopswitch_cli::{parse, CliError, Command, DnsCommand, ProxyCommand};

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

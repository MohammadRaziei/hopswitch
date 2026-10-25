//! Behavioural tests for the Windows proxy adapter, driven by a fake command runner.

use std::sync::Arc;

use base64::Engine;
use hopswitch_core::testing::{FakeRunner, RecordedCall};
use hopswitch_core::{
    DnsConfig, DnsState, ProxyConfig, ProxyScheme, ProxyState, SystemError, SystemNetwork,
};
use hopswitch_platform_windows::WindowsNetwork;
use serde_json::json;

fn setup() -> (Arc<FakeRunner>, WindowsNetwork) {
    let runner = Arc::new(FakeRunner::new());
    let net = WindowsNetwork::new(runner.clone());
    (runner, net)
}

/// Registry JSON as the read script prints it.
fn registry(
    enable: Option<u32>,
    server: Option<&str>,
    over: Option<&str>,
    pac: Option<&str>,
    detect: Option<u32>,
) -> String {
    json!({
        "ProxyEnable": enable,
        "ProxyServer": server,
        "ProxyOverride": over,
        "AutoConfigURL": pac,
        "AutoDetect": detect,
    })
    .to_string()
}

fn read_with(stdout: String) -> Result<ProxyState, SystemError> {
    let (runner, net) = setup();
    runner.push_stdout(stdout);
    net.read_proxy()
}

fn cfg(scheme: ProxyScheme, host: &str, port: u16) -> ProxyConfig {
    ProxyConfig::new(scheme, host, port).unwrap()
}

/// The `PowerShell` script carried by a call (decoded from `-EncodedCommand`).
fn script_of(call: &RecordedCall) -> String {
    let position = call
        .args
        .iter()
        .position(|a| a == "-EncodedCommand")
        .expect("the script is passed with -EncodedCommand");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&call.args[position + 1])
        .expect("valid base64");
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).expect("valid UTF-16")
}

/// The `PowerShell` script of the most recent command.
fn last_script(runner: &FakeRunner) -> String {
    script_of(runner.calls().last().expect("a command was run"))
}

// ---------- reading ----------

#[test]
fn a_disabled_proxy_reads_as_disabled() {
    let state = read_with(registry(Some(0), None, None, None, Some(1))).unwrap();

    assert_eq!(
        state,
        ProxyState::Disabled,
        "auto-detect alone is not a proxy"
    );
}

#[test]
fn a_registry_without_any_proxy_values_reads_as_disabled() {
    let state = read_with(registry(None, None, None, None, None)).unwrap();

    assert_eq!(state, ProxyState::Disabled);
}

#[test]
fn a_leftover_server_with_the_proxy_switched_off_is_an_inactive_foreign_state() {
    // Windows keeps the address when the proxy is switched off; losing it would not be a faithful restore.
    let state = read_with(registry(Some(0), Some("127.0.0.1:1"), None, None, None)).unwrap();

    assert!(
        matches!(state, ProxyState::Other { enabled: false, .. }),
        "{state:?}"
    );
}

#[test]
fn a_leftover_bypass_list_alone_counts_too() {
    let state = read_with(registry(Some(0), None, Some("<local>"), None, None)).unwrap();

    assert!(
        matches!(state, ProxyState::Other { enabled: false, .. }),
        "{state:?}"
    );
}

#[test]
fn empty_strings_are_not_leftovers() {
    for enable in [None, Some(0)] {
        let state = read_with(registry(enable, Some(""), Some(""), None, None)).unwrap();

        assert_eq!(state, ProxyState::Disabled, "ProxyEnable = {enable:?}");
    }
}

#[test]
fn an_inactive_foreign_state_is_restored_value_for_value() {
    let (runner, net) = setup();
    runner.push_stdout(registry(
        Some(0),
        Some("corp:3128"),
        Some("<local>"),
        None,
        None,
    ));
    let before = net.read_proxy().unwrap();

    net.write_proxy(&before).unwrap();

    let script = last_script(&runner);
    assert!(
        script.contains("-Name 'ProxyServer' -Value 'corp:3128'"),
        "{script}"
    );
    assert!(
        script.contains("-Name 'ProxyOverride' -Value '<local>'"),
        "{script}"
    );
    assert!(script.contains("-Name 'ProxyEnable' -Value 0"), "{script}");
}

#[test]
fn a_plain_host_and_port_is_an_http_proxy() {
    let state = read_with(registry(Some(1), Some("127.0.0.1:10809"), None, None, None)).unwrap();

    assert_eq!(
        state,
        ProxyState::Manual(cfg(ProxyScheme::Http, "127.0.0.1", 10809))
    );
}

#[test]
fn a_socks_entry_is_a_socks5_proxy() {
    let state = read_with(registry(
        Some(1),
        Some("socks=127.0.0.1:10808"),
        None,
        None,
        None,
    ))
    .unwrap();

    assert_eq!(
        state,
        ProxyState::Manual(cfg(ProxyScheme::Socks5, "127.0.0.1", 10808))
    );
}

#[test]
fn an_ipv6_host_is_read_without_brackets() {
    let state = read_with(registry(Some(1), Some("[::1]:1080"), None, None, None)).unwrap();

    assert_eq!(
        state,
        ProxyState::Manual(cfg(ProxyScheme::Http, "::1", 1080))
    );
}

#[test]
fn the_bypass_list_is_split_trimmed_and_cleaned() {
    let state = read_with(registry(
        Some(1),
        Some("proxy.local:3128"),
        Some("localhost; 127.*;;<local>"),
        None,
        None,
    ))
    .unwrap();

    let expected = cfg(ProxyScheme::Http, "proxy.local", 3128)
        .with_bypass(vec!["localhost".into(), "127.*".into(), "<local>".into()])
        .unwrap();
    assert_eq!(state, ProxyState::Manual(expected));
}

#[test]
fn a_pac_script_is_a_foreign_state() {
    let state = read_with(registry(
        Some(0),
        None,
        None,
        Some("http://wpad/p.pac"),
        Some(1),
    ))
    .unwrap();

    assert!(
        matches!(state, ProxyState::Other { enabled: true, .. }),
        "{state:?}"
    );
}

#[test]
fn per_protocol_proxies_are_a_foreign_state() {
    let state = read_with(registry(
        Some(1),
        Some("http=a.example:1;https=b.example:2"),
        None,
        None,
        None,
    ))
    .unwrap();

    assert!(
        matches!(state, ProxyState::Other { enabled: true, .. }),
        "{state:?}"
    );
}

#[test]
fn an_enabled_proxy_without_a_server_is_a_foreign_state() {
    let state = read_with(registry(Some(1), None, None, None, None)).unwrap();

    assert!(
        matches!(state, ProxyState::Other { enabled: true, .. }),
        "{state:?}"
    );
}

#[test]
fn unparseable_output_is_reported_not_guessed() {
    let err = read_with("not json at all".into()).unwrap_err();

    assert!(matches!(err, SystemError::CommandFailed { .. }), "{err:?}");
}

#[test]
fn a_failing_script_reports_its_stderr() {
    let (runner, net) = setup();
    runner.push_failure("something broke");

    let err = net.read_proxy().unwrap_err();

    assert!(
        matches!(&err, SystemError::CommandFailed { details, .. } if details.contains("something broke")),
        "{err:?}"
    );
}

#[test]
fn access_denied_is_reported_as_a_permission_problem() {
    let (runner, net) = setup();
    runner.push_failure("Set-ItemProperty : Access is denied");

    assert!(matches!(
        net.read_proxy().unwrap_err(),
        SystemError::PermissionDenied(_)
    ));
}

#[test]
fn a_runner_that_cannot_start_the_program_propagates_its_error() {
    let (runner, net) = setup();
    runner.push_error(SystemError::Unsupported("powershell not found".into()));

    assert_eq!(
        net.read_proxy().unwrap_err(),
        SystemError::Unsupported("powershell not found".into())
    );
}

#[test]
fn powershell_is_started_non_interactively_with_an_encoded_script() {
    let (runner, net) = setup();
    runner.push_stdout(registry(Some(0), None, None, None, None));
    net.read_proxy().unwrap();
    net.write_proxy(&ProxyState::Manual(cfg(ProxyScheme::Http, "h", 1)))
        .unwrap();

    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    for call in &calls {
        assert_eq!(call.program, "powershell");
        for flag in ["-NoProfile", "-NonInteractive", "-EncodedCommand"] {
            assert!(call.args.iter().any(|a| a == flag), "missing {flag}");
        }
        assert_ne!(script_of(call), "");
    }
}

#[test]
fn the_read_script_prints_utf8_so_non_ascii_values_survive() {
    let (runner, net) = setup();
    runner.push_stdout(registry(Some(0), None, None, None, None));

    net.read_proxy().unwrap();

    assert!(last_script(&runner).contains("OutputEncoding"));
}

// ---------- writing ----------

fn write(state: &ProxyState) -> String {
    let (runner, net) = setup();
    net.write_proxy(state).unwrap();
    last_script(&runner)
}

#[test]
fn an_http_proxy_is_written_as_host_and_port() {
    let script = write(&ProxyState::Manual(cfg(
        ProxyScheme::Http,
        "127.0.0.1",
        10809,
    )));

    assert!(
        script.contains("-Name 'ProxyServer' -Value '127.0.0.1:10809'"),
        "{script}"
    );
    assert!(script.contains("-Name 'ProxyEnable' -Value 1"), "{script}");
}

#[test]
fn a_socks5_proxy_is_written_with_the_socks_prefix() {
    let script = write(&ProxyState::Manual(cfg(
        ProxyScheme::Socks5,
        "127.0.0.1",
        10808,
    )));

    assert!(
        script.contains("-Value 'socks=127.0.0.1:10808'"),
        "{script}"
    );
}

#[test]
fn an_ipv6_host_is_written_in_brackets() {
    let script = write(&ProxyState::Manual(cfg(ProxyScheme::Http, "::1", 1080)));

    assert!(script.contains("-Value '[::1]:1080'"), "{script}");
}

#[test]
fn the_server_is_written_before_the_proxy_is_enabled() {
    let script = write(&ProxyState::Manual(cfg(ProxyScheme::Http, "h", 1)));

    let server = script.find("'ProxyServer'").unwrap();
    let enable = script.find("'ProxyEnable'").unwrap();
    assert!(server < enable, "{script}");
}

#[test]
fn a_manual_proxy_replaces_any_pac_script() {
    let script = write(&ProxyState::Manual(cfg(ProxyScheme::Http, "h", 1)));

    assert!(
        script.contains("Remove-ItemProperty") && script.contains("'AutoConfigURL'"),
        "{script}"
    );
}

#[test]
fn the_bypass_list_is_joined_with_semicolons_or_removed_when_empty() {
    let with = cfg(ProxyScheme::Http, "h", 1)
        .with_bypass(vec!["localhost".into(), "*.local".into()])
        .unwrap();
    let script = write(&ProxyState::Manual(with));
    assert!(
        script.contains("-Name 'ProxyOverride' -Value 'localhost;*.local'"),
        "{script}"
    );

    let script = write(&ProxyState::Manual(cfg(ProxyScheme::Http, "h", 1)));
    assert!(
        script.contains("Remove-ItemProperty") && script.contains("'ProxyOverride'"),
        "{script}"
    );
    assert!(
        !script.contains("Set-ItemProperty -Path $k -Name 'ProxyOverride'"),
        "{script}"
    );
}

#[test]
fn single_quotes_in_values_are_escaped() {
    let with = cfg(ProxyScheme::Http, "h", 1)
        .with_bypass(vec!["o'brien".into()])
        .unwrap();

    let script = write(&ProxyState::Manual(with));

    assert!(script.contains("o''brien"), "{script}");
}

#[test]
fn schemes_the_registry_cannot_express_are_refused_without_touching_anything() {
    for scheme in [ProxyScheme::Https, ProxyScheme::Socks4] {
        let (runner, net) = setup();

        let err = net
            .write_proxy(&ProxyState::Manual(cfg(scheme, "h", 1)))
            .unwrap_err();

        assert!(
            matches!(err, SystemError::Unsupported(_)),
            "{scheme}: {err:?}"
        );
        assert_eq!(runner.calls(), Vec::<RecordedCall>::new());
    }
}

#[test]
fn bypass_entries_that_would_corrupt_the_list_are_refused() {
    for bad in ["a;b", "line\nbreak", "tab\there"] {
        let (runner, net) = setup();
        let with = cfg(ProxyScheme::Http, "h", 1)
            .with_bypass(vec![bad.into()])
            .unwrap();

        let err = net.write_proxy(&ProxyState::Manual(with)).unwrap_err();

        assert!(
            matches!(err, SystemError::Unsupported(_)),
            "{bad:?}: {err:?}"
        );
        assert_eq!(runner.calls(), Vec::<RecordedCall>::new());
    }
}

#[test]
fn disabling_leaves_nothing_of_ours_behind() {
    // `Disabled` means "no proxy settings at all"; an address that was there before is a different
    // state (an inactive foreign state) and is restored on its own.
    let script = write(&ProxyState::Disabled);

    assert!(script.contains("-Name 'ProxyEnable' -Value 0"), "{script}");
    for name in ["AutoConfigURL", "ProxyServer", "ProxyOverride"] {
        assert!(
            script.contains(&format!("Remove-ItemProperty -Path $k -Name '{name}'")),
            "{name} should be removed: {script}"
        );
    }
}

#[test]
fn every_write_tells_wininet_to_reload_the_settings() {
    for state in [
        ProxyState::Disabled,
        ProxyState::Manual(cfg(ProxyScheme::Http, "h", 1)),
    ] {
        let script = write(&state);
        assert!(script.contains("InternetSetOption"), "{script}");
        assert!(script.contains("try"), "refresh is best effort: {script}");
    }
}

#[test]
fn a_foreign_state_read_earlier_is_restored_value_for_value() {
    let (runner, net) = setup();
    runner.push_stdout(registry(
        Some(1),
        Some("a.example:1;https=b.example:2"),
        None,
        Some("http://wpad/p.pac"),
        Some(1),
    ));
    let foreign = net.read_proxy().unwrap();

    net.write_proxy(&foreign).unwrap();

    let script = last_script(&runner);
    assert!(
        script.contains("-Name 'ProxyServer' -Value 'a.example:1;https=b.example:2'"),
        "{script}"
    );
    assert!(
        script.contains("-Name 'AutoConfigURL' -Value 'http://wpad/p.pac'"),
        "{script}"
    );
    assert!(script.contains("-Name 'AutoDetect' -Value 1"), "{script}");
    assert!(script.contains("-Name 'ProxyEnable' -Value 1"), "{script}");
    assert!(
        script.contains("Remove-ItemProperty") && script.contains("'ProxyOverride'"),
        "values that were absent are removed: {script}"
    );
}

#[test]
fn a_foreign_payload_from_somewhere_else_is_refused() {
    let (runner, net) = setup();

    let err = net
        .write_proxy(&ProxyState::Other {
            raw: "gsettings:whatever".into(),
            enabled: true,
        })
        .unwrap_err();

    assert!(matches!(err, SystemError::Unsupported(_)), "{err:?}");
    assert_eq!(runner.calls(), Vec::<RecordedCall>::new());
}

// ---------- DNS (needs administrator rights; arrives with the elevated helper) ----------

#[test]
fn dns_is_explicitly_not_supported_yet() {
    let (runner, net) = setup();

    assert!(matches!(
        net.read_dns().unwrap_err(),
        SystemError::Unsupported(_)
    ));
    assert!(matches!(
        net.write_dns(&DnsState::Manual(
            DnsConfig::from_strs(["1.1.1.1"]).unwrap()
        ))
        .unwrap_err(),
        SystemError::Unsupported(_)
    ));
    assert_eq!(runner.calls(), Vec::<RecordedCall>::new());
}

#[test]
fn quotes_of_every_kind_survive_inside_values() {
    let with = cfg(ProxyScheme::Http, "h", 1)
        .with_bypass(vec!["say\"hi\"".into(), "curly\u{2019}quote".into()])
        .unwrap();

    let script = write(&ProxyState::Manual(with));

    assert!(script.contains("say\"hi\""), "{script}");
    // PowerShell treats typographic single quotes like ASCII ones, so they are doubled too.
    assert!(script.contains("curly\u{2019}\u{2019}quote"), "{script}");
}

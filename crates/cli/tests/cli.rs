//! End-to-end behaviour of the CLI against an in-memory system and a temp directory.
//! Every `run` call builds fresh services, exactly like a new process would.

use std::fs;
use std::sync::Arc;

use hopswitch_cli::{run, CliError, Context, UnsupportedNetwork};
use hopswitch_core::testing::{FakeProber, InMemoryNetwork};
use hopswitch_core::{
    DnsState, Health, ProxyConfig, ProxyScheme, ProxyState, SystemError, SystemNetwork,
};

struct Env {
    dir: tempfile::TempDir,
    net: Arc<InMemoryNetwork>,
    prober: Arc<FakeProber>,
}

fn users_proxy() -> ProxyState {
    ProxyState::Manual(ProxyConfig::new(ProxyScheme::Http, "corp.example", 3128).unwrap())
}

fn env() -> Env {
    env_with(FakeProber::new())
}

fn env_with(prober: FakeProber) -> Env {
    Env {
        dir: tempfile::tempdir().unwrap(),
        net: Arc::new(InMemoryNetwork::new().with_proxy(users_proxy())),
        prober: Arc::new(prober),
    }
}

impl Env {
    fn ctx(&self) -> Context {
        Context {
            dir: self.dir.path().join("hopswitch"),
            network: self.net.clone(),
            prober: self.prober.clone(),
        }
    }

    /// Runs one command; returns its result and everything it printed.
    fn run(&self, args: &[&str]) -> (Result<(), CliError>, String) {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        let mut out = Vec::new();
        let result = run(&owned, &self.ctx(), &mut out);
        (result, String::from_utf8(out).unwrap())
    }

    fn ok(&self, args: &[&str]) -> String {
        let (result, out) = self.run(args);
        result.unwrap_or_else(|e| panic!("{args:?} failed: {e}\n{out}"));
        out
    }

    fn fails(&self, args: &[&str]) -> String {
        let (result, _) = self.run(args);
        match result {
            Err(CliError::Failure(message)) => message,
            other => panic!("{args:?} should fail, got {other:?}"),
        }
    }

    fn file(&self, name: &str) -> std::path::PathBuf {
        self.dir.path().join("hopswitch").join(name)
    }
}

fn socks(host: &str, port: u16) -> ProxyState {
    ProxyState::Manual(ProxyConfig::new(ProxyScheme::Socks5, host, port).unwrap())
}

// ---------- using and restoring ----------

#[test]
fn using_a_preset_applies_it_and_remembers_it() {
    let e = env();

    let out = e.ok(&["proxy", "use", "v2rayn-socks"]);

    assert_eq!(e.net.read_proxy().unwrap(), socks("127.0.0.1", 10808));
    assert!(out.contains("v2rayN"), "{out}");
    assert!(e.file("profiles.json").exists());
    assert!(e.file("session-proxy.json").exists());
    assert!(e.ok(&["proxy", "list"]).contains("v2rayn-socks"));
}

#[test]
fn turning_it_off_in_a_later_run_restores_the_original() {
    let e = env();
    e.ok(&["proxy", "use", "v2rayn-socks"]);

    let out = e.ok(&["proxy", "off"]);

    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
    assert!(!e.file("session-proxy.json").exists());
    assert!(out.to_lowercase().contains("restored"), "{out}");
}

#[test]
fn switching_keeps_the_very_first_original() {
    let e = env();
    e.ok(&["proxy", "use", "v2rayn-socks"]);
    e.ok(&["proxy", "use", "clash"]);
    assert_eq!(
        e.net.read_proxy().unwrap(),
        ProxyState::Manual(ProxyConfig::new(ProxyScheme::Http, "127.0.0.1", 7890).unwrap())
    );

    e.ok(&["proxy", "off"]);

    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
}

#[test]
fn a_saved_profile_can_be_used_by_id_or_by_name_ignoring_case() {
    let e = env();
    e.ok(&["proxy", "add", "My Work", "socks5://10.0.0.5:1080"]);

    e.ok(&["proxy", "use", "my-work"]);
    assert_eq!(e.net.read_proxy().unwrap(), socks("10.0.0.5", 1080));
    e.ok(&["proxy", "off"]);

    e.ok(&["proxy", "use", "MY WORK"]);
    assert_eq!(e.net.read_proxy().unwrap(), socks("10.0.0.5", 1080));
}

#[test]
fn using_a_preset_twice_does_not_save_it_twice() {
    let e = env();
    e.ok(&["proxy", "use", "clash"]);
    e.ok(&["proxy", "off"]);
    e.ok(&["proxy", "use", "clash"]);

    let list = e.ok(&["proxy", "list"]);

    assert_eq!(list.matches("Clash / Mihomo").count(), 1, "{list}");
}

#[test]
fn an_unknown_name_changes_nothing_and_points_to_the_lists() {
    let e = env();

    let message = e.fails(&["proxy", "use", "nope"]);

    assert!(message.contains("nope"), "{message}");
    assert!(message.contains("presets"), "{message}");
    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
    assert_eq!(e.net.proxy_write_count(), 0);
}

#[test]
fn a_failed_apply_leaves_no_profile_and_no_session() {
    let e = env();
    e.net
        .fail_next_proxy_writes(1, SystemError::PermissionDenied("denied".into()));

    let message = e.fails(&["proxy", "use", "v2rayn-socks"]);

    assert!(message.contains("denied"), "{message}");
    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
    assert!(!e.file("session-proxy.json").exists());
    assert!(!e.file("profiles.json").exists());
}

#[test]
fn off_with_nothing_active_is_a_friendly_no_op() {
    let e = env();

    let out = e.ok(&["off"]);

    assert!(out.to_lowercase().contains("nothing"), "{out}");
    assert_eq!(e.net.proxy_write_count(), 0);
    assert_eq!(e.net.dns_write_count(), 0);
}

#[test]
fn off_restores_proxy_and_dns_together() {
    let e = env();
    e.ok(&["proxy", "use", "v2rayn-socks"]);
    e.ok(&["dns", "use", "cloudflare"]);
    assert!(matches!(e.net.read_dns().unwrap(), DnsState::Manual(_)));

    e.ok(&["off"]);

    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
    assert_eq!(e.net.read_dns().unwrap(), DnsState::Automatic);
}

#[test]
fn dns_off_never_touches_the_proxy() {
    let e = env();
    e.ok(&["proxy", "use", "v2rayn-socks"]);
    e.ok(&["dns", "use", "google"]);
    let proxy_writes = e.net.proxy_write_count();

    e.ok(&["dns", "off"]);

    assert_eq!(e.net.proxy_write_count(), proxy_writes);
    assert_eq!(e.net.read_proxy().unwrap(), socks("127.0.0.1", 10808));
}

// ---------- managing profiles ----------

#[test]
fn adding_a_proxy_saves_it_with_a_readable_id() {
    let e = env();

    let out = e.ok(&[
        "proxy",
        "add",
        "My Proxy!",
        "socks5://10.0.0.5:1080",
        "--bypass",
        "localhost,*.local",
    ]);

    assert!(out.contains("my-proxy"), "{out}");
    let saved = fs::read_to_string(e.file("profiles.json")).unwrap();
    assert!(saved.contains("*.local"), "{saved}");
}

#[test]
fn ids_stay_unique_and_non_latin_names_work() {
    let e = env();
    e.ok(&["proxy", "add", "Work", "http://h:1"]);
    let second = e.ok(&["proxy", "add", "Work", "http://h:2"]);
    assert!(second.contains("work-2"), "{second}");

    let persian = e.ok(&["proxy", "add", "محل کار", "http://h:3"]);
    assert!(persian.contains("محل-کار"), "{persian}");

    let symbols = e.ok(&["proxy", "add", "!!!", "http://h:4"]);
    assert!(symbols.contains("`profile`"), "{symbols}");
}

#[test]
fn invalid_input_is_reported_clearly_and_saves_nothing() {
    let e = env();

    assert!(e
        .fails(&["proxy", "add", "Bad", "not-a-url"])
        .to_lowercase()
        .contains("url"));
    assert!(e
        .fails(&["proxy", "add", "Bad", "http://h:0"])
        .to_lowercase()
        .contains("port"));
    assert!(!e.fails(&["dns", "add", "Bad", "1.2.3"]).is_empty());
    assert!(!e.file("profiles.json").exists());
}

#[test]
fn dns_profiles_can_be_added_listed_used_and_removed() {
    let e = env();
    e.ok(&["dns", "add", "Home", "9.9.9.9,149.112.112.112"]);
    assert!(e.ok(&["dns", "list"]).contains("9.9.9.9"));

    e.ok(&["dns", "use", "home"]);
    assert!(matches!(e.net.read_dns().unwrap(), DnsState::Manual(_)));
    e.ok(&["dns", "off"]);

    e.ok(&["dns", "remove", "home"]);
    assert!(!e.ok(&["dns", "list"]).contains("9.9.9.9"));
}

#[test]
fn removing_an_active_profile_is_refused_and_removing_an_inactive_one_works() {
    let e = env();
    e.ok(&["proxy", "add", "Work", "http://h:1"]);
    e.ok(&["proxy", "use", "work"]);

    let message = e.fails(&["proxy", "remove", "work"]);
    assert!(message.contains("off"), "{message}");

    e.ok(&["proxy", "off"]);
    e.ok(&["proxy", "remove", "work"]);
    assert!(!e.ok(&["proxy", "list"]).contains("Work"));
    assert!(!e.fails(&["proxy", "remove", "work"]).is_empty());
}

#[test]
fn an_empty_list_points_to_presets() {
    let e = env();

    assert!(e.ok(&["proxy", "list"]).contains("presets"));
    assert!(e.ok(&["dns", "list"]).contains("presets"));
}

#[test]
fn the_active_profile_is_marked_in_the_list() {
    let e = env();
    e.ok(&["proxy", "use", "clash"]);

    let list = e.ok(&["proxy", "list"]);

    assert!(
        list.lines()
            .any(|l| l.starts_with('*') && l.contains("clash")),
        "{list}"
    );
}

// ---------- presets and status ----------

#[test]
fn presets_are_listed_and_saved_ones_are_marked() {
    let e = env();
    let before = e.ok(&["presets"]);
    assert!(
        before.contains("cloudflare") && before.contains("1.1.1.1"),
        "{before}"
    );
    assert!(
        before.contains("v2rayn-socks") && before.contains("socks5://127.0.0.1:10808"),
        "{before}"
    );
    assert!(!before.contains("[saved]"));

    e.ok(&["dns", "use", "cloudflare"]);
    let after = e.ok(&["presets"]);

    assert!(
        after
            .lines()
            .any(|l| l.contains("cloudflare") && l.contains("[saved]")),
        "{after}"
    );
}

#[test]
fn status_describes_both_settings() {
    let e = env();
    let before = e.ok(&["status"]);
    assert!(
        before.contains("proxy:") && before.contains("dns:"),
        "{before}"
    );
    assert!(
        before.contains("corp.example"),
        "an external proxy is shown: {before}"
    );

    e.ok(&["proxy", "use", "v2rayn-socks"]);
    let after = e.ok(&["status"]);

    assert!(
        after.contains("active") && after.contains("v2rayN"),
        "{after}"
    );
}

#[test]
fn status_survives_a_setting_that_cannot_be_read() {
    let e = env();
    e.net
        .fail_next_dns_read(SystemError::Unsupported("no dns here".into()));

    let out = e.ok(&["status"]);

    assert!(
        out.contains("unavailable") && out.contains("no dns here"),
        "{out}"
    );
}

#[test]
fn help_lists_the_commands() {
    let out = env().ok(&["help"]);

    for word in ["status", "proxy use", "dns use", "off", "presets"] {
        assert!(out.contains(word), "help should mention `{word}`");
    }
}

// ---------- damaged files ----------

#[test]
fn a_corrupt_profile_file_is_reported_and_left_untouched() {
    let e = env();
    fs::create_dir_all(e.file("")).unwrap();
    fs::write(e.file("profiles.json"), b"{ broken").unwrap();

    let message = e.fails(&["proxy", "list"]);

    assert!(message.to_lowercase().contains("corrupt"), "{message}");
    assert_eq!(fs::read(e.file("profiles.json")).unwrap(), b"{ broken");
}

#[test]
fn off_still_works_when_the_profile_file_is_corrupt() {
    let e = env();
    e.ok(&["proxy", "use", "v2rayn-socks"]);
    fs::write(e.file("profiles.json"), b"{ broken").unwrap();

    e.ok(&["off"]);

    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
}

#[test]
fn a_corrupt_session_is_an_error_that_keeps_the_file() {
    let e = env();
    fs::create_dir_all(e.file("")).unwrap();
    fs::write(e.file("session-proxy.json"), b"{ broken").unwrap();

    let message = e.fails(&["off"]);

    assert!(message.contains("session-proxy.json"), "{message}");
    assert_eq!(fs::read(e.file("session-proxy.json")).unwrap(), b"{ broken");
}

// ---------- unsupported systems ----------

#[test]
fn a_system_without_an_adapter_explains_itself() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context {
        dir: dir.path().to_path_buf(),
        network: Arc::new(UnsupportedNetwork),
        prober: Arc::new(FakeProber::new()),
    };
    let mut out = Vec::new();

    let result = run(
        &["proxy".to_owned(), "use".to_owned(), "clash".to_owned()],
        &ctx,
        &mut out,
    );

    match result {
        Err(CliError::Failure(message)) => {
            assert!(message.contains("not supported"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

// ---------- ping ----------

fn alive(ms: u32) -> Health {
    Health::Alive { latency_ms: ms }
}

/// The line of `out` that mentions `needle`.
fn line_with<'a>(out: &'a str, needle: &str) -> &'a str {
    out.lines()
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no line with {needle:?} in:\n{out}"))
}

fn saved_pair() -> Env {
    let e = env_with(
        FakeProber::new()
            .with_proxy("socks5://127.0.0.1:1080", alive(42))
            .with_dns("1.1.1.1", alive(18)),
    );
    e.ok(&["proxy", "add", "Fast", "socks5://127.0.0.1:1080"]);
    e.ok(&["proxy", "add", "Down", "http://127.0.0.1:8080"]);
    e.ok(&["dns", "add", "Cf", "1.1.1.1"]);
    e
}

#[test]
fn ping_shows_latency_or_unreachable_for_every_saved_profile() {
    let e = saved_pair();

    let out = e.ok(&["ping"]);

    assert!(line_with(&out, "fast").contains("42 ms"), "{out}");
    assert!(line_with(&out, "down").contains("unreachable"), "{out}");
    assert!(line_with(&out, "cf").contains("18 ms"), "{out}");
}

#[test]
fn ping_marks_the_active_profile() {
    let e = saved_pair();
    e.ok(&["proxy", "use", "fast"]);

    let out = e.ok(&["ping"]);

    assert!(line_with(&out, "fast").starts_with('*'), "{out}");
    assert!(line_with(&out, "down").starts_with(' '), "{out}");
}

#[test]
fn ping_can_be_limited_to_proxies_or_dns() {
    let e = saved_pair();

    e.ok(&["ping", "proxy"]);
    assert!(e.prober.calls().iter().all(|c| c.starts_with("proxy ")));

    let e = saved_pair();
    e.ok(&["ping", "dns"]);
    assert!(e.prober.calls().iter().all(|c| c.starts_with("dns ")));
}

#[test]
fn ping_options_choose_the_target_domain_and_timeout() {
    let e = saved_pair();

    e.ok(&[
        "ping",
        "--url",
        "https://probe.test/up",
        "--domain",
        "probe.test",
        "--timeout",
        "1500",
    ]);

    let calls = e.prober.calls();
    assert!(
        calls.iter().any(|c| c.contains("proxy")
            && c.contains("https://probe.test/up")
            && c.contains("1500ms")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.contains("dns") && c.contains("probe.test") && c.contains("1500ms")),
        "{calls:?}"
    );
}

#[test]
fn ping_by_default_uses_the_documented_test_targets() {
    let e = saved_pair();

    e.ok(&["ping"]);

    let calls = e.prober.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.contains("http://cp.cloudflare.com/generate_204") && c.contains("3000ms")),
        "{calls:?}"
    );
    assert!(calls.iter().any(|c| c.contains("example.com")), "{calls:?}");
}

#[test]
fn ping_rejects_bad_settings_before_testing_anything() {
    let e = saved_pair();

    for args in [
        ["ping", "--timeout", "100"],
        ["ping", "--url", "not-a-url"],
        ["ping", "--domain", "bad domain"],
    ] {
        e.fails(&args);
    }
    assert!(e.prober.calls().is_empty());
}

#[test]
fn ping_with_nothing_saved_points_to_the_presets() {
    let e = env();

    let out = e.ok(&["ping"]);

    assert!(out.contains("presets"), "{out}");
    assert!(e.prober.calls().is_empty());
}

#[test]
fn ping_changes_nothing_on_the_system_or_on_disk() {
    let e = saved_pair();
    let before = fs::read(e.file("profiles.json")).unwrap();
    let writes = (e.net.proxy_write_count(), e.net.dns_write_count());

    e.ok(&["ping"]);

    assert_eq!(fs::read(e.file("profiles.json")).unwrap(), before);
    assert_eq!((e.net.proxy_write_count(), e.net.dns_write_count()), writes);
    assert!(!e.file("session-proxy.json").exists());
}

#[test]
fn ping_reports_a_damaged_profiles_file() {
    let e = env();
    fs::create_dir_all(e.dir.path().join("hopswitch")).unwrap();
    fs::write(e.file("profiles.json"), b"{ broken").unwrap();

    let message = e.fails(&["ping"]);

    assert!(message.contains("corrupt"), "{message}");
}

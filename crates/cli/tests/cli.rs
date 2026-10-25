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

// ---------- groups ----------

fn world() -> Env {
    let e = env_with(
        FakeProber::new()
            .with_proxy("socks5://127.0.0.1:1080", alive(42))
            .with_proxy("socks5://127.0.0.1:2080", alive(300))
            .with_dns("1.1.1.1", alive(18)),
    );
    e.ok(&["proxy", "add", "Fast", "socks5://127.0.0.1:1080"]);
    e.ok(&["proxy", "add", "Down", "http://127.0.0.1:8080"]);
    e.ok(&["proxy", "add", "Other", "socks5://127.0.0.1:2080"]);
    e.ok(&["dns", "add", "Cf", "1.1.1.1"]);
    e
}

fn proxy_now(e: &Env) -> ProxyState {
    e.net.read_proxy().unwrap()
}

fn fast_proxy() -> ProxyState {
    ProxyState::Manual(ProxyConfig::new(ProxyScheme::Socks5, "127.0.0.1", 1080).unwrap())
}

fn with_group() -> Env {
    let e = world();
    e.ok(&["proxy", "add-group", "Smart", "down", "other", "fast"]);
    e
}

#[test]
fn a_group_is_saved_and_listed_with_its_members_in_order() {
    let e = world();

    let out = e.ok(&["proxy", "add-group", "Smart", "Other", "fast"]);
    let list = e.ok(&["proxy", "list"]);

    assert!(out.contains("smart"), "{out}");
    let line = line_with(&list, "Smart");
    assert!(
        line.contains("fastest") && line.contains("other, fast"),
        "{list}"
    );
    assert!(
        list.contains("socks5://127.0.0.1:1080"),
        "plain proxies are still listed: {list}"
    );
}

#[test]
fn a_group_can_only_contain_saved_plain_proxies() {
    let e = with_group();

    assert!(e
        .fails(&["proxy", "add-group", "G", "ghost"])
        .contains("ghost"));
    assert!(
        e.fails(&["proxy", "add-group", "G", "fast", "Smart"])
            .contains("Smart"),
        "no group inside a group"
    );
    assert!(
        e.fails(&["proxy", "add-group", "G", "fast", "FAST"])
            .contains("fast"),
        "no repeated member"
    );
    assert_eq!(
        e.ok(&["proxy", "list"]).matches("fastest").count(),
        1,
        "nothing half-saved"
    );
}

#[test]
fn group_ids_never_collide_with_proxy_ids() {
    let e = world();

    assert!(e
        .ok(&["proxy", "add-group", "Fast", "other"])
        .contains("fast-2"));
    assert!(e
        .ok(&["proxy", "add-group", "Fast", "other"])
        .contains("fast-3"));
}

#[test]
fn a_proxy_in_use_by_a_group_cannot_be_removed() {
    let e = with_group();

    let message = e.fails(&["proxy", "remove", "other"]);

    assert!(message.contains("smart"), "{message}");
    e.ok(&["proxy", "remove", "smart"]);
    e.ok(&["proxy", "remove", "other"]);
}

#[test]
fn using_a_group_applies_its_fastest_live_member() {
    let e = with_group();

    let out = e.ok(&["proxy", "use", "smart"]);

    assert_eq!(
        proxy_now(&e),
        fast_proxy(),
        "down is dead, fast beats other"
    );
    assert!(
        out.contains("Fast") && out.contains("Smart") && out.contains("42 ms"),
        "{out}"
    );
    assert!(e.file("session-proxy.json").exists());
}

#[test]
fn using_a_group_whose_members_are_all_dead_changes_nothing() {
    let e = world();
    e.ok(&["proxy", "add-group", "Dead", "down"]);

    let message = e.fails(&["proxy", "use", "dead"]);

    assert!(message.contains("Dead"), "{message}");
    assert_eq!(proxy_now(&e), users_proxy());
    assert!(!e.file("session-proxy.json").exists());
    assert!(
        !e.ok(&["auto", "status"]).contains("dead"),
        "the selection is untouched"
    );
}

#[test]
fn using_a_profile_selects_it_for_auto_connect() {
    let e = with_group();
    assert!(e.ok(&["auto", "status"]).contains("none selected"));

    e.ok(&["proxy", "use", "fast"]);
    assert!(line_with(&e.ok(&["auto", "status"]), "proxy:").contains("profile: fast"));

    e.ok(&["proxy", "use", "smart"]);
    assert!(line_with(&e.ok(&["auto", "status"]), "proxy:").contains("profile: smart"));
}

#[test]
fn status_and_list_show_which_group_the_active_proxy_came_from() {
    let e = with_group();
    e.ok(&["proxy", "use", "smart"]);

    let status = e.ok(&["status"]);
    let list = e.ok(&["proxy", "list"]);

    assert!(
        line_with(&status, "proxy:").contains("Fast")
            && line_with(&status, "proxy:").contains("via Smart"),
        "{status}"
    );
    assert!(line_with(&list, "smart").starts_with('*'), "{list}");
    assert!(line_with(&list, "fast").starts_with('*'), "{list}");
}

#[test]
fn removing_the_selected_profile_clears_the_selection() {
    let e = with_group();
    e.ok(&["proxy", "use", "smart"]);
    e.ok(&["proxy", "off"]);

    e.ok(&["proxy", "remove", "smart"]);

    assert!(e.ok(&["auto", "status"]).contains("none selected"));
}

#[test]
fn ping_shows_a_groups_best_member() {
    let e = with_group();
    e.ok(&["proxy", "add-group", "Dead", "down"]);

    let out = e.ok(&["ping", "proxy"]);

    let smart = line_with(&out, "smart");
    assert!(smart.contains("Fast") && smart.contains("42 ms"), "{out}");
    assert!(line_with(&out, "dead").contains("unreachable"), "{out}");
}

// ---------- auto settings ----------

#[test]
fn auto_is_off_and_empty_until_configured() {
    let e = world();

    let out = e.ok(&["auto", "status"]);

    let proxy = line_with(&out, "proxy:");
    assert!(
        proxy.contains("connect off") && proxy.contains("none selected"),
        "{out}"
    );
    let dns = line_with(&out, "dns:");
    assert!(
        dns.contains("connect off") && dns.contains("switch off"),
        "{out}"
    );
    assert!(out.contains("(empty)"), "{out}");
    assert!(
        out.contains("generate_204") && out.contains("3000 ms") && out.contains("30 s"),
        "{out}"
    );
}

#[test]
fn proxy_and_dns_options_are_saved_independently() {
    let e = world();

    e.ok(&["auto", "proxy", "connect", "on"]);
    e.ok(&["auto", "dns", "switch", "on"]);
    let out = e.ok(&["auto", "status"]);

    assert!(line_with(&out, "proxy:").contains("connect on"), "{out}");
    let dns = line_with(&out, "dns:");
    assert!(
        dns.contains("connect off") && dns.contains("switch on"),
        "{out}"
    );

    e.ok(&["auto", "proxy", "connect", "off"]);
    assert!(line_with(&e.ok(&["auto", "status"]), "proxy:").contains("connect off"));
}

#[test]
fn turning_auto_connect_on_without_a_selection_says_what_is_missing() {
    let e = world();

    let out = e.ok(&["auto", "proxy", "connect", "on"]);

    assert!(out.contains("proxy use"), "{out}");
}

#[test]
fn the_dns_pool_takes_profiles_by_name_or_id_in_order_without_duplicates() {
    let e = world();
    e.ok(&["dns", "add", "Quad", "9.9.9.9"]);

    e.ok(&["auto", "dns", "pool", "add", "Quad"]);
    e.ok(&["auto", "dns", "pool", "add", "cf"]);
    let again = e.ok(&["auto", "dns", "pool", "add", "quad"]);
    let out = e.ok(&["auto", "status"]);

    assert!(again.contains("already"), "{again}");
    assert!(out.contains("quad, cf"), "{out}");

    e.ok(&["auto", "dns", "pool", "remove", "Quad"]);
    assert!(!e.ok(&["auto", "status"]).contains("quad"));
    assert!(e
        .fails(&["auto", "dns", "pool", "remove", "quad"])
        .contains("quad"));
}

#[test]
fn only_saved_dns_profiles_can_join_the_dns_pool() {
    let e = world();

    assert!(e
        .fails(&["auto", "dns", "pool", "add", "ghost"])
        .contains("ghost"));
    assert!(
        e.fails(&["auto", "dns", "pool", "add", "Fast"])
            .contains("Fast"),
        "a proxy is not a DNS profile"
    );
}

#[test]
fn removing_a_dns_profile_also_removes_it_from_the_pool() {
    let e = world();
    e.ok(&["auto", "dns", "pool", "add", "cf"]);

    e.ok(&["dns", "remove", "cf"]);

    assert!(e.ok(&["auto", "status"]).contains("(empty)"));
}

#[test]
fn test_settings_are_saved_and_used_by_ping_unless_overridden() {
    let e = world();

    e.ok(&[
        "auto",
        "set",
        "--url",
        "https://probe.test/up",
        "--domain",
        "probe.test",
        "--timeout",
        "1500",
        "--interval",
        "10",
    ]);
    let status = e.ok(&["auto", "status"]);
    assert!(
        status.contains("https://probe.test/up")
            && status.contains("probe.test")
            && status.contains("1500 ms")
            && status.contains("10 s"),
        "{status}"
    );

    e.ok(&["ping"]);
    assert!(
        e.prober.calls().iter().all(|c| c.contains("1500ms")),
        "{:?}",
        e.prober.calls()
    );

    e.ok(&["ping", "proxy", "--timeout", "800"]);
    assert!(e.prober.calls().iter().any(|c| c.contains("800ms")));
}

#[test]
fn invalid_test_settings_are_refused_and_nothing_is_saved() {
    let e = world();

    for args in [
        ["auto", "set", "--timeout", "100"],
        ["auto", "set", "--interval", "1"],
        ["auto", "set", "--url", "nope"],
    ] {
        e.fails(&args);
    }

    assert!(!e.file("auto.json").exists());
}

#[test]
fn a_damaged_auto_file_is_reported_but_off_still_works() {
    let e = world();
    e.ok(&["proxy", "use", "fast"]);
    fs::write(e.file("auto.json"), b"{ broken").unwrap();

    let message = e.fails(&["auto", "status"]);

    assert!(
        message.contains("corrupt") && message.contains("auto.json"),
        "{message}"
    );
    e.ok(&["off"]);
    assert_eq!(proxy_now(&e), users_proxy());
    assert_eq!(
        fs::read(e.file("auto.json")).unwrap(),
        b"{ broken",
        "left untouched"
    );
}

// ---------- a manual disconnect wins over automation ----------

#[test]
fn disconnecting_by_hand_turns_auto_connect_off_and_says_so() {
    let e = world();
    e.ok(&["proxy", "use", "fast"]);
    e.ok(&["auto", "proxy", "connect", "on"]);

    let out = e.ok(&["proxy", "off"]);

    assert!(out.to_lowercase().contains("auto connect"), "{out}");
    let status = e.ok(&["auto", "status"]);
    assert!(
        line_with(&status, "proxy:").contains("connect off"),
        "{status}"
    );
    assert!(
        line_with(&status, "proxy:").contains("profile: fast"),
        "the selection is kept: {status}"
    );
}

#[test]
fn off_does_the_same_for_both_kinds_but_only_for_what_was_active() {
    let e = world();
    e.ok(&["proxy", "use", "fast"]);
    e.ok(&["auto", "proxy", "connect", "on"]);
    e.ok(&["auto", "dns", "connect", "on"]); // DNS was never connected

    let out = e.ok(&["off"]);

    let status = e.ok(&["auto", "status"]);
    assert!(
        line_with(&status, "proxy:").contains("connect off"),
        "{status}"
    );
    assert!(
        line_with(&status, "dns:").contains("connect on"),
        "{status}"
    );
    assert!(out.to_lowercase().contains("proxy"), "{out}");
}

#[test]
fn dns_off_turns_dns_auto_connect_off() {
    let e = world();
    e.ok(&["dns", "use", "cf"]);
    e.ok(&["auto", "dns", "connect", "on"]);

    e.ok(&["dns", "off"]);

    assert!(line_with(&e.ok(&["auto", "status"]), "dns:").contains("connect off"));
}

#[test]
fn off_with_nothing_active_leaves_the_flags_alone() {
    let e = world();
    e.ok(&["auto", "proxy", "connect", "on"]);

    e.ok(&["proxy", "off"]);

    assert!(line_with(&e.ok(&["auto", "status"]), "proxy:").contains("connect on"));
}

// ---------- auto run --once ----------

/// Selects `selected` for auto connect and leaves the system disconnected with the flag on.
fn auto_proxy(e: &Env, selected: &str) {
    e.ok(&["proxy", "use", selected]);
    e.ok(&["proxy", "off"]);
    e.ok(&["auto", "proxy", "connect", "on"]);
}

#[test]
fn run_once_connects_the_selected_proxy() {
    let e = world();
    auto_proxy(&e, "fast");
    assert_eq!(proxy_now(&e), users_proxy());

    let out = e.ok(&["auto", "run", "--once"]);

    assert_eq!(proxy_now(&e), fast_proxy());
    assert!(out.contains("Fast") && out.contains("42 ms"), "{out}");
}

#[test]
fn run_once_connects_a_selected_group_to_its_fastest_member() {
    let e = with_group();
    auto_proxy(&e, "smart");

    let out = e.ok(&["auto", "run", "--once"]);

    assert_eq!(proxy_now(&e), fast_proxy());
    assert!(out.contains("Smart"), "{out}");
}

#[test]
fn run_once_disconnects_at_once_when_the_active_proxy_stops_answering() {
    let e = world();
    e.ok(&["proxy", "use", "down"]);
    e.ok(&["auto", "proxy", "connect", "on"]);

    let out = e.ok(&["auto", "run", "--once"]);

    assert_eq!(proxy_now(&e), users_proxy());
    assert!(out.to_lowercase().contains("restored"), "{out}");
    assert!(!e.file("session-proxy.json").exists());
}

#[test]
fn run_once_with_nothing_to_automate_sends_no_traffic() {
    let e = world();

    let out = e.ok(&["auto", "run", "--once"]);

    assert!(out.contains("off"), "{out}");
    assert!(e.prober.calls().is_empty());
    assert_eq!(proxy_now(&e), users_proxy());
}

#[test]
fn run_once_with_the_flag_on_but_nothing_selected_does_nothing() {
    let e = world();
    e.ok(&["auto", "proxy", "connect", "on"]);

    let out = e.ok(&["auto", "run", "--once"]);

    assert!(out.contains("off") || out.contains("nothing"), "{out}");
    assert!(e.prober.calls().is_empty());
}

#[test]
fn run_once_reports_a_failing_kind_without_blocking_the_other() {
    let e = world();
    auto_proxy(&e, "fast");
    e.ok(&["auto", "dns", "connect", "on"]);
    e.ok(&["auto", "dns", "pool", "add", "cf"]);
    e.net
        .fail_next_dns_writes(1, SystemError::PermissionDenied("admin needed".into()));

    let (result, out) = e.run(&["auto", "run", "--once"]);

    assert_eq!(proxy_now(&e), fast_proxy());
    assert!(line_with(&out, "dns").contains("admin needed"), "{out}");
    assert!(matches!(result, Err(CliError::Failure(_))), "{result:?}");
}

#[test]
fn run_once_survives_a_selection_that_no_longer_exists() {
    let e = env();
    e.ok(&["auto", "proxy", "connect", "on"]);
    fs::write(
        e.file("auto.json"),
        r#"{"version":2,"proxy":{"connect":true,"selected":"ghost"}}"#,
    )
    .unwrap();

    let out = e.ok(&["auto", "run", "--once"]);

    assert!(
        out.contains("Nothing to change") || out.contains("off"),
        "{out}"
    );
}

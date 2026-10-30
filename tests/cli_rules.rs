//! `hopswitch rules ...`: managing rules profiles, and `rules use`, which applies one as a PAC
//! script served by this process (ADR-0011). Against an in-memory system, a fake PAC host and a
//! scripted prober; every `run` builds fresh services, like a new process.

use std::sync::Arc;

use hopswitch::cli::{run, start_rules, CliError, Context};
use hopswitch::core::testing::{FakePacHost, FakeProber, InMemoryNetwork};
use hopswitch::core::{
    Health, PacConfig, ProxyConfig, ProxyScheme, ProxyState, SystemError, SystemNetwork,
};

struct Env {
    dir: tempfile::TempDir,
    net: Arc<InMemoryNetwork>,
    host: FakePacHost,
    prober: Arc<FakeProber>,
}

fn users_proxy() -> ProxyState {
    ProxyState::Manual(ProxyConfig::new(ProxyScheme::Http, "corp.example", 3128).unwrap())
}

fn alive(latency_ms: u32) -> Health {
    Health::Alive { latency_ms }
}

fn env() -> Env {
    env_with(FakeProber::new())
}

fn env_with(prober: FakeProber) -> Env {
    Env {
        dir: tempfile::tempdir().unwrap(),
        net: Arc::new(InMemoryNetwork::new().with_proxy(users_proxy())),
        host: FakePacHost::with_port(47123),
        prober: Arc::new(prober),
    }
}

impl Env {
    fn ctx(&self) -> Context {
        Context {
            dir: self.dir.path().join("hopswitch"),
            network: self.net.clone(),
            prober: self.prober.clone(),
            pac: Arc::new(self.host.clone()),
        }
    }

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

    /// Saves an HTTP proxy `corp` and a rules profile `work` (default `corp`, `bank.ir` direct).
    fn with_work_profile(&self) {
        self.ok(&["proxy", "add", "corp", "http://corp.test:3128"]);
        self.ok(&["rules", "add", "Work", "--default", "corp"]);
        self.ok(&["rules", "rule", "add", "work", "bank.ir", "direct"]);
    }
}

fn pac(port: u16) -> ProxyState {
    ProxyState::Pac(PacConfig::loopback(port))
}

// ---------- managing profiles ----------

#[test]
fn a_new_profile_is_listed_and_shown_with_its_rules_in_order() {
    let e = env();
    e.ok(&["proxy", "add", "corp", "http://corp.test:3128"]);

    e.ok(&["rules", "add", "Work", "--default", "corp"]);
    e.ok(&["rules", "rule", "add", "work", "bank.ir", "direct"]);
    e.ok(&["rules", "rule", "add", "work", "Dev.Example.com", "corp"]);

    let list = e.ok(&["rules", "list"]);
    assert!(list.contains("work"), "{list}");
    assert!(list.contains("2 rules"), "{list}");
    let shown = e.ok(&["rules", "show", "work"]);
    let bank = shown.find("bank.ir").expect(&shown);
    let dev = shown.find("dev.example.com").expect(&shown);
    assert!(bank < dev, "{shown}");
    assert!(shown.contains("default"), "{shown}");
    assert!(shown.contains("corp"), "{shown}");
}

#[test]
fn one_rule_is_not_called_rules() {
    let e = env();
    e.ok(&["rules", "add", "Work"]);
    e.ok(&["rules", "rule", "add", "work", "example.com", "direct"]);

    assert!(e.ok(&["rules", "list"]).contains("1 rule,"));
}

#[test]
fn an_empty_list_says_how_to_start() {
    let out = env().ok(&["rules", "list"]);

    assert!(out.contains("rules add"), "{out}");
}

#[test]
fn the_default_target_is_direct_unless_told_otherwise() {
    let e = env();

    e.ok(&["rules", "add", "Plain"]);

    let shown = e.ok(&["rules", "show", "plain"]);
    assert!(
        shown.contains("default") && shown.contains("direct"),
        "{shown}"
    );
}

#[test]
fn profiles_are_found_by_id_or_by_name_ignoring_case() {
    let e = env();
    e.ok(&["rules", "add", "My Work"]);

    assert!(e.ok(&["rules", "show", "my-work"]).contains("My Work"));
    assert!(e.ok(&["rules", "show", "MY WORK"]).contains("My Work"));
}

#[test]
fn a_target_that_is_not_saved_is_refused_with_a_hint() {
    let e = env();
    e.ok(&["rules", "add", "Work"]);

    let message = e.fails(&["rules", "rule", "add", "work", "example.com", "ghost"]);

    assert!(message.contains("ghost"), "{message}");
    assert!(message.contains("direct"), "{message}");
    assert!(e
        .fails(&["rules", "add", "Other", "--default", "ghost"])
        .contains("ghost"));
}

#[test]
fn a_second_rule_for_the_same_domain_is_refused() {
    let e = env();
    e.ok(&["rules", "add", "Work"]);
    e.ok(&["rules", "rule", "add", "work", "example.com", "direct"]);

    let message = e.fails(&["rules", "rule", "add", "work", "EXAMPLE.com", "direct"]);

    assert!(message.contains("example.com"), "{message}");
}

#[test]
fn an_invalid_domain_is_refused() {
    let e = env();
    e.ok(&["rules", "add", "Work"]);

    let message = e.fails(&["rules", "rule", "add", "work", "not a domain", "direct"]);

    assert!(message.contains("not a domain"), "{message}");
}

#[test]
fn a_rule_can_be_removed_and_a_missing_one_is_reported() {
    let e = env();
    e.ok(&["rules", "add", "Work"]);
    e.ok(&["rules", "rule", "add", "work", "example.com", "direct"]);

    e.ok(&["rules", "rule", "remove", "work", ".Example.com"]);

    assert!(e.ok(&["rules", "list"]).contains("0 rules"));
    let message = e.fails(&["rules", "rule", "remove", "work", "example.com"]);
    assert!(message.contains("example.com"), "{message}");
}

#[test]
fn the_default_can_be_changed() {
    let e = env();
    e.ok(&["proxy", "add", "corp", "http://corp.test:3128"]);
    e.ok(&["rules", "add", "Work"]);

    e.ok(&["rules", "default", "work", "corp"]);

    assert!(e.ok(&["rules", "show", "work"]).contains("corp"));
}

#[test]
fn a_profile_can_be_removed_and_its_id_becomes_free() {
    let e = env();
    e.ok(&["rules", "add", "Work"]);

    e.ok(&["rules", "remove", "work"]);

    assert!(e.fails(&["rules", "show", "work"]).contains("work"));
    assert!(e.ok(&["rules", "add", "Work"]).contains("`work`"));
}

#[test]
fn a_proxy_that_a_rules_profile_uses_cannot_be_removed() {
    let e = env();
    e.with_work_profile();

    let message = e.fails(&["proxy", "remove", "corp"]);

    assert!(message.contains("work"), "{message}");
}

#[test]
fn a_rules_profile_cannot_take_the_id_of_a_proxy() {
    let e = env();
    e.ok(&["proxy", "add", "Work", "http://corp.test:3128"]);

    let out = e.ok(&["rules", "add", "Work"]);

    assert!(out.contains("`work-2`"), "{out}");
}

// ---------- applying ----------

#[test]
fn using_a_profile_serves_the_script_and_points_the_system_at_it() {
    let e = env();
    e.with_work_profile();

    let run = start_rules(&e.ctx(), "work").unwrap();

    assert_eq!(e.net.read_proxy().unwrap(), pac(47123));
    assert_eq!(run.url(), PacConfig::loopback(47123));
    assert!(e.host.is_running());
    let script = e.host.script().unwrap();
    assert!(script.contains("PROXY corp.test:3128"), "{script}");
    assert!(script.contains("bank.ir"), "{script}");
    assert!(e.file("session-proxy.json").exists());
}

#[test]
fn stopping_restores_the_original_settings_and_then_stops_serving() {
    let e = env();
    e.with_work_profile();
    let run = start_rules(&e.ctx(), "work").unwrap();

    run.stop().unwrap();

    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
    assert!(!e.host.is_running());
    assert!(!e.file("session-proxy.json").exists());
}

#[test]
fn a_process_that_died_can_be_cleaned_up_with_proxy_off() {
    let e = env();
    e.with_work_profile();
    let run = start_rules(&e.ctx(), "work").unwrap();
    // Nothing runs its destructors when a process is killed.
    std::mem::forget(run);

    e.ok(&["proxy", "off"]);

    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
}

#[test]
fn an_unknown_profile_changes_nothing() {
    let e = env();

    let error = start_rules(&e.ctx(), "ghost").err().unwrap();

    assert!(error.to_string().contains("ghost"), "{error}");
    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
    assert_eq!(e.host.starts(), 0);
}

#[test]
fn a_profile_that_needs_socks_is_refused_and_nothing_changes() {
    let e = env();
    e.ok(&["proxy", "add", "v2ray", "socks5://127.0.0.1:10808"]);
    e.ok(&["rules", "add", "Dev", "--default", "v2ray"]);

    let error = start_rules(&e.ctx(), "dev").err().unwrap();

    let message = error.to_string();
    assert!(
        message.contains("v2ray") && message.contains("HTTP"),
        "{message}"
    );
    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
    assert_eq!(e.host.starts(), 0);
    assert!(!e.file("session-proxy.json").exists());
}

#[test]
fn a_group_with_no_live_member_changes_nothing() {
    let e = env();
    e.ok(&["proxy", "add", "a", "http://a.test:3128"]);
    e.ok(&["proxy", "add-group", "Best", "a"]);
    e.ok(&["rules", "add", "Dev", "--default", "best"]);

    let error = start_rules(&e.ctx(), "dev").err().unwrap();

    assert!(error.to_string().contains("Best"), "{error}");
    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
    assert_eq!(e.host.starts(), 0);
}

#[test]
fn a_group_target_is_served_as_its_fastest_member() {
    let e = env_with(
        FakeProber::new()
            .with_proxy("http://slow.test:3128", alive(300))
            .with_proxy("http://quick.test:3128", alive(30)),
    );
    e.ok(&["proxy", "add", "slow", "http://slow.test:3128"]);
    e.ok(&["proxy", "add", "quick", "http://quick.test:3128"]);
    e.ok(&["proxy", "add-group", "Best", "slow", "quick"]);
    e.ok(&["rules", "add", "Dev", "--default", "best"]);

    let _run = start_rules(&e.ctx(), "dev").unwrap();

    let script = e.host.script().unwrap();
    assert!(script.contains("quick.test:3128"), "{script}");
    assert!(!script.contains("slow.test"), "{script}");
}

#[test]
fn a_failing_apply_leaves_the_system_alone_and_stops_the_server() {
    let e = env();
    e.with_work_profile();
    e.net
        .fail_next_proxy_writes(1, SystemError::PermissionDenied("registry".to_owned()));

    let error = start_rules(&e.ctx(), "work").err().unwrap();

    assert!(error.to_string().contains("registry"), "{error}");
    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
    assert!(!e.host.is_running());
}

#[test]
fn a_server_that_cannot_start_changes_nothing() {
    let e = env();
    e.with_work_profile();
    e.host.fail_next_start(SystemError::CommandFailed {
        command: "start the PAC server".to_owned(),
        details: "address in use".to_owned(),
    });

    let error = start_rules(&e.ctx(), "work").err().unwrap();

    assert!(error.to_string().contains("address in use"), "{error}");
    assert_eq!(e.net.read_proxy().unwrap(), users_proxy());
}

#[test]
fn a_refresh_picks_up_an_edited_profile_and_keeps_the_address() {
    let e = env();
    e.with_work_profile();
    let mut run = start_rules(&e.ctx(), "work").unwrap();
    assert!(!run.refresh().unwrap(), "nothing changed yet");
    assert_eq!(e.host.script_count(), 1);

    e.ok(&["rules", "rule", "add", "work", "example.org", "direct"]);

    assert!(run.refresh().unwrap());
    assert!(e.host.script().unwrap().contains("example.org"));
    assert_eq!(e.net.read_proxy().unwrap(), pac(47123));
}

#[test]
fn a_refresh_that_cannot_build_a_script_keeps_serving_the_old_one() {
    let e = env();
    e.with_work_profile();
    let mut run = start_rules(&e.ctx(), "work").unwrap();
    let before = e.host.script().unwrap();
    e.ok(&["proxy", "add", "v2ray", "socks5://127.0.0.1:10808"]);
    e.ok(&["rules", "rule", "add", "work", "dev.example", "v2ray"]);

    let error = run.refresh().unwrap_err();

    assert!(error.to_string().contains("v2ray"), "{error}");
    assert_eq!(e.host.script().unwrap(), before);
    assert!(e.host.is_running());
}

#[test]
fn it_says_so_when_auto_connect_would_replace_the_script() {
    let e = env();
    e.with_work_profile();
    e.ok(&["proxy", "use", "corp"]);
    e.ok(&["auto", "proxy", "connect", "on"]);

    let run = start_rules(&e.ctx(), "work").unwrap();

    let notes = run.notes().join("\n");
    assert!(notes.contains("auto proxy connect off"), "{notes}");
}

#[test]
fn no_note_when_auto_connect_is_off() {
    let e = env();
    e.with_work_profile();

    let run = start_rules(&e.ctx(), "work").unwrap();

    assert!(run.notes().is_empty(), "{:?}", run.notes());
}

#[test]
fn a_profile_that_is_applied_cannot_be_removed() {
    let e = env();
    e.with_work_profile();
    let _run = start_rules(&e.ctx(), "work").unwrap();

    let message = e.fails(&["rules", "remove", "work"]);

    assert!(
        message.contains("rules off") || message.contains("proxy off"),
        "{message}"
    );
}

#[test]
fn the_list_marks_the_applied_profile() {
    let e = env();
    e.with_work_profile();
    e.ok(&["rules", "add", "Other"]);
    let _run = start_rules(&e.ctx(), "work").unwrap();

    let list = e.ok(&["rules", "list"]);

    let marked: Vec<&str> = list.lines().filter(|l| l.starts_with('*')).collect();
    assert_eq!(marked.len(), 1, "{list}");
    assert!(marked[0].contains("work"), "{list}");
}

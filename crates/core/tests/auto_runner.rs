//! One round of the automation against in-memory fakes: the proxy follows the
//! selected profile (plain proxy or group), DNS follows its own pool.

use std::sync::Arc;

use hopswitch_core::testing::{FakeProber, InMemoryNetwork, InMemoryStore};
use hopswitch_core::{
    AutoConfig, AutoRunner, Decision, DnsConfig, DnsProfile, DnsService, DnsState, Health,
    Profiles, ProxyConfig, ProxyGroup, ProxyProfile, ProxyScheme, ProxyService, ProxyState,
    SystemError, SystemNetwork, TickReport,
};

fn alive(ms: u32) -> Health {
    Health::Alive { latency_ms: ms }
}

fn users_proxy() -> ProxyState {
    ProxyState::Manual(ProxyConfig::new(ProxyScheme::Http, "corp.example", 3128).unwrap())
}

struct World {
    net: Arc<InMemoryNetwork>,
    proxy: ProxyService,
    dns: DnsService,
    profiles: Profiles,
}

/// Proxies `a`, `b`, `c` (hosts `a.test` ...), the group `smart` = [a, b], DNS profile `cf`.
fn world() -> World {
    let net = Arc::new(InMemoryNetwork::new().with_proxy(users_proxy()));
    let mut profiles = Profiles::new();
    for id in ["a", "b", "c"] {
        let config = ProxyConfig::new(ProxyScheme::Socks5, &format!("{id}.test"), 1080).unwrap();
        profiles
            .add_proxy(ProxyProfile::new(id, id, config).unwrap())
            .unwrap();
    }
    profiles
        .add_group(ProxyGroup::new("smart", "Smart", ["a", "b"].map(String::from)).unwrap())
        .unwrap();
    let cf = DnsProfile::new("cf", "cf", DnsConfig::from_strs(["1.1.1.1"]).unwrap()).unwrap();
    profiles.add_dns(cf).unwrap();
    World {
        proxy: ProxyService::new(net.clone(), Arc::new(InMemoryStore::new())),
        dns: DnsService::new(net.clone(), Arc::new(InMemoryStore::new())),
        net,
        profiles,
    }
}

fn proxy_auto(selected: Option<&str>) -> AutoConfig {
    let mut config = AutoConfig::default();
    config.proxy.connect = true;
    config.proxy.selected = selected.map(str::to_owned);
    config
}

fn tick(
    w: &mut World,
    runner: &mut AutoRunner,
    prober: &FakeProber,
    config: &AutoConfig,
) -> TickReport {
    runner.tick(&mut w.proxy, &mut w.dns, prober, &w.profiles, config)
}

fn proxy_host(w: &World) -> Option<String> {
    match w.net.read_proxy().unwrap() {
        ProxyState::Manual(c) => Some(c.host().to_owned()),
        _ => None,
    }
}

fn apply(w: &mut World, id: &str) {
    w.proxy.activate(w.profiles.proxy(id).unwrap()).unwrap();
}

// ---------- when nothing should happen ----------

#[test]
fn without_the_flag_or_a_selection_nothing_is_probed() {
    let prober = FakeProber::new().with_proxy("socks5://a.test:1080", alive(10));

    let mut flag_off = world();
    let mut config = proxy_auto(Some("a"));
    config.proxy.connect = false;
    tick(&mut flag_off, &mut AutoRunner::new(), &prober, &config);

    let mut nothing_selected = world();
    tick(
        &mut nothing_selected,
        &mut AutoRunner::new(),
        &prober,
        &proxy_auto(None),
    );

    assert!(prober.calls().is_empty());
    assert_eq!(
        flag_off.net.proxy_write_count() + nothing_selected.net.proxy_write_count(),
        0
    );
}

#[test]
fn a_selection_that_no_longer_exists_does_nothing() {
    let mut w = world();
    let prober = FakeProber::new();

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &prober,
        &proxy_auto(Some("ghost")),
    );

    assert_eq!(report.proxy.decision, Decision::Keep);
    assert!(prober.calls().is_empty());
}

// ---------- a plain proxy is selected ----------

#[test]
fn a_selected_proxy_is_connected_when_it_answers() {
    let mut w = world();
    let prober = FakeProber::new()
        .with_proxy("socks5://b.test:1080", alive(50))
        .with_proxy("socks5://a.test:1080", alive(10));

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &prober,
        &proxy_auto(Some("b")),
    );

    assert_eq!(report.proxy.decision, Decision::Activate("b".into()));
    assert_eq!(
        report.proxy.probes.len(),
        1,
        "only the selected proxy is tested"
    );
    assert_eq!(proxy_host(&w).as_deref(), Some("b.test"));
}

#[test]
fn a_selected_proxy_that_does_not_answer_is_not_connected() {
    let mut w = world();

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &FakeProber::new(),
        &proxy_auto(Some("a")),
    );

    assert_eq!(report.proxy.decision, Decision::Keep);
    assert_eq!(w.net.read_proxy().unwrap(), users_proxy());
}

#[test]
fn a_selected_proxy_is_given_up_only_after_a_streak_unless_decisive() {
    let cfg = proxy_auto(Some("a"));
    let nothing = FakeProber::new();

    let mut patient = world();
    apply(&mut patient, "a");
    let mut runner = AutoRunner::new();
    assert_eq!(
        tick(&mut patient, &mut runner, &nothing, &cfg)
            .proxy
            .decision,
        Decision::Keep
    );
    assert_eq!(
        tick(&mut patient, &mut runner, &nothing, &cfg)
            .proxy
            .decision,
        Decision::Keep
    );
    assert_eq!(
        tick(&mut patient, &mut runner, &nothing, &cfg)
            .proxy
            .decision,
        Decision::Deactivate
    );
    assert_eq!(
        patient.net.read_proxy().unwrap(),
        users_proxy(),
        "original settings are back"
    );

    let mut quick = world();
    apply(&mut quick, "a");
    let report = tick(&mut quick, &mut AutoRunner::decisive(), &nothing, &cfg);
    assert_eq!(report.proxy.decision, Decision::Deactivate);
}

#[test]
fn a_selected_proxy_never_hops_to_a_faster_one() {
    let mut w = world();
    apply(&mut w, "a");
    let prober = FakeProber::new()
        .with_proxy("socks5://a.test:1080", alive(900))
        .with_proxy("socks5://c.test:1080", alive(5));

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &prober,
        &proxy_auto(Some("a")),
    );

    assert_eq!(report.proxy.decision, Decision::Keep);
}

#[test]
fn a_profile_applied_by_hand_that_is_not_selected_is_left_alone() {
    let mut w = world();
    apply(&mut w, "c");
    let prober = FakeProber::new().with_proxy("socks5://a.test:1080", alive(1));

    for _ in 0..4 {
        let report = tick(
            &mut w,
            &mut AutoRunner::new(),
            &prober,
            &proxy_auto(Some("a")),
        );
        assert_eq!(report.proxy.decision, Decision::Keep);
    }
    assert_eq!(proxy_host(&w).as_deref(), Some("c.test"));
}

// ---------- a group is selected ----------

#[test]
fn a_selected_group_connects_to_its_fastest_live_member() {
    let mut w = world();
    let prober = FakeProber::new()
        .with_proxy("socks5://a.test:1080", alive(80))
        .with_proxy("socks5://b.test:1080", alive(20))
        .with_proxy("socks5://c.test:1080", alive(1));

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &prober,
        &proxy_auto(Some("smart")),
    );

    assert_eq!(report.proxy.decision, Decision::Activate("b".into()));
    assert_eq!(report.proxy.probes.len(), 2, "c is not a member");
    assert_eq!(proxy_host(&w).as_deref(), Some("b.test"));
}

#[test]
fn a_selected_group_moves_to_a_clearly_faster_member_but_not_a_slightly_faster_one() {
    let mut w = world();
    apply(&mut w, "a");
    let cfg = proxy_auto(Some("smart"));

    let slight = FakeProber::new()
        .with_proxy("socks5://a.test:1080", alive(100))
        .with_proxy("socks5://b.test:1080", alive(80));
    assert_eq!(
        tick(&mut w, &mut AutoRunner::new(), &slight, &cfg)
            .proxy
            .decision,
        Decision::Keep
    );

    let clear = FakeProber::new()
        .with_proxy("socks5://a.test:1080", alive(400))
        .with_proxy("socks5://b.test:1080", alive(60));
    assert_eq!(
        tick(&mut w, &mut AutoRunner::new(), &clear, &cfg)
            .proxy
            .decision,
        Decision::Activate("b".into())
    );
}

#[test]
fn a_selected_group_fails_over_then_disconnects_when_nothing_answers() {
    let cfg = proxy_auto(Some("smart"));

    let mut failover = world();
    apply(&mut failover, "a");
    let only_b = FakeProber::new().with_proxy("socks5://b.test:1080", alive(300));
    let mut runner = AutoRunner::new();
    tick(&mut failover, &mut runner, &only_b, &cfg);
    tick(&mut failover, &mut runner, &only_b, &cfg);
    assert_eq!(
        tick(&mut failover, &mut runner, &only_b, &cfg)
            .proxy
            .decision,
        Decision::Activate("b".into())
    );

    let mut dead = world();
    apply(&mut dead, "a");
    let mut runner = AutoRunner::new();
    let nothing = FakeProber::new();
    tick(&mut dead, &mut runner, &nothing, &cfg);
    tick(&mut dead, &mut runner, &nothing, &cfg);
    assert_eq!(
        tick(&mut dead, &mut runner, &nothing, &cfg).proxy.decision,
        Decision::Deactivate
    );
    assert_eq!(dead.net.read_proxy().unwrap(), users_proxy());
}

// ---------- DNS and independence ----------

#[test]
fn dns_still_follows_its_own_pool_and_options() {
    let mut w = world();
    let prober = FakeProber::new().with_dns("1.1.1.1", alive(20));
    let mut config = AutoConfig::default();
    config.dns.connect = true;
    config.dns.add_to_pool("cf");

    let report = tick(&mut w, &mut AutoRunner::new(), &prober, &config);

    assert_eq!(report.dns.decision, Decision::Activate("cf".into()));
    assert_eq!(report.proxy.decision, Decision::Keep);
}

#[test]
fn a_failure_in_one_kind_does_not_stop_the_other() {
    let mut w = world();
    w.net
        .fail_next_dns_writes(1, SystemError::PermissionDenied("admin needed".into()));
    let prober = FakeProber::new()
        .with_proxy("socks5://a.test:1080", alive(30))
        .with_dns("1.1.1.1", alive(20));
    let mut config = proxy_auto(Some("a"));
    config.dns.connect = true;
    config.dns.add_to_pool("cf");

    let report = tick(&mut w, &mut AutoRunner::new(), &prober, &config);

    assert_eq!(report.proxy.error, None);
    assert_eq!(proxy_host(&w).as_deref(), Some("a.test"));
    assert_eq!(report.dns.decision, Decision::Activate("cf".into()));
    assert!(
        report
            .dns
            .error
            .as_deref()
            .is_some_and(|e| e.contains("admin needed")),
        "{:?}",
        report.dns.error
    );
    assert_eq!(
        w.net.read_dns().unwrap(),
        DnsState::Automatic,
        "DNS was rolled back"
    );
}

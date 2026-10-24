//! One round of auto connect / auto switch against in-memory fakes.

use std::sync::Arc;

use hopswitch_core::testing::{FakeProber, InMemoryNetwork, InMemoryStore};
use hopswitch_core::{
    AutoConfig, AutoRunner, Decision, DnsConfig, DnsProfile, DnsService, DnsState, Health,
    Profiles, ProxyConfig, ProxyProfile, ProxyScheme, ProxyService, ProxyState, SystemError,
    SystemNetwork,
};

fn alive(ms: u32) -> Health {
    Health::Alive { latency_ms: ms }
}

fn proxy_config(host: &str) -> ProxyConfig {
    ProxyConfig::new(ProxyScheme::Socks5, host, 1080).unwrap()
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

/// Proxies `a`, `b`, `c` (hosts `a.test` ...) and DNS profile `cf` (1.1.1.1) are saved.
fn world() -> World {
    let net = Arc::new(InMemoryNetwork::new().with_proxy(users_proxy()));
    let mut profiles = Profiles::new();
    for id in ["a", "b", "c"] {
        let config = proxy_config(&format!("{id}.test"));
        profiles
            .add_proxy(ProxyProfile::new(id, id, config).unwrap())
            .unwrap();
    }
    let cf = DnsProfile::new("cf", "cf", DnsConfig::from_strs(["1.1.1.1"]).unwrap()).unwrap();
    profiles.add_dns(cf).unwrap();
    World {
        proxy: ProxyService::new(net.clone(), Arc::new(InMemoryStore::new())),
        dns: DnsService::new(net.clone(), Arc::new(InMemoryStore::new())),
        net,
        profiles,
    }
}

fn config(connect: bool, switch: bool, pool: &[&str]) -> AutoConfig {
    let mut config = AutoConfig::default();
    config.proxy.connect = connect;
    config.proxy.switch = switch;
    for id in pool {
        config.proxy.add_to_pool(id);
    }
    config
}

fn tick(
    w: &mut World,
    runner: &mut AutoRunner,
    prober: &FakeProber,
    config: &AutoConfig,
) -> hopswitch_core::TickReport {
    runner.tick(&mut w.proxy, &mut w.dns, prober, &w.profiles, config)
}

fn current_proxy_host(w: &World) -> Option<String> {
    match w.net.read_proxy().unwrap() {
        ProxyState::Manual(c) => Some(c.host().to_owned()),
        _ => None,
    }
}

#[test]
fn disabled_kinds_cost_no_traffic_and_change_nothing() {
    let mut w = world();
    let prober = FakeProber::new().with_proxy("socks5://a.test:1080", alive(10));

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &prober,
        &config(false, false, &["a"]),
    );

    assert_eq!(report.proxy.decision, Decision::Keep);
    assert_eq!(report.dns.decision, Decision::Keep);
    assert!(prober.calls().is_empty());
    assert_eq!(w.net.proxy_write_count(), 0);
}

#[test]
fn connect_applies_the_first_live_profile_of_the_pool() {
    let mut w = world();
    let prober = FakeProber::new()
        .with_proxy("socks5://b.test:1080", alive(80))
        .with_proxy("socks5://c.test:1080", alive(20));

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &prober,
        &config(true, false, &["a", "b", "c"]),
    );

    assert_eq!(report.proxy.decision, Decision::Activate("b".into()));
    assert_eq!(report.proxy.error, None);
    assert_eq!(current_proxy_host(&w).as_deref(), Some("b.test"));
}

#[test]
fn profiles_outside_the_pool_and_unknown_ids_are_ignored() {
    let mut w = world();
    let prober = FakeProber::new()
        .with_proxy("socks5://a.test:1080", alive(5))
        .with_proxy("socks5://c.test:1080", alive(50));

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &prober,
        &config(true, true, &["ghost", "c"]),
    );

    assert_eq!(report.proxy.decision, Decision::Activate("c".into()));
    assert_eq!(
        report.proxy.probes.len(),
        1,
        "only the existing pool member is probed"
    );
}

#[test]
fn switch_moves_an_active_profile_to_a_clearly_faster_one() {
    let mut w = world();
    w.proxy.activate(w.profiles.proxy("a").unwrap()).unwrap();
    let prober = FakeProber::new()
        .with_proxy("socks5://a.test:1080", alive(400))
        .with_proxy("socks5://b.test:1080", alive(60));

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &prober,
        &config(false, true, &["a", "b"]),
    );

    assert_eq!(report.proxy.decision, Decision::Activate("b".into()));
    assert_eq!(current_proxy_host(&w).as_deref(), Some("b.test"));
}

#[test]
fn a_profile_chosen_by_hand_outside_the_pool_is_left_alone() {
    let mut w = world();
    w.proxy.activate(w.profiles.proxy("c").unwrap()).unwrap();
    let prober = FakeProber::new().with_proxy("socks5://a.test:1080", alive(1));

    let report = tick(
        &mut w,
        &mut AutoRunner::new(),
        &prober,
        &config(true, true, &["a", "b"]),
    );

    assert_eq!(report.proxy.decision, Decision::Keep);
    assert_eq!(current_proxy_host(&w).as_deref(), Some("c.test"));
}

#[test]
fn giving_up_needs_a_streak_unless_the_runner_is_decisive() {
    let cfg = config(true, false, &["a"]);
    let nothing = FakeProber::new();

    let mut patient = world();
    patient
        .proxy
        .activate(patient.profiles.proxy("a").unwrap())
        .unwrap();
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
    quick
        .proxy
        .activate(quick.profiles.proxy("a").unwrap())
        .unwrap();
    let report = tick(&mut quick, &mut AutoRunner::decisive(), &nothing, &cfg);
    assert_eq!(report.proxy.decision, Decision::Deactivate);
}

#[test]
fn a_failure_in_one_kind_does_not_stop_the_other() {
    let mut w = world();
    w.net
        .fail_next_dns_writes(1, SystemError::PermissionDenied("admin needed".into()));
    let prober = FakeProber::new()
        .with_proxy("socks5://a.test:1080", alive(30))
        .with_dns("1.1.1.1", alive(20));
    let mut cfg = config(true, false, &["a"]);
    cfg.dns.connect = true;
    cfg.dns.add_to_pool("cf");

    let report = tick(&mut w, &mut AutoRunner::new(), &prober, &cfg);

    assert_eq!(report.proxy.error, None);
    assert_eq!(current_proxy_host(&w).as_deref(), Some("a.test"));
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

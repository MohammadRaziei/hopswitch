//! Probing every saved profile through the `Prober` port.

use std::net::IpAddr;
use std::time::{Duration, Instant};

use hopswitch::core::testing::FakeProber;
use hopswitch::core::{
    probe_dns_profiles, probe_proxies, ConnectivitySettings, DnsConfig, DnsProfile, Health, Prober,
    ProxyConfig, ProxyProfile,
};

fn proxy(id: &str, url: &str) -> ProxyProfile {
    ProxyProfile::new(id, id, url.parse::<ProxyConfig>().unwrap()).unwrap()
}

fn dns(id: &str, servers: &[&str]) -> DnsProfile {
    DnsProfile::new(id, id, DnsConfig::from_strs(servers).unwrap()).unwrap()
}

fn alive(ms: u32) -> Health {
    Health::Alive { latency_ms: ms }
}

#[test]
fn proxy_results_keep_order_ids_and_use_the_settings() {
    let prober = FakeProber::new()
        .with_proxy("socks5://127.0.0.1:1080", alive(40))
        .with_proxy("http://127.0.0.1:8080", Health::Dead);
    let profiles = [
        proxy("a", "socks5://127.0.0.1:1080"),
        proxy("b", "http://127.0.0.1:8080"),
    ];
    let settings =
        ConnectivitySettings::new("http://probe.test/x", "example.com", 1500, 30).unwrap();

    let results = probe_proxies(&prober, &profiles, &settings);

    assert_eq!(results.len(), 2);
    assert_eq!(
        (results[0].id.as_str(), results[0].health),
        ("a", alive(40))
    );
    assert_eq!(
        (results[1].id.as_str(), results[1].health),
        ("b", Health::Dead)
    );
    let calls = prober.calls();
    assert!(
        calls
            .iter()
            .all(|c| c.contains("http://probe.test/x") && c.contains("1500")),
        "{calls:?}"
    );
}

#[test]
fn a_dns_profile_is_as_fast_as_its_fastest_live_server() {
    let prober = FakeProber::new()
        .with_dns("9.9.9.9", Health::Dead)
        .with_dns("149.112.112.112", alive(70))
        .with_dns("1.1.1.1", alive(25));
    let profiles = [
        dns("quad", &["9.9.9.9", "149.112.112.112"]),
        dns("cf", &["1.1.1.1"]),
    ];

    let results = probe_dns_profiles(&prober, &profiles, &ConnectivitySettings::default());

    assert_eq!(results[0].health, alive(70));
    assert_eq!(results[1].health, alive(25));
}

#[test]
fn a_dns_profile_is_dead_only_when_every_server_is() {
    let prober = FakeProber::new().with_dns("9.9.9.9", Health::Dead);
    let profiles = [dns("quad", &["9.9.9.9", "8.8.8.8"])];

    let results = probe_dns_profiles(&prober, &profiles, &ConnectivitySettings::default());

    assert_eq!(results[0].health, Health::Dead);
}

#[test]
fn nothing_to_probe_gives_nothing() {
    let prober = FakeProber::new();

    assert_eq!(
        probe_proxies(&prober, &[], &ConnectivitySettings::default()),
        Vec::<hopswitch::core::Probe>::new()
    );
    assert_eq!(prober.calls(), Vec::<String>::new());
}

struct Slow;

impl Prober for Slow {
    fn probe_proxy(&self, _: &ProxyConfig, _: &str, _: Duration) -> Health {
        std::thread::sleep(Duration::from_millis(250));
        Health::Alive { latency_ms: 250 }
    }

    fn probe_dns(&self, _: IpAddr, _: &str, _: Duration) -> Health {
        Health::Dead
    }

    fn probe_local_port(&self, _: u16, _: Duration) -> Health {
        Health::Dead
    }
}

#[test]
fn profiles_are_probed_in_parallel() {
    let profiles: Vec<_> = (1..=4)
        .map(|n| proxy(&format!("p{n}"), &format!("http://127.0.0.1:{}", 8000 + n)))
        .collect();
    let started = Instant::now();

    let results = probe_proxies(&Slow, &profiles, &ConnectivitySettings::default());

    assert_eq!(results.len(), 4);
    assert!(
        started.elapsed() < Duration::from_millis(700),
        "{:?}",
        started.elapsed()
    );
}

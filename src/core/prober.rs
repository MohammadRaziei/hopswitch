//! The port through which `core` measures whether a proxy or DNS server works.
//!
//! The real implementation (the `probe` module) opens sockets; services and tests
//! use [`crate::core::testing::FakeProber`]. Probes are real tests, never ICMP (ADR-0009).

use std::net::IpAddr;
use std::thread;
use std::time::Duration;

use crate::core::{
    ConnectivitySettings, DnsConfig, DnsProfile, Health, PacConfig, Probe, ProxyConfig,
    ProxyProfile, ProxyState,
};

/// Tests one proxy or one DNS server and reports how it went.
pub trait Prober: Send + Sync {
    /// Fetches `target` (an `http://` or `https://` URL) *through* the proxy.
    fn probe_proxy(&self, config: &ProxyConfig, target: &str, timeout: Duration) -> Health;
    /// Asks the DNS `server` directly for `domain`.
    fn probe_dns(&self, server: IpAddr, domain: &str, timeout: Duration) -> Health;
    /// Whether something accepts connections on `port` of the loopback interface (127.0.0.1).
    fn probe_local_port(&self, port: u16, timeout: Duration) -> Health;
}

/// The PAC address in `state` when it is Hopswitch's own (`http://127.0.0.1:<port>/hopswitch.pac`)
/// but nothing is listening there any more: the process that served it is gone, so the system
/// points at a dead address. A PAC script someone else set, or any other state, is never judged
/// and nothing is probed for it.
pub fn stale_hopswitch_pac(
    state: &ProxyState,
    prober: &dyn Prober,
    timeout: Duration,
) -> Option<PacConfig> {
    let ProxyState::Pac(pac) = state else {
        return None;
    };
    let port = pac.hopswitch_port()?;
    match prober.probe_local_port(port, timeout) {
        Health::Alive { .. } => None,
        Health::Dead => Some(pac.clone()),
    }
}

fn timeout_of(settings: &ConnectivitySettings) -> Duration {
    Duration::from_millis(u64::from(settings.timeout_ms()))
}

/// Probes every proxy profile in parallel; results keep the input order.
pub fn probe_proxies(
    prober: &dyn Prober,
    profiles: &[ProxyProfile],
    settings: &ConnectivitySettings,
) -> Vec<Probe> {
    let timeout = timeout_of(settings);
    thread::scope(|scope| {
        let handles: Vec<_> = profiles
            .iter()
            .map(|profile| {
                scope.spawn(move || {
                    prober.probe_proxy(profile.config(), settings.proxy_target(), timeout)
                })
            })
            .collect();
        join_all(profiles.iter().map(ProxyProfile::id), handles)
    })
}

/// Probes every DNS profile in parallel. A profile is as fast as its fastest
/// live server, and dead only when all of its servers are.
pub fn probe_dns_profiles(
    prober: &dyn Prober,
    profiles: &[DnsProfile],
    settings: &ConnectivitySettings,
) -> Vec<Probe> {
    let timeout = timeout_of(settings);
    thread::scope(|scope| {
        let handles: Vec<_> = profiles
            .iter()
            .map(|profile| {
                scope.spawn(move || {
                    probe_servers(prober, profile.config(), settings.dns_domain(), timeout)
                })
            })
            .collect();
        join_all(profiles.iter().map(DnsProfile::id), handles)
    })
}

fn probe_servers(
    prober: &dyn Prober,
    config: &DnsConfig,
    domain: &str,
    timeout: Duration,
) -> Health {
    let results: Vec<Health> = thread::scope(|scope| {
        let handles: Vec<_> = config
            .servers()
            .iter()
            .map(|&server| scope.spawn(move || prober.probe_dns(server, domain, timeout)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or(Health::Dead))
            .collect()
    });
    results
        .into_iter()
        .filter_map(|health| match health {
            Health::Alive { latency_ms } => Some(latency_ms),
            Health::Dead => None,
        })
        .min()
        .map_or(Health::Dead, |latency_ms| Health::Alive { latency_ms })
}

/// A probe that panicked counts as dead rather than taking everything down.
fn join_all<'a>(
    ids: impl Iterator<Item = &'a str>,
    handles: Vec<thread::ScopedJoinHandle<'_, Health>>,
) -> Vec<Probe> {
    ids.zip(handles)
        .map(|(id, handle)| Probe::new(id, handle.join().unwrap_or(Health::Dead)))
        .collect()
}

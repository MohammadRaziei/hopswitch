//! Real connectivity probes for Hopswitch, using only the standard library.
//!
//! * DNS: a real `A` query sent over UDP straight to the server.
//! * Proxy: an HTTP request made *through* the proxy (HTTP, `SOCKS4a` or SOCKS5).
//!   For an `https://` target the tunnel opening is the test; no TLS is needed.
//!
//! A probe never fails loudly: anything that goes wrong is [`Health::Dead`].
//! Latency is the wall-clock time of the whole exchange (ADR-0009).

mod dns;
mod net;
mod proxy;
mod target;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use crate::core::{Health, Prober, ProxyConfig};

pub use dns::{build_query, is_answer};

/// The production [`Prober`].
#[derive(Debug, Clone)]
pub struct NetProber {
    dns_port: u16,
}

impl Default for NetProber {
    fn default() -> Self {
        Self::new()
    }
}

impl NetProber {
    /// A prober that talks to DNS servers on the standard port 53.
    pub fn new() -> Self {
        Self { dns_port: 53 }
    }

    /// Uses another DNS port (for tests against a local fake resolver).
    #[must_use]
    pub fn with_dns_port(mut self, port: u16) -> Self {
        self.dns_port = port;
        self
    }
}

impl Prober for NetProber {
    fn probe_proxy(&self, config: &ProxyConfig, target: &str, timeout: Duration) -> Health {
        let started = Instant::now();
        let result = proxy::probe(config, target, net::Deadline::after(timeout));
        health_of(result.is_ok(), started)
    }

    fn probe_dns(&self, server: IpAddr, domain: &str, timeout: Duration) -> Health {
        let started = Instant::now();
        let result = dns::probe(server, self.dns_port, domain, net::Deadline::after(timeout));
        health_of(result.is_ok(), started)
    }

    fn probe_local_port(&self, port: u16, timeout: Duration) -> Health {
        let started = Instant::now();
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        health_of(
            TcpStream::connect_timeout(&address, timeout).is_ok(),
            started,
        )
    }
}

fn health_of(ok: bool, started: Instant) -> Health {
    if ok {
        let ms = started.elapsed().as_millis().max(1);
        Health::Alive {
            latency_ms: u32::try_from(ms).unwrap_or(u32::MAX),
        }
    } else {
        Health::Dead
    }
}

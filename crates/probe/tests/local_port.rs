//! `NetProber::probe_local_port` against real loopback sockets.

use std::net::TcpListener;
use std::time::{Duration, Instant};

use hopswitch_core::{Health, Prober};
use hopswitch_probe::NetProber;

const TIMEOUT: Duration = Duration::from_secs(2);

#[test]
fn a_port_with_a_listener_is_alive() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let health = NetProber::new().probe_local_port(port, TIMEOUT);

    assert!(matches!(health, Health::Alive { .. }), "{health:?}");
}

#[test]
fn a_port_nobody_listens_on_is_dead() {
    // Bind to learn a free port, then close it again.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let health = NetProber::new().probe_local_port(port, TIMEOUT);

    assert_eq!(health, Health::Dead);
}

#[test]
fn the_probe_gives_up_within_its_timeout() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let started = Instant::now();

    let _ = NetProber::new().probe_local_port(port, Duration::from_millis(300));

    // Windows may retry a refused loopback connect for about a second before giving up.
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

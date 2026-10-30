//! DNS probing against a fake resolver on the loopback interface.

use std::net::{IpAddr, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use hopswitch::core::{Health, Prober};
use hopswitch::probe::{build_query, is_answer, NetProber};

const LOCALHOST: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

/// A resolver that answers one query using `reply(query) -> Option<packet>`.
fn resolver(reply: impl FnOnce(&[u8]) -> Option<Vec<u8>> + Send + 'static) -> u16 {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    thread::spawn(move || {
        let mut buf = [0u8; 512];
        if let Ok((n, from)) = socket.recv_from(&mut buf) {
            if let Some(packet) = reply(&buf[..n]) {
                let _ = socket.send_to(&packet, from);
            }
        }
    });
    port
}

/// A well-formed answer to `query`, with the given response code and answer count.
fn answer(query: &[u8], rcode: u8, answers: u8) -> Vec<u8> {
    let mut packet = query.to_vec();
    packet[2] = 0x81;
    packet[3] = 0x80 | rcode;
    packet[7] = answers;
    for _ in 0..answers {
        packet.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 93, 184, 216, 34]);
    }
    packet
}

fn probe(port: u16, timeout_ms: u64) -> Health {
    NetProber::new().with_dns_port(port).probe_dns(
        LOCALHOST,
        "example.com",
        Duration::from_millis(timeout_ms),
    )
}

#[test]
fn a_query_is_a_standard_recursive_a_lookup() {
    let packet = build_query(0x1234, "Example.COM.").unwrap();

    assert_eq!(&packet[..4], &[0x12, 0x34, 0x01, 0x00], "id, then RD flag");
    assert_eq!(&packet[4..12], &[0, 1, 0, 0, 0, 0, 0, 0], "one question");
    assert_eq!(&packet[12..], b"\x07example\x03com\x00\x00\x01\x00\x01");
}

#[test]
fn unusable_domains_are_refused_before_sending() {
    assert!(build_query(1, "").is_none());
    assert!(build_query(1, "a..b").is_none());
    assert!(build_query(1, &"a".repeat(64)).is_none());
}

#[test]
fn only_a_matching_successful_answer_counts() {
    let query = build_query(7, "example.com").unwrap();

    assert!(is_answer(7, &answer(&query, 0, 1)));
    assert!(!is_answer(8, &answer(&query, 0, 1)), "wrong id");
    assert!(!is_answer(7, &answer(&query, 3, 0)), "NXDOMAIN");
    assert!(!is_answer(7, &answer(&query, 0, 0)), "empty answer");
    assert!(!is_answer(7, &query), "a query is not an answer");
    assert!(!is_answer(7, &[0u8; 5]), "truncated");
}

#[test]
fn a_working_resolver_is_alive_with_a_measured_latency() {
    let port = resolver(|q| {
        thread::sleep(Duration::from_millis(80));
        Some(answer(q, 0, 1))
    });

    match probe(port, 2000) {
        Health::Alive { latency_ms } => assert!((60..3000).contains(&latency_ms), "{latency_ms}"),
        Health::Dead => panic!("should be alive"),
    }
}

#[test]
fn an_nxdomain_answer_is_dead() {
    let port = resolver(|q| Some(answer(q, 3, 0)));

    assert_eq!(probe(port, 1000), Health::Dead);
}

#[test]
fn a_silent_resolver_is_dead_after_the_timeout() {
    let port = resolver(|_| None);
    let started = Instant::now();

    assert_eq!(probe(port, 300), Health::Dead);
    assert!(
        started.elapsed() < Duration::from_millis(2500),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn an_answer_with_the_wrong_id_is_ignored() {
    let port = resolver(|q| {
        let mut packet = answer(q, 0, 1);
        packet[0] ^= 0xFF;
        Some(packet)
    });

    assert_eq!(probe(port, 300), Health::Dead);
}

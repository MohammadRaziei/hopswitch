//! Proxy probing against fake HTTP / SOCKS proxies on the loopback interface.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use hopswitch::core::{Health, Prober, ProxyConfig, ProxyScheme};
use hopswitch::probe::NetProber;

const OK_204: &[u8] = b"HTTP/1.1 204 No Content\r\n\r\n";

/// Starts a one-connection fake proxy; returns its port.
fn proxy(handler: impl FnOnce(TcpStream) + Send + 'static) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            handler(stream);
        }
    });
    port
}

fn read_n(stream: &mut TcpStream, n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).unwrap();
    buf
}

/// Reads a request head (up to the blank line) as text.
fn read_head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

fn probe(scheme: ProxyScheme, port: u16, target: &str, timeout_ms: u64) -> Health {
    let config = ProxyConfig::new(scheme, "127.0.0.1", port).unwrap();
    NetProber::new().probe_proxy(&config, target, Duration::from_millis(timeout_ms))
}

fn is_alive(health: Health) -> bool {
    matches!(health, Health::Alive { .. })
}

// ---------- HTTP proxy ----------

#[test]
fn an_http_proxy_gets_an_absolute_form_request_and_204_is_alive() {
    let port = proxy(|mut s| {
        let head = read_head(&mut s);
        assert!(
            head.starts_with("GET http://probe.test/ping?x=1 HTTP/1.1\r\n"),
            "{head}"
        );
        assert!(head.contains("Host: probe.test\r\n"), "{head}");
        s.write_all(OK_204).unwrap();
    });

    assert!(is_alive(probe(
        ProxyScheme::Http,
        port,
        "http://probe.test/ping?x=1",
        2000
    )));
}

#[test]
fn a_proxy_error_status_is_dead() {
    let port = proxy(|mut s| {
        read_head(&mut s);
        s.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").unwrap();
    });

    assert_eq!(
        probe(ProxyScheme::Http, port, "http://probe.test/", 2000),
        Health::Dead
    );
}

#[test]
fn a_connection_closed_without_an_answer_is_dead() {
    let port = proxy(drop);

    assert_eq!(
        probe(ProxyScheme::Http, port, "http://probe.test/", 2000),
        Health::Dead
    );
}

#[test]
fn nothing_listening_is_dead_quickly() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let started = Instant::now();

    // Windows retries a refused loopback connection for about two seconds, so it is the probe's own
    // timeout that bounds the wait there: use a short one.
    assert_eq!(
        probe(ProxyScheme::Http, port, "http://probe.test/", 500),
        Health::Dead
    );
    assert!(
        started.elapsed() < Duration::from_millis(2500),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_proxy_that_never_answers_is_dead_after_the_timeout() {
    let port = proxy(|mut s| {
        read_head(&mut s);
        thread::sleep(Duration::from_secs(2));
    });
    let started = Instant::now();

    assert_eq!(
        probe(ProxyScheme::Http, port, "http://probe.test/", 300),
        Health::Dead
    );
    assert!(
        started.elapsed() < Duration::from_millis(2500),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn latency_covers_the_whole_exchange() {
    let port = proxy(|mut s| {
        read_head(&mut s);
        thread::sleep(Duration::from_millis(120));
        s.write_all(OK_204).unwrap();
    });

    match probe(ProxyScheme::Http, port, "http://probe.test/", 2000) {
        Health::Alive { latency_ms } => assert!((100..3000).contains(&latency_ms), "{latency_ms}"),
        Health::Dead => panic!("should be alive"),
    }
}

#[test]
fn an_https_target_only_needs_the_tunnel_to_open() {
    let port = proxy(|mut s| {
        let head = read_head(&mut s);
        assert!(
            head.starts_with("CONNECT probe.test:443 HTTP/1.1\r\n"),
            "{head}"
        );
        s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .unwrap();
    });

    assert!(is_alive(probe(
        ProxyScheme::Http,
        port,
        "https://probe.test/x",
        2000
    )));
}

#[test]
fn a_refused_tunnel_is_dead() {
    let port = proxy(|mut s| {
        read_head(&mut s);
        s.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").unwrap();
    });

    assert_eq!(
        probe(ProxyScheme::Http, port, "https://probe.test/", 2000),
        Health::Dead
    );
}

#[test]
fn a_malformed_target_is_dead_without_connecting() {
    for bad in [
        "",
        "ftp://x/",
        "http://",
        "http://a b/",
        "http://user@host/",
    ] {
        assert_eq!(
            probe(ProxyScheme::Http, 9, bad, 500),
            Health::Dead,
            "{bad:?}"
        );
    }
}

#[test]
fn a_tls_proxy_is_not_supported_and_reports_dead() {
    let port = proxy(drop);

    assert_eq!(
        probe(ProxyScheme::Https, port, "http://probe.test/", 500),
        Health::Dead
    );
}

// ---------- SOCKS5 ----------

/// Plays the server side of a SOCKS5 no-auth handshake; returns the requested (host, port).
fn socks5_server(s: &mut TcpStream, reply_code: u8) -> (String, u16) {
    assert_eq!(read_n(s, 3), [5, 1, 0]);
    s.write_all(&[5, 0]).unwrap();
    let head: [u8; 4] = read_n(s, 4).try_into().unwrap();
    assert_eq!(&head[..3], &[5, 1, 0]);
    let host = match head[3] {
        3 => {
            let len = usize::from(read_n(s, 1).first().copied().unwrap());
            String::from_utf8(read_n(s, len)).unwrap()
        }
        1 => read_n(s, 4)
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join("."),
        other => panic!("unexpected address type {other}"),
    };
    let port = u16::from_be_bytes(read_n(s, 2).try_into().unwrap());
    s.write_all(&[5, reply_code, 0, 1, 0, 0, 0, 0, 0, 0])
        .unwrap();
    (host, port)
}

#[test]
fn socks5_sends_the_hostname_and_then_the_request() {
    let port = proxy(|mut s| {
        assert_eq!(socks5_server(&mut s, 0), ("probe.test".to_owned(), 80));
        let head = read_head(&mut s);
        assert!(head.starts_with("GET /ping HTTP/1.1\r\n"), "{head}");
        s.write_all(OK_204).unwrap();
    });

    assert!(is_alive(probe(
        ProxyScheme::Socks5,
        port,
        "http://probe.test/ping",
        2000
    )));
}

#[test]
fn socks5_with_an_https_target_stops_after_the_tunnel() {
    let port = proxy(|mut s| {
        assert_eq!(socks5_server(&mut s, 0), ("probe.test".to_owned(), 443));
    });

    assert!(is_alive(probe(
        ProxyScheme::Socks5,
        port,
        "https://probe.test/",
        2000
    )));
}

#[test]
fn socks5_refusal_and_required_authentication_are_dead() {
    let refused = proxy(|mut s| {
        socks5_server(&mut s, 5);
    });
    assert_eq!(
        probe(ProxyScheme::Socks5, refused, "http://probe.test/", 2000),
        Health::Dead
    );

    let needs_auth = proxy(|mut s| {
        read_n(&mut s, 3);
        s.write_all(&[5, 0xFF]).unwrap();
    });
    assert_eq!(
        probe(ProxyScheme::Socks5, needs_auth, "http://probe.test/", 2000),
        Health::Dead
    );
}

// ---------- SOCKS4 ----------

#[test]
fn socks4_uses_the_4a_extension_for_hostnames() {
    let port = proxy(|mut s| {
        assert_eq!(read_n(&mut s, 8), [4, 1, 0, 80, 0, 0, 0, 1]);
        assert_eq!(read_n(&mut s, 1), [0], "empty user id");
        assert_eq!(read_n(&mut s, 11), b"probe.test\0");
        s.write_all(&[0, 0x5A, 0, 0, 0, 0, 0, 0]).unwrap();
        let head = read_head(&mut s);
        assert!(head.starts_with("GET / HTTP/1.1\r\n"), "{head}");
        s.write_all(OK_204).unwrap();
    });

    assert!(is_alive(probe(
        ProxyScheme::Socks4,
        port,
        "http://probe.test/",
        2000
    )));
}

#[test]
fn socks4_rejection_is_dead() {
    let port = proxy(|mut s| {
        read_n(&mut s, 8 + 1 + 11);
        s.write_all(&[0, 0x5B, 0, 0, 0, 0, 0, 0]).unwrap();
    });

    assert_eq!(
        probe(ProxyScheme::Socks4, port, "http://probe.test/", 2000),
        Health::Dead
    );
}

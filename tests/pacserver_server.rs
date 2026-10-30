//! The loopback PAC server, exercised over real sockets (std only, like the server itself).

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use hopswitch::pacserver::PacServer;

const SCRIPT: &str = "function FindProxyForURL(url, host) { return \"DIRECT\"; }\n";

struct Reply {
    head: String,
    body: Vec<u8>,
}

impl Reply {
    fn status(&self) -> &str {
        self.head.lines().next().unwrap_or_default()
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

fn connect(addr: SocketAddr) -> TcpStream {
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
}

fn read_reply(mut stream: TcpStream) -> Reply {
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("no end of headers in {} bytes", raw.len()));
    Reply {
        head: String::from_utf8_lossy(&raw[..split]).into_owned(),
        body: raw[split + 4..].to_vec(),
    }
}

fn send(server: &PacServer, raw: &str) -> Reply {
    let mut stream = connect(server.addr());
    stream.write_all(raw.as_bytes()).unwrap();
    read_reply(stream)
}

fn get(server: &PacServer) -> Reply {
    let host = server.addr();
    send(
        server,
        &format!("GET /hopswitch.pac HTTP/1.1\r\nHost: {host}\r\nAccept: */*\r\n\r\n"),
    )
}

#[test]
fn it_listens_on_the_loopback_interface_on_a_free_port() {
    let server = PacServer::start(SCRIPT).unwrap();
    assert!(server.addr().ip().is_loopback());
    assert_ne!(server.addr().port(), 0);
}

#[test]
fn the_url_names_the_loopback_address_and_the_port_it_got() {
    let server = PacServer::start(SCRIPT).unwrap();
    let expected = format!("http://127.0.0.1:{}/hopswitch.pac", server.addr().port());
    assert_eq!(server.url().url(), expected);
    assert_eq!(server.url().hopswitch_port(), Some(server.addr().port()));
}

#[test]
fn two_servers_do_not_share_a_port() {
    let one = PacServer::start(SCRIPT).unwrap();
    let two = PacServer::start(SCRIPT).unwrap();
    assert_ne!(one.addr().port(), two.addr().port());
}

#[test]
fn a_get_returns_the_script_with_the_pac_content_type() {
    let server = PacServer::start(SCRIPT).unwrap();
    let reply = get(&server);
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
    assert_eq!(
        reply.header("content-type"),
        Some("application/x-ns-proxy-autoconfig")
    );
    assert_eq!(
        reply.header("content-length"),
        Some(SCRIPT.len().to_string().as_str())
    );
    assert_eq!(reply.header("connection"), Some("close"));
    assert_eq!(reply.body, SCRIPT.as_bytes());
}

#[test]
fn the_script_is_never_cached_so_a_changed_profile_is_seen_at_once() {
    let server = PacServer::start(SCRIPT).unwrap();
    let reply = get(&server);
    let cache = reply.header("cache-control").unwrap_or_default();
    assert!(cache.contains("no-store"), "{cache}");
}

#[test]
fn a_script_of_the_largest_size_comes_through_whole() {
    let big = format!("// {}\n{SCRIPT}", "x".repeat(990_000));
    let server = PacServer::start(big.clone()).unwrap();
    let reply = get(&server);
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
    assert_eq!(reply.body.len(), big.len());
    assert_eq!(reply.body, big.as_bytes());
}

#[test]
fn a_head_returns_the_headers_without_the_body() {
    let server = PacServer::start(SCRIPT).unwrap();
    let host = server.addr();
    let reply = send(
        &server,
        &format!("HEAD /hopswitch.pac HTTP/1.1\r\nHost: {host}\r\n\r\n"),
    );
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
    assert_eq!(
        reply.header("content-length"),
        Some(SCRIPT.len().to_string().as_str())
    );
    assert_eq!(reply.body, Vec::<u8>::new());
}

#[test]
fn a_query_string_is_ignored() {
    let server = PacServer::start(SCRIPT).unwrap();
    let host = server.addr();
    let reply = send(
        &server,
        &format!("GET /hopswitch.pac?nocache=17 HTTP/1.1\r\nHost: {host}\r\n\r\n"),
    );
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
}

#[test]
fn any_other_path_is_not_found() {
    let server = PacServer::start(SCRIPT).unwrap();
    let host = server.addr();
    for path in ["/", "/wpad.dat", "/hopswitch.pac/x", "/../hopswitch.pac"] {
        let reply = send(
            &server,
            &format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n\r\n"),
        );
        assert_eq!(reply.status(), "HTTP/1.1 404 Not Found", "{path}");
    }
}

#[test]
fn other_methods_are_refused_and_the_allowed_ones_are_named() {
    let server = PacServer::start(SCRIPT).unwrap();
    let host = server.addr();
    let reply = send(
        &server,
        &format!("POST /hopswitch.pac HTTP/1.1\r\nHost: {host}\r\nContent-Length: 0\r\n\r\n"),
    );
    assert_eq!(reply.status(), "HTTP/1.1 405 Method Not Allowed");
    assert_eq!(reply.header("allow"), Some("GET, HEAD"));
}

#[test]
fn garbage_is_a_bad_request() {
    let server = PacServer::start(SCRIPT).unwrap();
    for raw in [
        "hello\r\n\r\n",
        "GET\r\n\r\n",
        "GET /hopswitch.pac\r\n\r\n",
        "\r\n\r\n",
    ] {
        let reply = send(&server, raw);
        assert_eq!(reply.status(), "HTTP/1.1 400 Bad Request", "{raw:?}");
    }
}

#[test]
fn a_host_header_that_is_not_the_server_is_refused_against_dns_rebinding() {
    let server = PacServer::start(SCRIPT).unwrap();
    for host in [
        "evil.example",
        "evil.example:80",
        "127.0.0.1:1",
        "localhost.evil.example",
    ] {
        let reply = send(
            &server,
            &format!("GET /hopswitch.pac HTTP/1.1\r\nHost: {host}\r\n\r\n"),
        );
        assert_eq!(reply.status(), "HTTP/1.1 403 Forbidden", "{host}");
        assert!(
            reply.body.is_empty() || !String::from_utf8_lossy(&reply.body).contains("FindProxy")
        );
    }
}

#[test]
fn the_server_is_also_reachable_as_localhost_with_its_port() {
    let server = PacServer::start(SCRIPT).unwrap();
    let port = server.addr().port();
    let reply = send(
        &server,
        &format!("GET /hopswitch.pac HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"),
    );
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
}

#[test]
fn a_request_without_a_host_header_is_served_because_browsers_always_send_one() {
    let server = PacServer::start(SCRIPT).unwrap();
    let reply = send(&server, "GET /hopswitch.pac HTTP/1.0\r\n\r\n");
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
}

#[test]
fn a_request_that_arrives_in_pieces_is_understood() {
    let server = PacServer::start(SCRIPT).unwrap();
    let host = server.addr();
    let raw = format!("GET /hopswitch.pac HTTP/1.1\r\nHost: {host}\r\n\r\n");
    let mut stream = connect(server.addr());
    for byte in raw.as_bytes() {
        stream.write_all(&[*byte]).unwrap();
        stream.flush().unwrap();
    }
    assert_eq!(read_reply(stream).body, SCRIPT.as_bytes());
}

#[test]
fn oversized_headers_are_refused_and_the_server_keeps_working() {
    let server = PacServer::start(SCRIPT).unwrap();
    let host = server.addr();
    let junk = "a".repeat(20_000);
    let reply = send(
        &server,
        &format!("GET /hopswitch.pac HTTP/1.1\r\nHost: {host}\r\nX-Junk: {junk}\r\n\r\n"),
    );
    assert_eq!(
        reply.status(),
        "HTTP/1.1 431 Request Header Fields Too Large"
    );
    assert_eq!(get(&server).body, SCRIPT.as_bytes());
}

#[test]
fn set_script_changes_what_later_requests_get_without_changing_the_address() {
    let server = PacServer::start(SCRIPT).unwrap();
    let before = server.url();
    assert_eq!(get(&server).body, SCRIPT.as_bytes());

    server.set_script("function FindProxyForURL(u, h) { return \"PROXY a:1\"; }\n");

    assert_eq!(server.url(), before);
    assert!(String::from_utf8_lossy(&get(&server).body).contains("PROXY a:1"));
}

#[test]
fn an_idle_connection_does_not_hold_up_other_clients() {
    let server = PacServer::start(SCRIPT).unwrap();
    // Browsers open sockets ahead of time and send nothing on them.
    let _idle: Vec<TcpStream> = (0..5).map(|_| connect(server.addr())).collect();

    let started = Instant::now();
    let reply = get(&server);

    assert_eq!(reply.body, SCRIPT.as_bytes());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn many_clients_at_once_all_get_the_whole_script() {
    let server = Arc::new(PacServer::start(SCRIPT).unwrap());
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let server = Arc::clone(&server);
            thread::spawn(move || (0..5).all(|_| get(&server).body == SCRIPT.as_bytes()))
        })
        .collect();
    for worker in workers {
        assert!(worker.join().unwrap());
    }
}

#[test]
fn stopping_closes_the_port() {
    let server = PacServer::start(SCRIPT).unwrap();
    let addr = server.addr();
    assert!(TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_ok());

    server.stop();

    assert!(TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_err());
}

#[test]
fn dropping_the_server_stops_it_too() {
    let addr = {
        let server = PacServer::start(SCRIPT).unwrap();
        server.addr()
    };
    assert!(TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_err());
}

#[test]
fn debug_output_does_not_dump_the_script() {
    let server = PacServer::start(format!("// {}\n{SCRIPT}", "x".repeat(5_000))).unwrap();
    let text = format!("{server:?}");
    assert!(text.len() < 300, "{} bytes: {text}", text.len());
    assert!(text.contains("PacServer"));
}

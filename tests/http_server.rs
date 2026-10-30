//! The shared HTTP server, exercised over real sockets (std only, like the server itself).

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use hopswitch::http::{Request, Response, Server, MAX_CONNECTIONS};

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

fn loopback() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
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

fn send(server: &Server, raw: &str) -> Reply {
    let mut stream = connect(server.addr());
    stream.write_all(raw.as_bytes()).unwrap();
    read_reply(stream)
}

fn request_to(server: &Server, method: &str, target: &str) -> Reply {
    let host = server.addr();
    send(
        server,
        &format!("{method} {target} HTTP/1.1\r\nHost: {host}\r\n\r\n"),
    )
}

fn echo_server() -> Server {
    Server::start(loopback(), "test", |request: &Request| {
        Response::new("200 OK")
            .header("Content-Type", "text/plain")
            .body(format!("{} {}", request.method(), request.path()))
    })
    .unwrap()
}

#[test]
fn it_listens_on_the_loopback_interface_on_a_free_port() {
    let server = echo_server();
    assert!(server.addr().ip().is_loopback());
    assert_ne!(server.addr().port(), 0);
}

#[test]
fn it_refuses_to_bind_to_an_address_that_is_not_loopback() {
    for ip in [Ipv4Addr::UNSPECIFIED, Ipv4Addr::new(192, 168, 1, 10)] {
        let error = Server::start(SocketAddr::from((ip, 0)), "test", |_: &Request| {
            Response::new("200 OK")
        })
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{ip}");
    }
}

#[test]
fn the_handler_sees_the_method_path_and_headers_of_the_request() {
    let server = Server::start(loopback(), "test", |request: &Request| {
        let agent = request.header("USER-AGENT").unwrap_or("none");
        Response::new("200 OK").body(format!(
            "{}|{}|{}|{agent}",
            request.method(),
            request.target(),
            request.path()
        ))
    })
    .unwrap();
    let host = server.addr();
    let reply = send(
        &server,
        &format!("GET /a/b?x=1 HTTP/1.1\r\nHost: {host}\r\nuser-agent: probe\r\n\r\n"),
    );
    assert_eq!(reply.body, b"GET|/a/b?x=1|/a/b|probe");
}

#[test]
fn a_response_carries_its_status_headers_body_and_length() {
    let server = echo_server();
    let reply = request_to(&server, "GET", "/hello");
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
    assert_eq!(reply.header("content-type"), Some("text/plain"));
    assert_eq!(reply.header("content-length"), Some("10"));
    assert_eq!(reply.header("connection"), Some("close"));
    assert_eq!(reply.body, b"GET /hello");
}

#[test]
fn a_head_gets_the_headers_and_the_length_but_no_body() {
    let server = echo_server();
    let reply = request_to(&server, "HEAD", "/hello");
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
    assert_eq!(reply.header("content-length"), Some("11"));
    assert_eq!(reply.body, Vec::<u8>::new());
}

#[test]
fn a_handler_cannot_smuggle_extra_headers_through_a_line_break() {
    let server = Server::start(loopback(), "test", |_: &Request| {
        Response::new("200 OK").header("X-Note", "fine\r\nSet-Cookie: stolen=1")
    })
    .unwrap();
    let reply = request_to(&server, "GET", "/");
    assert!(reply.header("set-cookie").is_none(), "{}", reply.head);
    assert!(reply.header("x-note").is_none(), "{}", reply.head);
}

#[test]
fn a_handler_cannot_override_the_length_or_connection_headers() {
    let server = Server::start(loopback(), "test", |_: &Request| {
        Response::new("200 OK")
            .header("Content-Length", "999")
            .header("Connection", "keep-alive")
            .body("abc")
    })
    .unwrap();
    let reply = request_to(&server, "GET", "/");
    assert_eq!(reply.header("content-length"), Some("3"));
    assert_eq!(reply.header("connection"), Some("close"));
    assert_eq!(reply.body, b"abc");
}

#[test]
fn garbage_is_a_bad_request_and_never_reaches_the_handler() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let server = Server::start(loopback(), "test", move |_: &Request| {
        seen.fetch_add(1, Ordering::SeqCst);
        Response::new("200 OK")
    })
    .unwrap();
    for raw in [
        "nonsense\r\n\r\n",
        "GET /\r\n\r\n",
        "GET / FTP/1.0\r\n\r\n",
        "GET index.html HTTP/1.1\r\n\r\n",
    ] {
        assert_eq!(
            send(&server, raw).status(),
            "HTTP/1.1 400 Bad Request",
            "{raw:?}"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_host_header_that_is_not_the_server_is_refused_against_dns_rebinding() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let server = Server::start(loopback(), "test", move |_: &Request| {
        seen.fetch_add(1, Ordering::SeqCst);
        Response::new("200 OK")
    })
    .unwrap();
    let port = server.addr().port();
    for host in [
        "evil.example".to_owned(),
        format!("evil.example:{port}"),
        "127.0.0.1".to_owned(),
        format!("127.0.0.1:{}", port.wrapping_add(1)),
        format!("127.0.0.1.evil.example:{port}"),
    ] {
        let reply = send(&server, &format!("GET / HTTP/1.1\r\nHost: {host}\r\n\r\n"));
        assert_eq!(reply.status(), "HTTP/1.1 403 Forbidden", "{host}");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn the_server_is_also_reachable_as_localhost_with_its_port() {
    let server = echo_server();
    let port = server.addr().port();
    let reply = send(
        &server,
        &format!("GET / HTTP/1.1\r\nHost: LocalHost:{port}\r\n\r\n"),
    );
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
}

#[test]
fn a_request_without_a_host_header_is_served_because_browsers_always_send_one() {
    let server = echo_server();
    let reply = send(&server, "GET / HTTP/1.0\r\n\r\n");
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
}

#[test]
fn a_request_that_arrives_in_pieces_is_understood() {
    let server = echo_server();
    let host = server.addr();
    let mut stream = connect(host);
    for piece in [
        "GE".to_owned(),
        "T /slow HTTP/1.1\r\nHo".to_owned(),
        format!("st: {host}\r\n"),
        "\r\n".to_owned(),
    ] {
        stream.write_all(piece.as_bytes()).unwrap();
        stream.flush().unwrap();
        thread::sleep(Duration::from_millis(30));
    }
    assert_eq!(read_reply(stream).body, b"GET /slow");
}

#[test]
fn oversized_headers_are_refused_and_the_server_keeps_working() {
    let server = echo_server();
    let host = server.addr();
    let padding = "a".repeat(20 * 1024);
    let reply = send(
        &server,
        &format!("GET / HTTP/1.1\r\nHost: {host}\r\nX-Big: {padding}\r\n\r\n"),
    );
    assert_eq!(
        reply.status(),
        "HTTP/1.1 431 Request Header Fields Too Large"
    );
    assert_eq!(request_to(&server, "GET", "/").status(), "HTTP/1.1 200 OK");
}

#[test]
fn an_idle_connection_does_not_hold_up_other_clients() {
    let server = echo_server();
    let _idle = connect(server.addr());
    let started = Instant::now();
    assert_eq!(request_to(&server, "GET", "/").status(), "HTTP/1.1 200 OK");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn many_clients_at_once_all_get_their_answer() {
    let server = Arc::new(echo_server());
    let workers: Vec<_> = (0..MAX_CONNECTIONS / 2)
        .map(|n| {
            let server = Arc::clone(&server);
            thread::spawn(move || request_to(&server, "GET", &format!("/{n}")).body)
        })
        .collect();
    for (n, worker) in workers.into_iter().enumerate() {
        assert_eq!(worker.join().unwrap(), format!("GET /{n}").into_bytes());
    }
}

#[test]
fn a_handler_that_panics_gets_a_500_and_does_not_use_up_connection_slots() {
    let server = Server::start(loopback(), "test", |request: &Request| {
        assert!(request.path() != "/boom", "a handler bug");
        Response::new("200 OK")
    })
    .unwrap();
    for _ in 0..(MAX_CONNECTIONS + 8) {
        assert_eq!(
            request_to(&server, "GET", "/boom").status(),
            "HTTP/1.1 500 Internal Server Error"
        );
    }
    assert_eq!(
        request_to(&server, "GET", "/ok").status(),
        "HTTP/1.1 200 OK"
    );
}

#[test]
fn stopping_closes_the_port() {
    let server = echo_server();
    let addr = server.addr();
    server.stop();
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err());
}

#[test]
fn dropping_the_server_stops_it_too() {
    let addr = {
        let server = echo_server();
        server.addr()
    };
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err());
}

#[test]
fn debug_output_names_the_address() {
    let server = echo_server();
    assert!(format!("{server:?}").contains(&server.addr().port().to_string()));
}

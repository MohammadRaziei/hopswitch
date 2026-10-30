//! The web API of `hopswitch serve`, exercised over real sockets with an in-memory system.

use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use hopswitch::cli::{run, start_web, CliError, Context, ServeOptions, WebServer};
use hopswitch::core::testing::{FakePacHost, FakeProber, InMemoryNetwork};

struct Env {
    dir: tempfile::TempDir,
}

fn env() -> Env {
    Env {
        dir: tempfile::tempdir().unwrap(),
    }
}

impl Env {
    fn ctx(&self) -> Context {
        Context {
            dir: self.dir.path().join("hopswitch"),
            network: Arc::new(InMemoryNetwork::new()),
            prober: Arc::new(FakeProber::new()),
            pac: Arc::new(FakePacHost::new()),
        }
    }

    fn start(&self) -> WebServer {
        start_web(Arc::new(self.ctx()), &options(0)).unwrap()
    }

    fn token_path(&self) -> std::path::PathBuf {
        self.dir.path().join("hopswitch").join("web-token")
    }
}

fn options(port: u16) -> ServeOptions {
    ServeOptions {
        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port,
        auto: false,
    }
}

struct Reply {
    head: String,
    body: String,
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

fn exchange(addr: SocketAddr, raw: &str) -> Reply {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").expect("no end of headers");
    Reply {
        head: head.to_owned(),
        body: body.to_owned(),
    }
}

/// `extra` is any further header lines, each ending in `\r\n`.
fn call(server: &WebServer, method: &str, path: &str, extra: &str) -> Reply {
    let addr = server.addr();
    exchange(
        addr,
        &format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{extra}\r\n"),
    )
}

fn with_token(server: &WebServer) -> String {
    format!("Authorization: Bearer {}\r\n", server.token())
}

#[test]
fn the_token_is_128_bits_or_more_of_hex_kept_in_a_file_in_the_settings_folder() {
    let env = env();
    let server = env.start();
    let token = server.token().to_owned();
    assert!(
        token.len() >= 32 && token.chars().all(|c| c.is_ascii_hexdigit()),
        "{token}"
    );
    assert_eq!(fs::read_to_string(env.token_path()).unwrap().trim(), token);
}

#[test]
fn the_same_token_is_used_again_after_a_restart_and_two_homes_differ() {
    let env = env();
    let first = env.start().token().to_owned();
    assert_eq!(env.start().token(), first);
    assert_ne!(self::env().start().token(), first);
}

#[test]
fn a_damaged_token_file_is_replaced_not_trusted() {
    let env = env();
    let first = env.start().token().to_owned();
    fs::write(env.token_path(), "abc").unwrap();
    let second = env.start().token().to_owned();
    assert_ne!(second, "abc");
    assert!(second.len() >= 32);
    assert_ne!(second, first);
}

#[cfg(unix)]
#[test]
fn the_token_file_is_readable_by_its_owner_only_even_if_it_was_not_before() {
    use std::os::unix::fs::PermissionsExt;
    let env = env();
    let _first = env.start();
    let mode = |env: &Env| fs::metadata(env.token_path()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&env), 0o600);
    fs::set_permissions(env.token_path(), fs::Permissions::from_mode(0o644)).unwrap();
    let _second = env.start();
    assert_eq!(mode(&env), 0o600);
}

#[test]
fn the_status_endpoint_answers_with_the_cli_status_as_json() {
    let env = env();
    let server = env.start();
    let reply = call(&server, "GET", "/api/status", &with_token(&server));
    assert_eq!(reply.status(), "HTTP/1.1 200 OK");
    assert_eq!(reply.header("content-type"), Some("application/json"));
    let json: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let mut expected = Vec::new();
    run(&["status".to_owned()], &env.ctx(), &mut expected).unwrap();
    assert_eq!(json["status"], String::from_utf8(expected).unwrap());
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn api_responses_are_never_cached_and_carry_no_cors_headers() {
    let env = env();
    let server = env.start();
    let reply = call(&server, "GET", "/api/status", &with_token(&server));
    assert_eq!(reply.header("cache-control"), Some("no-store"));
    assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
    assert!(
        reply.header("access-control-allow-origin").is_none(),
        "{}",
        reply.head
    );
}

#[test]
fn without_the_right_token_the_api_says_401_and_never_runs_a_command() {
    let env = env();
    let server = env.start();
    let token = server.token().to_owned();
    for extra in [
        String::new(),
        "Authorization: Bearer nope\r\n".to_owned(),
        format!("Authorization: Bearer {token}x\r\n"),
        format!("Authorization: Bearer {}\r\n", &token[..token.len() - 1]),
        format!("Authorization: Basic {token}\r\n"),
        format!("Authorization: {token}\r\n"),
        "Authorization: Bearer\r\n".to_owned(),
    ] {
        let reply = call(&server, "GET", "/api/status", &extra);
        assert_eq!(reply.status(), "HTTP/1.1 401 Unauthorized", "{extra:?}");
        assert_eq!(reply.header("www-authenticate"), Some("Bearer"));
        assert!(!reply.body.contains("proxy"), "{extra:?}");
    }
}

#[test]
fn the_bearer_scheme_is_matched_without_regard_to_letter_case() {
    let env = env();
    let server = env.start();
    let extra = format!("Authorization: bearer {}\r\n", server.token());
    assert_eq!(
        call(&server, "GET", "/api/status", &extra).status(),
        "HTTP/1.1 200 OK"
    );
}

#[test]
fn an_unknown_api_path_does_not_reveal_whether_it_exists_before_the_token_is_checked() {
    let env = env();
    let server = env.start();
    assert_eq!(
        call(&server, "GET", "/api/nothing", "").status(),
        "HTTP/1.1 401 Unauthorized"
    );
    assert_eq!(
        call(&server, "GET", "/api/nothing", &with_token(&server)).status(),
        "HTTP/1.1 404 Not Found"
    );
}

#[test]
fn only_get_and_head_are_allowed_and_the_allowed_ones_are_named() {
    let env = env();
    let server = env.start();
    let reply = call(&server, "POST", "/api/status", &with_token(&server));
    assert_eq!(reply.status(), "HTTP/1.1 405 Method Not Allowed");
    assert_eq!(reply.header("allow"), Some("GET, HEAD"));
    let head = call(&server, "HEAD", "/api/status", &with_token(&server));
    assert_eq!(head.status(), "HTTP/1.1 200 OK");
    assert_eq!(head.body, "");
}

#[test]
fn a_request_from_another_origin_is_refused_even_with_the_right_token() {
    let env = env();
    let server = env.start();
    let token = with_token(&server);
    for origin in ["http://evil.example", "https://127.0.0.1", "null"] {
        let extra = format!("{token}Origin: {origin}\r\n");
        let reply = call(&server, "GET", "/api/status", &extra);
        assert_eq!(reply.status(), "HTTP/1.1 403 Forbidden", "{origin}");
    }
}

#[test]
fn a_request_from_the_servers_own_origin_or_with_none_is_served() {
    let env = env();
    let server = env.start();
    let token = with_token(&server);
    let own = format!("{token}Origin: http://{}\r\n", server.addr());
    assert_eq!(
        call(&server, "GET", "/api/status", &own).status(),
        "HTTP/1.1 200 OK"
    );
    assert_eq!(
        call(&server, "GET", "/api/status", &token).status(),
        "HTTP/1.1 200 OK"
    );
}

#[test]
fn a_foreign_host_name_is_refused_against_dns_rebinding() {
    let env = env();
    let server = env.start();
    let raw = format!(
        "GET /api/status HTTP/1.1\r\nHost: evil.example:{}\r\n{}\r\n",
        server.addr().port(),
        with_token(&server)
    );
    assert_eq!(
        exchange(server.addr(), &raw).status(),
        "HTTP/1.1 403 Forbidden"
    );
}

#[test]
fn outside_the_api_there_is_nothing_yet() {
    let env = env();
    let server = env.start();
    assert_eq!(
        call(&server, "GET", "/", "").status(),
        "HTTP/1.1 404 Not Found"
    );
}

#[test]
fn an_address_that_is_not_loopback_is_a_usage_error() {
    let env = env();
    let bad = ServeOptions {
        ip: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        port: 0,
        auto: false,
    };
    assert!(matches!(
        start_web(Arc::new(env.ctx()), &bad),
        Err(CliError::Usage(_))
    ));
}

#[test]
fn a_busy_port_is_a_failure_that_names_the_option_to_change() {
    let env = env();
    let first = env.start();
    let busy = options(first.addr().port());
    match start_web(Arc::new(env.ctx()), &busy) {
        Err(CliError::Failure(message)) => assert!(message.contains("--port"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_url_puts_the_token_after_a_hash_so_a_browser_never_sends_it() {
    let env = env();
    let server = env.start();
    let url = server.url();
    assert_eq!(
        url,
        format!("http://{}/#token={}", server.addr(), server.token())
    );
}

#[test]
fn debug_output_does_not_show_the_token() {
    let env = env();
    let server = env.start();
    assert!(!format!("{server:?}").contains(server.token()));
}

//! A loopback HTTP server that hands one PAC script to the system proxy settings (ADR-0011
//! decision 6), using only the standard library.
//!
//! The script is served at `http://127.0.0.1:<port>/hopswitch.pac`; the port is chosen by the
//! system, so the address is only known after [`PacServer::start`]. While a rules profile is applied
//! this server has to be running: PAC clients fetch the script from here.
//!
//! What it guards against, because it is a web server on a machine that also runs browsers:
//!
//! * **DNS rebinding.** A request whose `Host` header is not this server's own address is refused,
//!   so a web page cannot read the script (it lists the user's proxy addresses) by pointing a name
//!   it controls at `127.0.0.1`.
//! * **Idle sockets.** Browsers open connections ahead of time and send nothing, so each connection
//!   has its own thread (at most [`MAX_CONNECTIONS`] at once) and a timeout.
//! * **Oversized requests.** Headers beyond 8 KiB get `431` and are not buffered further.
//! * **Stale scripts.** Responses carry `Cache-Control: no-store`.

use std::fmt::{self, Write as _};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use hopswitch_core::{PacConfig, HOPSWITCH_PAC_PATH};

/// Where the script is served.
const PATH: &str = HOPSWITCH_PAC_PATH;
/// What PAC clients expect in `Content-Type`.
const CONTENT_TYPE: &str = "application/x-ns-proxy-autoconfig";
/// Most bytes of request line and headers read before answering `431`.
const MAX_HEADER_BYTES: usize = 8 * 1024;
/// Most connections handled at the same time; further ones are closed at once.
pub const MAX_CONNECTIONS: usize = 32;
/// How long a client may take to send its request, and to take the response.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long to keep reading after the response, so a client still sending is not reset.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);
/// Most bytes read and thrown away after the response.
const DRAIN_LIMIT: usize = 1 << 20;

/// State shared by the accepting thread and the connection threads.
struct Shared {
    script: RwLock<Arc<str>>,
    stopping: AtomicBool,
    active: AtomicUsize,
}

/// A running PAC server. It stops when dropped or when [`PacServer::stop`] is called.
pub struct PacServer {
    addr: SocketAddr,
    url: PacConfig,
    shared: Arc<Shared>,
    accepting: Option<JoinHandle<()>>,
}

impl fmt::Debug for PacServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The script can be close to a megabyte; its size is what matters in a log.
        f.debug_struct("PacServer")
            .field("addr", &self.addr)
            .field("script_bytes", &self.shared.script_snapshot().len())
            .finish_non_exhaustive()
    }
}

impl Shared {
    fn script_snapshot(&self) -> Arc<str> {
        Arc::clone(&self.script.read().unwrap_or_else(PoisonError::into_inner))
    }
}

impl PacServer {
    /// Starts serving `script` on a free port of the loopback interface.
    ///
    /// # Errors
    /// The I/O error when the port cannot be bound or the thread cannot be started.
    pub fn start(script: impl Into<String>) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let url = PacConfig::loopback(addr.port());
        let shared = Arc::new(Shared {
            script: RwLock::new(Arc::from(script.into())),
            stopping: AtomicBool::new(false),
            active: AtomicUsize::new(0),
        });
        let for_thread = Arc::clone(&shared);
        let accepting = thread::Builder::new()
            .name("hopswitch-pac-accept".to_owned())
            .spawn(move || accept_loop(&listener, &for_thread, addr))?;
        Ok(Self {
            addr,
            url,
            shared,
            accepting: Some(accepting),
        })
    }

    /// The address the server listens on (always loopback).
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The address to give the system as its PAC script address.
    #[must_use]
    pub fn url(&self) -> PacConfig {
        self.url.clone()
    }

    /// Replaces the script that later requests receive; the address stays the same.
    pub fn set_script(&self, script: impl Into<String>) {
        *self
            .shared
            .script
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Arc::from(script.into());
    }

    /// Stops listening and waits for the accepting thread to end. Connections already being
    /// answered finish on their own.
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        let Some(accepting) = self.accepting.take() else {
            return;
        };
        self.shared.stopping.store(true, Ordering::SeqCst);
        // `accept` blocks, so one connection wakes the thread to let it see the flag.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(2));
        let _ = accepting.join();
    }
}

impl Drop for PacServer {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>, addr: SocketAddr) {
    for incoming in listener.incoming() {
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = incoming else {
            // Out of descriptors or a reset while accepting: do not spin.
            thread::sleep(Duration::from_millis(50));
            continue;
        };
        if shared.active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            shared.active.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let for_connection = Arc::clone(shared);
        let spawned = thread::Builder::new()
            .name("hopswitch-pac-conn".to_owned())
            .spawn(move || {
                serve(stream, &for_connection, addr);
                for_connection.active.fetch_sub(1, Ordering::SeqCst);
            });
        if spawned.is_err() {
            shared.active.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// What a request asked for, once it has been understood.
enum Verdict {
    Send { body: bool },
    Refuse(&'static str, &'static [(&'static str, &'static str)]),
}

fn serve(mut stream: TcpStream, shared: &Shared, addr: SocketAddr) {
    let _ = stream.set_read_timeout(Some(CLIENT_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CLIENT_TIMEOUT));
    let head = match read_head(&mut stream) {
        Head::Complete(text) => text,
        Head::TooLarge => {
            respond_error(&mut stream, "431 Request Header Fields Too Large", &[]);
            return;
        }
        // Nothing arrived, or the client went away or stalled: there is nobody to answer.
        Head::Incomplete => return,
    };
    match judge(&head, addr) {
        Verdict::Send { body } => {
            let script = shared.script_snapshot();
            let mut reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {CONTENT_TYPE}\r\nContent-Length: {}\r\n\
                 Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
                script.len()
            )
            .into_bytes();
            if body {
                reply.extend_from_slice(script.as_bytes());
            }
            let _ = stream.write_all(&reply);
            finish(&mut stream);
        }
        Verdict::Refuse(status, extra) => respond_error(&mut stream, status, extra),
    }
}

enum Head {
    Complete(String),
    TooLarge,
    Incomplete,
}

/// Reads up to the blank line that ends the headers.
fn read_head(stream: &mut TcpStream) -> Head {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            return Head::Complete(String::from_utf8_lossy(&buffer[..end]).into_owned());
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return Head::TooLarge;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Head::Incomplete,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
}

/// Decides what to answer. Order: the request must make sense (400), come from this server's own
/// name (403), use a method we serve (405) and ask for our path (404).
fn judge(head: &str, addr: SocketAddr) -> Verdict {
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let [method, target, version] = parts[..] else {
        return Verdict::Refuse("400 Bad Request", &[]);
    };
    if !version.starts_with("HTTP/1.") || !target.starts_with('/') {
        return Verdict::Refuse("400 Bad Request", &[]);
    }
    let host = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("host")
            .then(|| value.trim())
    });
    if host.is_some_and(|host| !is_own_name(host, addr)) {
        return Verdict::Refuse("403 Forbidden", &[]);
    }
    let body = match method {
        "GET" => true,
        "HEAD" => false,
        _ => return Verdict::Refuse("405 Method Not Allowed", &[("Allow", "GET, HEAD")]),
    };
    let path = target.split_once('?').map_or(target, |(path, _)| path);
    if path != PATH {
        return Verdict::Refuse("404 Not Found", &[]);
    }
    Verdict::Send { body }
}

/// Whether `host` is how this server would be reached: its loopback address or `localhost`, with
/// its port.
fn is_own_name(host: &str, addr: SocketAddr) -> bool {
    let port = addr.port();
    host == format!("127.0.0.1:{port}") || host.eq_ignore_ascii_case(&format!("localhost:{port}"))
}

fn respond_error(stream: &mut TcpStream, status: &str, extra: &[(&str, &str)]) {
    let mut reply = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n");
    for (name, value) in extra {
        let _ = write!(reply, "{name}: {value}\r\n");
    }
    reply.push_str("\r\n");
    let _ = stream.write_all(reply.as_bytes());
    finish(stream);
}

/// Ends the response without resetting a client that is still sending: close our side, then read
/// what is left for a moment. (Closing a socket with unread data makes some systems send a reset,
/// which can destroy the response before the client has read it.)
fn finish(stream: &mut TcpStream) {
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(DRAIN_TIMEOUT));
    let mut sink = [0_u8; 4096];
    let mut total = 0;
    while let Ok(n) = stream.read(&mut sink) {
        total += n;
        if n == 0 || total > DRAIN_LIMIT {
            break;
        }
    }
}

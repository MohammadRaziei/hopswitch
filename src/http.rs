//! A tiny HTTP/1.1 server for the loopback interface, using only the standard library. The PAC
//! server and the web interface (ADR-0011, ADR-0012) are handlers on top of it, so the parts that
//! keep a web server on a machine that also runs browsers safe exist once:
//!
//! * **Loopback only.** [`Server::start`] refuses any address that is not loopback. Reaching a
//!   server from elsewhere is done with an SSH tunnel, not by opening the port to the network.
//! * **DNS rebinding.** A request whose `Host` header is not this server's own address (or
//!   `localhost` with the same port) gets `403` and never reaches the handler.
//! * **Idle sockets.** Browsers open connections ahead of time and send nothing, so each connection
//!   has its own thread (at most [`MAX_CONNECTIONS`] at once) and a timeout.
//! * **Oversized requests.** Headers beyond 8 KiB get `431` and are not buffered further.
//! * **Handler bugs.** A handler that panics costs one `500`, not a connection slot or the server.
//! * **Response splitting.** A header whose name or value contains a line break is dropped, and the
//!   handler cannot set `Content-Length` or `Connection`; the server owns both.
//!
//! Every response closes the connection, so there is no request body handling: a body sent by the
//! client is read and discarded.

use std::fmt::{self, Write as _};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

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

/// A request whose line and headers have been read and found well formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
}

impl Request {
    /// The method exactly as sent (`GET`, `HEAD`, ...).
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The request target as sent: the path and the query, always starting with `/`.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The target without its query string.
    #[must_use]
    pub fn path(&self) -> &str {
        self.target
            .split_once('?')
            .map_or(self.target.as_str(), |(path, _)| path)
    }

    /// The value of the first header called `name` (any letter case), without surrounding blanks.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn parse(head: &str) -> Option<Self> {
        let mut lines = head.lines();
        let parts: Vec<&str> = lines.next()?.split_whitespace().collect();
        let [method, target, version] = parts[..] else {
            return None;
        };
        if !version.starts_with("HTTP/1.") || !target.starts_with('/') {
            return None;
        }
        let headers = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_owned(), value.trim().to_owned()))
            })
            .collect();
        Some(Self {
            method: method.to_owned(),
            target: target.to_owned(),
            headers,
        })
    }
}

/// What a handler answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    status: &'static str,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl Response {
    /// A response with the status text after `HTTP/1.1 `, for example `"200 OK"`, no headers and
    /// no body.
    #[must_use]
    pub fn new(status: &'static str) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Adds a header. A value with a line break is dropped (it would let a handler start a second
    /// header or response), and so are `Content-Length` and `Connection`, which the server sets.
    #[must_use]
    pub fn header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        let value = value.into();
        let reserved =
            name.eq_ignore_ascii_case("content-length") || name.eq_ignore_ascii_case("connection");
        let has_break = |text: &str| text.contains(['\r', '\n']);
        if !reserved && !has_break(name) && !has_break(&value) {
            self.headers.push((name, value));
        }
        self
    }

    /// Sets the body.
    #[must_use]
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
}

/// What a server calls for each well-formed request that is meant for it.
type Handler = dyn Fn(&Request) -> Response + Send + Sync;

/// State shared by the accepting thread and the connection threads.
struct Shared {
    handler: Box<Handler>,
    stopping: AtomicBool,
    active: AtomicUsize,
    own_names: Vec<String>,
}

/// Holds one of the [`MAX_CONNECTIONS`] places and gives it back however the thread ends.
struct Slot(Arc<Shared>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A running server. It stops when dropped or when [`Server::stop`] is called.
pub struct Server {
    addr: SocketAddr,
    shared: Arc<Shared>,
    accepting: Option<JoinHandle<()>>,
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// Starts serving on `bind` (port `0` picks a free port) with one thread called
    /// `hopswitch-<name>-...` per role. Every request that passes the checks in the module docs is
    /// given to `handler`.
    ///
    /// # Errors
    /// [`io::ErrorKind::InvalidInput`] when `bind` is not a loopback address, otherwise the I/O error
    /// when the port cannot be bound or the thread cannot be started.
    pub fn start(
        bind: SocketAddr,
        name: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> io::Result<Self> {
        if !bind.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} is not a loopback address; use an SSH tunnel to reach it",
                    bind.ip()
                ),
            ));
        }
        let listener = TcpListener::bind(bind)?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            handler: Box::new(handler),
            stopping: AtomicBool::new(false),
            active: AtomicUsize::new(0),
            own_names: vec![addr.to_string(), format!("localhost:{}", addr.port())],
        });
        let for_thread = Arc::clone(&shared);
        let accepting = thread::Builder::new()
            .name(format!("hopswitch-{name}-accept"))
            .spawn(move || accept_loop(&listener, &for_thread))?;
        Ok(Self {
            addr,
            shared,
            accepting: Some(accepting),
        })
    }

    /// The address the server listens on (always loopback).
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
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

impl Drop for Server {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>) {
    for incoming in listener.incoming() {
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = incoming else {
            // Out of descriptors or a reset while accepting: do not spin.
            thread::sleep(Duration::from_millis(50));
            continue;
        };
        let before = shared.active.fetch_add(1, Ordering::SeqCst);
        let slot = Slot(Arc::clone(shared));
        if before >= MAX_CONNECTIONS {
            continue; // `slot` is dropped here, which gives the place back
        }
        let spawned = thread::Builder::new()
            .name("hopswitch-http-conn".to_owned())
            .spawn(move || {
                serve(stream, &slot.0);
                drop(slot);
            });
        // When the thread cannot start, the closure (and with it the slot) is dropped.
        drop(spawned);
    }
}

fn serve(mut stream: TcpStream, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(CLIENT_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CLIENT_TIMEOUT));
    let head = match read_head(&mut stream) {
        Head::Complete(text) => text,
        Head::TooLarge => {
            send(
                &mut stream,
                &Response::new("431 Request Header Fields Too Large"),
                false,
            );
            return;
        }
        // Nothing arrived, or the client went away or stalled: there is nobody to answer.
        Head::Incomplete => return,
    };
    let Some(request) = Request::parse(&head) else {
        send(&mut stream, &Response::new("400 Bad Request"), false);
        return;
    };
    if request.header("host").is_some_and(|host| {
        !shared
            .own_names
            .iter()
            .any(|own| own.eq_ignore_ascii_case(host))
    }) {
        send(&mut stream, &Response::new("403 Forbidden"), false);
        return;
    }
    let response = panic::catch_unwind(AssertUnwindSafe(|| (shared.handler)(&request)))
        .unwrap_or_else(|_| Response::new("500 Internal Server Error"));
    send(&mut stream, &response, request.method() == "HEAD");
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

/// Writes the response (without the body for a `HEAD`) and ends the connection.
fn send(stream: &mut TcpStream, response: &Response, head_only: bool) {
    let mut reply = format!("HTTP/1.1 {}\r\n", response.status);
    for (name, value) in &response.headers {
        let _ = write!(reply, "{name}: {value}\r\n");
    }
    let _ = write!(
        reply,
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.body.len()
    );
    let mut reply = reply.into_bytes();
    if !head_only {
        reply.extend_from_slice(&response.body);
    }
    let _ = stream.write_all(&reply);
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

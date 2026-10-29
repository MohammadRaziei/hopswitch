//! `hopswitch serve`: the web API on a loopback port (ADR-0012 decision 5).
//!
//! The HTTP plumbing (loopback only, `Host` check against DNS rebinding, limits) is
//! [`hopswitch_http`]. What this module adds, because the API changes the network settings of the
//! machine:
//!
//! * **A token.** Every `/api/` request needs `Authorization: Bearer <token>`. The token is 256
//!   random bits from the operating system, kept in `web-token` in the settings folder (readable by
//!   its owner only on Unix), compared in constant time. A missing or damaged file is replaced.
//! * **No cross-site requests.** A request with an `Origin` header other than the server's own is
//!   refused, and no CORS header is ever sent, so a web page on another site can neither call nor
//!   read the API. The token is also given to the user after a `#`, which a browser never sends.
//! * **No route probing.** The token is checked before the path, so an unknown path does not say
//!   whether it exists.
//! * **Nothing cached.** Responses carry `Cache-Control: no-store`.

use std::fs;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use hopswitch_http::{Request, Response, Server};

use crate::app::{auto_run, run};
use crate::command::{ServeOptions, NOT_LOOPBACK};
use crate::{CliError, Context};

/// The token file inside the settings folder.
const TOKEN_FILE: &str = "web-token";
/// Random bytes in a token (256 bits).
const TOKEN_BYTES: usize = 32;
/// How long the in-server auto-connect loop waits before looking again when auto connect is off.
const AUTO_RETRY: Duration = Duration::from_secs(30);

/// A running web API. It stops when dropped or when [`WebServer::stop`] is called.
pub struct WebServer {
    server: Server,
    token: String,
}

impl std::fmt::Debug for WebServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token is a secret: it must not end up in a log or a failed assertion.
        f.debug_struct("WebServer")
            .field("addr", &self.server.addr())
            .finish_non_exhaustive()
    }
}

impl WebServer {
    /// The address the server listens on.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.server.addr()
    }

    /// The token every API request must present.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The address to open in a browser. The token follows a `#`, which browsers keep to themselves.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}/#token={}", self.server.addr(), self.token)
    }

    /// Stops listening.
    pub fn stop(self) {
        self.server.stop();
    }
}

/// Starts the web API on `options.ip:options.port` and returns without waiting.
///
/// # Errors
/// [`CliError::Usage`] for an address that is not loopback; [`CliError::Failure`] when the token
/// file cannot be read or written or the port cannot be used.
pub fn start_web(ctx: Arc<Context>, options: &ServeOptions) -> Result<WebServer, CliError> {
    if !options.ip.is_loopback() {
        return Err(CliError::usage(NOT_LOOPBACK));
    }
    let token = load_or_create_token(&ctx.dir)
        .map_err(|e| CliError::failure(format!("cannot prepare the web token: {e}")))?;
    let for_handler = token.clone();
    let bind = SocketAddr::new(options.ip, options.port);
    let server = Server::start(bind, "web", move |request: &Request| {
        handle(&ctx, &for_handler, request)
    })
    .map_err(|e| CliError::failure(format!("cannot listen on {bind}: {e}; try another --port")))?;
    Ok(WebServer { server, token })
}

/// What `hopswitch serve` does: starts the API, says where it is, runs the auto-connect loop
/// (unless `--no-auto`) and then keeps the process alive until it is stopped.
pub(crate) fn serve_forever(
    ctx: &Context,
    options: &ServeOptions,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let ctx = Arc::new(ctx.clone());
    let web = start_web(Arc::clone(&ctx), options)?;
    let port = web.addr().port();
    let lines = [
        format!("Hopswitch web API: {}", web.url()),
        format!("From another machine: ssh -L {port}:127.0.0.1:{port} user@this-server"),
        "Stop with Ctrl+C.".to_owned(),
    ];
    for line in lines {
        writeln!(out, "{line}").map_err(CliError::failure)?;
    }
    out.flush().map_err(CliError::failure)?;
    if options.auto {
        spawn_auto(ctx);
    }
    loop {
        thread::park();
    }
}

/// Runs the auto-connect loop for as long as the process lives. `auto_run` returns when auto
/// connect is off, so it is tried again later in case it was turned on meanwhile.
fn spawn_auto(ctx: Arc<Context>) {
    let _ = thread::Builder::new()
        .name("hopswitch-auto".to_owned())
        .spawn(move || loop {
            let _ = auto_run(&ctx, false, &mut io::sink());
            thread::sleep(AUTO_RETRY);
        });
}

fn handle(ctx: &Context, token: &str, request: &Request) -> Response {
    if !origin_is_own(request) {
        return Response::new("403 Forbidden");
    }
    if !request.path().starts_with("/api/") {
        return Response::new("404 Not Found");
    }
    if !is_authorized(request, token) {
        return Response::new("401 Unauthorized").header("WWW-Authenticate", "Bearer");
    }
    if !matches!(request.method(), "GET" | "HEAD") {
        return Response::new("405 Method Not Allowed").header("Allow", "GET, HEAD");
    }
    match request.path() {
        "/api/status" => status(ctx),
        _ => Response::new("404 Not Found"),
    }
}

/// A browser sends `Origin` on cross-site requests; it must name this very server. (The `Host`
/// header was already checked against the server's own names.)
fn origin_is_own(request: &Request) -> bool {
    let Some(origin) = request.header("origin") else {
        return true;
    };
    request
        .header("host")
        .is_some_and(|host| origin.eq_ignore_ascii_case(&format!("http://{host}")))
}

fn is_authorized(request: &Request, token: &str) -> bool {
    let Some((scheme, credentials)) = request
        .header("authorization")
        .and_then(|value| value.split_once(' '))
    else {
        return false;
    };
    scheme.eq_ignore_ascii_case("bearer")
        && constant_time_eq(credentials.trim().as_bytes(), token.as_bytes())
}

/// Compares without stopping at the first difference, so timing does not reveal how much of a
/// guess was right.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0_u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

fn status(ctx: &Context) -> Response {
    let mut text = Vec::new();
    if let Err(error) = run(&["status".to_owned()], ctx, &mut text) {
        return json(
            "500 Internal Server Error",
            &serde_json::json!({ "error": error.to_string() }),
        );
    }
    json(
        "200 OK",
        &serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "status": String::from_utf8_lossy(&text),
        }),
    )
}

fn json(status: &'static str, value: &serde_json::Value) -> Response {
    Response::new(status)
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(value.to_string())
}

/// Reads the token, or makes a new one when there is none or the file is not a token.
fn load_or_create_token(dir: &Path) -> io::Result<String> {
    fs::create_dir_all(dir)?;
    let path = dir.join(TOKEN_FILE);
    if let Ok(text) = fs::read_to_string(&path) {
        let existing = text.trim();
        if existing.len() == TOKEN_BYTES * 2 && existing.bytes().all(|b| b.is_ascii_hexdigit()) {
            restrict(&path)?;
            return Ok(existing.to_owned());
        }
    }
    let token = generate_token()?;
    write_private(&path, &token)?;
    Ok(token)
}

fn generate_token() -> io::Result<String> {
    use std::fmt::Write as _;
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::getrandom(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes
        .iter()
        .fold(String::with_capacity(TOKEN_BYTES * 2), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
}

#[cfg(unix)]
fn write_private(path: &Path, token: &str) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(token.as_bytes())?;
    drop(file);
    // `mode` only applies to a file that is new; an old one may have looser rights.
    restrict(path)
}

/// On Windows the file inherits the rights of the user's own folder.
#[cfg(not(unix))]
fn write_private(path: &Path, token: &str) -> io::Result<()> {
    fs::write(path, token)
}

#[cfg(unix)]
fn restrict(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode();
    if mode & 0o077 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn restrict(_path: &Path) -> io::Result<()> {
    Ok(())
}

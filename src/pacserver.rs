//! A loopback HTTP server that hands one PAC script to the system proxy settings (ADR-0011
//! decision 6), using only the standard library.
//!
//! The script is served at `http://127.0.0.1:<port>/hopswitch.pac`; the port is chosen by the
//! system, so the address is only known after [`PacServer::start`]. While a rules profile is applied
//! this server has to be running: PAC clients fetch the script from here.
//!
//! The protections of a web server on a machine that also runs browsers (loopback only, a `Host`
//! check against DNS rebinding so a web page cannot read the script, which lists the user's proxy
//! addresses, per-connection threads with timeouts, a header size limit) come from
//! [`crate::http`]; what is specific to this server is that it answers only `GET` and `HEAD`
//! for one path and that responses carry `Cache-Control: no-store`, so a changed profile is seen at
//! once.

use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, PoisonError, RwLock};

use crate::core::{PacConfig, PacEndpoint, PacHost, SystemError, HOPSWITCH_PAC_PATH};
use crate::http::{Request, Response, Server};

pub use crate::http::MAX_CONNECTIONS;

/// What PAC clients expect in `Content-Type`.
const CONTENT_TYPE: &str = "application/x-ns-proxy-autoconfig";

/// The script that later requests receive; replaced as a whole.
type SharedScript = Arc<RwLock<Arc<str>>>;

/// A running PAC server. It stops when dropped or when [`PacServer::stop`] is called.
pub struct PacServer {
    server: Server,
    url: PacConfig,
    script: SharedScript,
}

impl fmt::Debug for PacServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The script can be close to a megabyte; its size is what matters in a log.
        f.debug_struct("PacServer")
            .field("addr", &self.server.addr())
            .field("script_bytes", &snapshot(&self.script).len())
            .finish_non_exhaustive()
    }
}

fn snapshot(script: &SharedScript) -> Arc<str> {
    Arc::clone(&script.read().unwrap_or_else(PoisonError::into_inner))
}

/// Order: a method we serve (405), then our path (404). The malformed (400) and foreign-`Host`
/// (403) cases never get this far.
fn answer(request: &Request, script: &SharedScript) -> Response {
    if !matches!(request.method(), "GET" | "HEAD") {
        return Response::new("405 Method Not Allowed").header("Allow", "GET, HEAD");
    }
    if request.path() != HOPSWITCH_PAC_PATH {
        return Response::new("404 Not Found");
    }
    Response::new("200 OK")
        .header("Content-Type", CONTENT_TYPE)
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(snapshot(script).as_bytes())
}

impl PacServer {
    /// Starts serving `script` on a free port of the loopback interface.
    ///
    /// # Errors
    /// The I/O error when the port cannot be bound or the thread cannot be started.
    pub fn start(script: impl Into<String>) -> io::Result<Self> {
        let script: SharedScript = Arc::new(RwLock::new(Arc::from(script.into())));
        let for_handler = Arc::clone(&script);
        let server = Server::start(
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            "pac",
            move |request: &Request| answer(request, &for_handler),
        )?;
        let url = PacConfig::loopback(server.addr().port());
        Ok(Self {
            server,
            url,
            script,
        })
    }

    /// The address the server listens on (always loopback).
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.server.addr()
    }

    /// The address to give the system as its PAC script address.
    #[must_use]
    pub fn url(&self) -> PacConfig {
        self.url.clone()
    }

    /// Replaces the script that later requests receive; the address stays the same.
    pub fn set_script(&self, script: impl Into<String>) {
        *self.script.write().unwrap_or_else(PoisonError::into_inner) = Arc::from(script.into());
    }

    /// Stops listening and waits for the accepting thread to end. Connections already being
    /// answered finish on their own.
    pub fn stop(self) {
        self.server.stop();
    }
}

/// The real [`PacHost`]: every [`PacHost::start`] runs a [`PacServer`] on a free loopback port.
#[derive(Debug, Clone, Copy, Default)]
pub struct PacServerHost;

impl PacEndpoint for PacServer {
    fn url(&self) -> PacConfig {
        PacServer::url(self)
    }

    fn set_script(&self, script: &str) {
        PacServer::set_script(self, script);
    }
}

impl PacHost for PacServerHost {
    fn start(&self, script: &str) -> Result<Box<dyn PacEndpoint>, SystemError> {
        PacServer::start(script)
            .map(|server| Box::new(server) as Box<dyn PacEndpoint>)
            .map_err(|error| SystemError::CommandFailed {
                command: "start the PAC server".to_owned(),
                details: error.to_string(),
            })
    }
}

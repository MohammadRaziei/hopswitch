//! Socket helpers that share one overall deadline across several steps.

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// The moment by which a whole probe must be finished.
#[derive(Debug, Clone, Copy)]
pub struct Deadline(Instant);

impl Deadline {
    pub fn after(timeout: Duration) -> Self {
        Self(Instant::now() + timeout)
    }

    /// Time left, or a timeout error once it has run out.
    pub fn remaining(self) -> io::Result<Duration> {
        self.0
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "probe timed out"))
    }
}

pub fn protocol_error(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

/// Connects to the first address of `host:port` that answers in time.
pub fn connect(host: &str, port: u16, deadline: Deadline) -> io::Result<TcpStream> {
    let mut last = protocol_error("no address for host");
    for addr in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, deadline.remaining()?) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn arm(stream: &TcpStream, deadline: Deadline) -> io::Result<()> {
    let left = deadline.remaining()?;
    stream.set_read_timeout(Some(left))?;
    stream.set_write_timeout(Some(left))
}

pub fn write_all(stream: &mut TcpStream, data: &[u8], deadline: Deadline) -> io::Result<()> {
    arm(stream, deadline)?;
    stream.write_all(data)
}

pub fn read_exact(stream: &mut TcpStream, buf: &mut [u8], deadline: Deadline) -> io::Result<()> {
    arm(stream, deadline)?;
    stream.read_exact(buf)
}

/// Reads the HTTP status code from the first line of a response.
pub fn read_status(stream: &mut TcpStream, deadline: Deadline) -> io::Result<u16> {
    const MAX_LINE: usize = 1024;
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while line.len() < MAX_LINE {
        read_exact(stream, &mut byte, deadline)?;
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&line);
    let mut parts = text.split_whitespace();
    match (parts.next(), parts.next().map(str::parse::<u16>)) {
        (Some(version), Some(Ok(code))) if version.starts_with("HTTP/") => Ok(code),
        _ => Err(protocol_error("not an HTTP response")),
    }
}

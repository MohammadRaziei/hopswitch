//! A minimal DNS client: one `A` query over UDP, answer checked by id and code.

use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::probe::net::{protocol_error, Deadline};

/// Size of a DNS message header in bytes (RFC 1035).
const HEADER_LEN: usize = 12;
/// Longest single label of a domain name (RFC 1035).
const MAX_LABEL: usize = 63;
/// Longest domain name in text form (RFC 1035).
const MAX_NAME: usize = 253;

/// Builds a standard recursive `A` query for `domain` (a trailing dot is allowed).
/// Returns `None` for names that cannot be encoded.
pub fn build_query(id: u16, domain: &str) -> Option<Vec<u8>> {
    let name = domain.strip_suffix('.').unwrap_or(domain);
    if name.is_empty() || name.len() > MAX_NAME {
        return None;
    }
    let mut packet = Vec::with_capacity(HEADER_LEN + name.len() + 6);
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.') {
        if label.is_empty() || label.len() > MAX_LABEL {
            return None;
        }
        packet.push(u8::try_from(label.len()).ok()?);
        packet.extend(label.bytes().map(|b| b.to_ascii_lowercase()));
    }
    packet.extend_from_slice(&[0, 0, 1, 0, 1]);
    Some(packet)
}

/// True when `packet` is a successful answer (response, no error, at least one
/// record) to the query that had `query_id`.
pub fn is_answer(query_id: u16, packet: &[u8]) -> bool {
    if packet.len() < HEADER_LEN || packet[..2] != query_id.to_be_bytes() {
        return false;
    }
    let is_response = packet[2] & 0x80 != 0;
    // The response code is the low nibble of byte 3; zero means no error.
    let no_error = packet[3].trailing_zeros() >= 4;
    let answers = u16::from_be_bytes([packet[6], packet[7]]);
    is_response && no_error && answers > 0
}

fn next_id() -> u16 {
    static COUNTER: AtomicU16 = AtomicU16::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    // Truncation is the point: only 16 bits of entropy are needed.
    #[allow(clippy::cast_possible_truncation)]
    let mixed = (nanos as u16) ^ COUNTER.fetch_add(0x9E37, Ordering::Relaxed);
    mixed
}

pub fn probe(server: IpAddr, port: u16, domain: &str, deadline: Deadline) -> io::Result<()> {
    let id = next_id();
    let query = build_query(id, domain).ok_or_else(|| protocol_error("unusable domain"))?;
    let bind = if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = UdpSocket::bind(bind)?;
    socket.connect(SocketAddr::new(server, port))?;
    socket.send(&query)?;

    let mut buf = [0u8; 1500];
    loop {
        socket.set_read_timeout(Some(deadline.remaining()?))?;
        let n = socket.recv(&mut buf)?;
        if is_answer(id, &buf[..n]) {
            return Ok(());
        }
        // A packet with a different id is stray; an error answer for our id is final.
        if buf[..n].starts_with(&id.to_be_bytes()) {
            return Err(protocol_error("the server answered with an error"));
        }
    }
}

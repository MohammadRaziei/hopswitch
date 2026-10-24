//! Fetching the target through an HTTP, `SOCKS4a` or SOCKS5 proxy.

use std::io;
use std::net::{IpAddr, TcpStream};

use hopswitch_core::{ProxyConfig, ProxyScheme};

use crate::net::{connect, protocol_error, read_exact, read_status, write_all, Deadline};
use crate::target::Target;

pub fn probe(config: &ProxyConfig, target: &str, deadline: Deadline) -> io::Result<()> {
    let target = Target::parse(target).ok_or_else(|| protocol_error("unusable target URL"))?;
    if config.scheme() == ProxyScheme::Https {
        return Err(protocol_error("proxies reached over TLS are not supported"));
    }
    let mut stream = connect(config.host(), config.port(), deadline)?;
    match config.scheme() {
        ProxyScheme::Http | ProxyScheme::Https => http_proxy(&mut stream, &target, deadline),
        ProxyScheme::Socks5 => {
            socks5(&mut stream, &target, deadline)?;
            finish(&mut stream, &target, deadline)
        }
        ProxyScheme::Socks4 => {
            socks4(&mut stream, &target, deadline)?;
            finish(&mut stream, &target, deadline)
        }
    }
}

fn http_proxy(stream: &mut TcpStream, target: &Target, deadline: Deadline) -> io::Result<()> {
    let authority = target.authority();
    let request = if target.https {
        let hostport = format!("{}:{}", bracketed(&target.host), target.port);
        format!("CONNECT {hostport} HTTP/1.1\r\nHost: {hostport}\r\n\r\n")
    } else {
        format!(
            "GET http://{authority}{} HTTP/1.1\r\nHost: {authority}\r\n\
             User-Agent: hopswitch-probe\r\nConnection: close\r\n\r\n",
            target.path
        )
    };
    write_all(stream, request.as_bytes(), deadline)?;
    expect_status(read_status(stream, deadline)?)
}

/// After a SOCKS tunnel is open: an `https://` target is done, plain HTTP is fetched.
fn finish(stream: &mut TcpStream, target: &Target, deadline: Deadline) -> io::Result<()> {
    if target.https {
        return Ok(());
    }
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: hopswitch-probe\r\nConnection: close\r\n\r\n",
        target.path,
        target.authority()
    );
    write_all(stream, request.as_bytes(), deadline)?;
    expect_status(read_status(stream, deadline)?)
}

fn expect_status(code: u16) -> io::Result<()> {
    if (200..400).contains(&code) {
        Ok(())
    } else {
        Err(protocol_error(
            "the proxy or target returned an error status",
        ))
    }
}

fn bracketed(host: &str) -> String {
    if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}

fn socks5(stream: &mut TcpStream, target: &Target, deadline: Deadline) -> io::Result<()> {
    write_all(stream, &[5, 1, 0], deadline)?;
    let mut choice = [0u8; 2];
    read_exact(stream, &mut choice, deadline)?;
    if choice != [5, 0] {
        return Err(protocol_error("SOCKS5 proxy wants authentication"));
    }

    let mut request = vec![5, 1, 0];
    match target.host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            request.push(1);
            request.extend_from_slice(&ip.octets());
        }
        Ok(IpAddr::V6(ip)) => {
            request.push(4);
            request.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            let len =
                u8::try_from(target.host.len()).map_err(|_| protocol_error("host too long"))?;
            request.push(3);
            request.push(len);
            request.extend_from_slice(target.host.as_bytes());
        }
    }
    request.extend_from_slice(&target.port.to_be_bytes());
    write_all(stream, &request, deadline)?;

    let mut head = [0u8; 4];
    read_exact(stream, &mut head, deadline)?;
    if head[0] != 5 || head[1] != 0 {
        return Err(protocol_error("SOCKS5 proxy refused the connection"));
    }
    let bound_len = match head[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut len = [0u8; 1];
            read_exact(stream, &mut len, deadline)?;
            usize::from(len[0])
        }
        _ => return Err(protocol_error("SOCKS5 reply has an unknown address type")),
    };
    let mut rest = vec![0u8; bound_len + 2];
    read_exact(stream, &mut rest, deadline)
}

fn socks4(stream: &mut TcpStream, target: &Target, deadline: Deadline) -> io::Result<()> {
    let mut request = vec![4, 1];
    request.extend_from_slice(&target.port.to_be_bytes());
    match target.host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            request.extend_from_slice(&ip.octets());
            request.push(0);
        }
        Ok(IpAddr::V6(_)) => return Err(protocol_error("SOCKS4 cannot reach IPv6 hosts")),
        Err(_) => {
            // SOCKS4a: the invalid address 0.0.0.1 asks the proxy to resolve the name.
            request.extend_from_slice(&[0, 0, 0, 1, 0]);
            request.extend_from_slice(target.host.as_bytes());
            request.push(0);
        }
    }
    write_all(stream, &request, deadline)?;
    let mut reply = [0u8; 8];
    read_exact(stream, &mut reply, deadline)?;
    if reply[1] == 0x5A {
        Ok(())
    } else {
        Err(protocol_error("SOCKS4 proxy rejected the request"))
    }
}

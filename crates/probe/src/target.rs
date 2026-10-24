//! The URL a proxy probe fetches, split into the parts the protocols need.

/// A parsed `http://` or `https://` URL (no credentials, no fragment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub https: bool,
    /// Host without brackets, as written.
    pub host: String,
    pub port: u16,
    /// Path and query, always starting with `/`.
    pub path: String,
}

impl Target {
    pub fn parse(url: &str) -> Option<Self> {
        let lower = url.to_ascii_lowercase();
        let (https, offset) = if lower.starts_with("https://") {
            (true, 8)
        } else if lower.starts_with("http://") {
            (false, 7)
        } else {
            return None;
        };
        let rest = &url[offset..];
        if rest
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '@')
        {
            return None;
        }
        let rest = rest.split('#').next().unwrap_or_default();
        let split = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(split);
        let path = match tail {
            "" => "/".to_owned(),
            t if t.starts_with('?') => format!("/{t}"),
            t => t.to_owned(),
        };
        let default_port = if https { 443 } else { 80 };
        let (host, port) = split_authority(authority, default_port)?;
        Some(Self {
            https,
            host,
            port,
            path,
        })
    }

    /// `host` or `host:port`, bracketed for IPv6; the form used in `Host:` headers.
    pub fn authority(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let default = if self.https { 443 } else { 80 };
        if self.port == default {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

fn split_authority(authority: &str, default_port: u16) -> Option<(String, u16)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None if after.is_empty() => default_port,
            None => return None,
        };
        return (!host.is_empty()).then(|| (host.to_owned(), port));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, p)) => (host, p.parse().ok()?),
        None => (authority, default_port),
    };
    (!host.is_empty() && port != 0).then(|| (host.to_owned(), port))
}

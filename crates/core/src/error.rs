//! Validation errors shared by every domain type.

use std::net::IpAddr;

use thiserror::Error;

/// Reason a domain value was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    /// The proxy host is empty or whitespace only.
    #[error("proxy host is empty")]
    EmptyHost,
    /// The proxy host is neither an IP address nor a valid hostname.
    #[error("invalid proxy host: {0}")]
    InvalidHost(String),
    /// The port is zero or out of the `1..=65535` range.
    #[error("port must be between 1 and 65535")]
    InvalidPort,
    /// The proxy scheme is not supported.
    #[error("unsupported proxy scheme: {0}")]
    UnsupportedScheme(String),
    /// The proxy URL could not be parsed.
    #[error("malformed proxy url: {0}")]
    MalformedUrl(String),
    /// A bypass-list entry is empty.
    #[error("bypass entries must not be empty")]
    InvalidBypassEntry,
    /// No DNS server was provided.
    #[error("at least one DNS server is required")]
    NoDnsServers,
    /// Too many DNS servers were provided.
    #[error("too many DNS servers (max {max})")]
    TooManyDnsServers {
        /// Maximum number of servers allowed.
        max: usize,
    },
    /// The same DNS server appears twice.
    #[error("duplicate DNS server: {0}")]
    DuplicateDnsServer(IpAddr),
    /// The DNS server is not a usable IP address.
    #[error("invalid DNS server address: {0}")]
    InvalidDnsServer(String),
    /// A proxy group has no members.
    #[error("a group needs at least one member")]
    NoGroupMembers,
    /// The same member appears twice in a group.
    #[error("duplicate group member: {0}")]
    DuplicateGroupMember(String),
    /// The profile id is empty.
    #[error("profile id is empty")]
    EmptyId,
    /// The profile name is empty.
    #[error("profile name is empty")]
    EmptyName,
    /// The profile name exceeds the allowed length.
    #[error("profile name is too long (max {max} characters)")]
    NameTooLong {
        /// Maximum number of characters allowed.
        max: usize,
    },
}

//! Behavioural tests for `DnsConfig`.

use std::net::IpAddr;

use hopswitch::core::{DnsConfig, ValidationError, MAX_DNS_SERVERS};

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[test]
fn creates_a_valid_configuration_and_keeps_order() {
    let cfg = DnsConfig::new(vec![ip("9.9.9.9"), ip("1.1.1.1")]).unwrap();
    assert_eq!(cfg.servers(), [ip("9.9.9.9"), ip("1.1.1.1")]);
}

#[test]
fn rejects_an_empty_list() {
    assert_eq!(
        DnsConfig::new(vec![]).unwrap_err(),
        ValidationError::NoDnsServers
    );
}

#[test]
fn rejects_duplicates() {
    let err = DnsConfig::new(vec![ip("1.1.1.1"), ip("1.1.1.1")]).unwrap_err();
    assert_eq!(err, ValidationError::DuplicateDnsServer(ip("1.1.1.1")));
}

#[test]
fn rejects_too_many_servers() {
    let servers = (1..=5).map(|n| ip(&format!("10.0.0.{n}"))).collect();
    let err = DnsConfig::new(servers).unwrap_err();
    assert_eq!(
        err,
        ValidationError::TooManyDnsServers {
            max: MAX_DNS_SERVERS
        }
    );
}

#[test]
fn rejects_unspecified_addresses() {
    let err = DnsConfig::new(vec![ip("0.0.0.0")]).unwrap_err();
    assert_eq!(err, ValidationError::InvalidDnsServer("0.0.0.0".into()));
}

#[test]
fn parses_mixed_ipv4_and_ipv6_text() {
    let cfg = DnsConfig::from_strs([" 1.1.1.1 ", "2606:4700:4700::1111"]).unwrap();
    assert_eq!(cfg.servers(), [ip("1.1.1.1"), ip("2606:4700:4700::1111")]);
}

#[test]
fn parsing_rejects_garbage() {
    let err = DnsConfig::from_strs(["not-an-ip"]).unwrap_err();
    assert_eq!(err, ValidationError::InvalidDnsServer("not-an-ip".into()));
}

#[test]
fn serialises_as_a_plain_array() {
    let cfg = DnsConfig::from_strs(["1.1.1.1", "8.8.8.8"]).unwrap();
    assert_eq!(
        serde_json::to_string(&cfg).unwrap(),
        r#"["1.1.1.1","8.8.8.8"]"#
    );
}

#[test]
fn deserialising_an_empty_array_is_rejected() {
    assert!(serde_json::from_str::<DnsConfig>("[]").is_err());
}

#[test]
fn serde_round_trip_preserves_the_value() {
    let cfg = DnsConfig::from_strs(["1.1.1.1", "::1"]).unwrap();
    let back: DnsConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
    assert_eq!(back, cfg);
}

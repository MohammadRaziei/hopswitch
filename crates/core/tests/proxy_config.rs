//! Behavioural tests for `ProxyConfig` and `ProxyScheme`.

use hopswitch_core::{ProxyConfig, ProxyScheme, ValidationError};

#[test]
fn creates_a_valid_proxy() {
    let p = ProxyConfig::new(ProxyScheme::Http, "127.0.0.1", 8080).unwrap();

    assert_eq!(p.scheme(), ProxyScheme::Http);
    assert_eq!(p.host(), "127.0.0.1");
    assert_eq!(p.port(), 8080);
    assert!(p.bypass().is_empty());
}

#[test]
fn rejects_an_empty_host() {
    let err = ProxyConfig::new(ProxyScheme::Socks5, "   ", 1080).unwrap_err();
    assert_eq!(err, ValidationError::EmptyHost);
}

#[test]
fn rejects_port_zero() {
    let err = ProxyConfig::new(ProxyScheme::Http, "localhost", 0).unwrap_err();
    assert_eq!(err, ValidationError::InvalidPort);
}

#[test]
fn rejects_hosts_with_illegal_characters() {
    for bad in [
        "bad host",
        "a/b",
        "-bad.example.com",
        "bad-.example.com",
        "a..b",
    ] {
        let err = ProxyConfig::new(ProxyScheme::Http, bad, 80).unwrap_err();
        assert_eq!(
            err,
            ValidationError::InvalidHost(bad.to_owned()),
            "host: {bad}"
        );
    }
}

#[test]
fn normalises_hostnames_to_lowercase() {
    let p = ProxyConfig::new(ProxyScheme::Https, "Proxy.Example.COM", 443).unwrap();
    assert_eq!(p.host(), "proxy.example.com");
}

#[test]
fn accepts_ipv6_hosts() {
    let p = ProxyConfig::new(ProxyScheme::Socks5, "::1", 1080).unwrap();
    assert_eq!(p.host(), "::1");
}

#[test]
fn bypass_entries_are_trimmed() {
    let p = ProxyConfig::new(ProxyScheme::Http, "localhost", 8080)
        .unwrap()
        .with_bypass(vec![" localhost ".into(), "*.local".into()])
        .unwrap();
    assert_eq!(p.bypass(), ["localhost", "*.local"]);
}

#[test]
fn rejects_empty_bypass_entries() {
    let err = ProxyConfig::new(ProxyScheme::Http, "localhost", 8080)
        .unwrap()
        .with_bypass(vec!["  ".into()])
        .unwrap_err();
    assert_eq!(err, ValidationError::InvalidBypassEntry);
}

#[test]
fn parses_a_url() {
    let p: ProxyConfig = "socks5://127.0.0.1:1080".parse().unwrap();
    assert_eq!(p.scheme(), ProxyScheme::Socks5);
    assert_eq!(p.host(), "127.0.0.1");
    assert_eq!(p.port(), 1080);
}

#[test]
fn parses_a_url_with_an_ipv6_host() {
    let p: ProxyConfig = "http://[::1]:8080".parse().unwrap();
    assert_eq!(p.host(), "::1");
    assert_eq!(p.port(), 8080);
}

#[test]
fn parsing_is_case_insensitive_on_scheme_and_tolerates_trailing_slash() {
    let p: ProxyConfig = "HTTP://localhost:3128/".parse().unwrap();
    assert_eq!(p.scheme(), ProxyScheme::Http);
    assert_eq!(p.port(), 3128);
}

#[test]
fn parsing_rejects_unsupported_schemes() {
    let err = "ftp://example.com:21".parse::<ProxyConfig>().unwrap_err();
    assert_eq!(err, ValidationError::UnsupportedScheme("ftp".into()));
}

#[test]
fn parsing_rejects_malformed_urls() {
    for bad in [
        "localhost:8080",
        "http://localhost",
        "http://h:abc",
        "http://[::1]",
    ] {
        let err = bad.parse::<ProxyConfig>().unwrap_err();
        assert_eq!(
            err,
            ValidationError::MalformedUrl(bad.to_owned()),
            "input: {bad}"
        );
    }
}

#[test]
fn parsing_rejects_out_of_range_ports() {
    for bad in ["http://h:0", "http://h:70000"] {
        let err = bad.parse::<ProxyConfig>().unwrap_err();
        assert_eq!(err, ValidationError::InvalidPort, "input: {bad}");
    }
}

#[test]
fn display_round_trips_through_parse() {
    for url in [
        "socks5://127.0.0.1:1080",
        "http://[::1]:8080",
        "https://proxy.example.com:443",
    ] {
        let p: ProxyConfig = url.parse().unwrap();
        assert_eq!(p.to_string(), url);
    }
}

#[test]
fn serde_round_trip_preserves_the_value() {
    let p = ProxyConfig::new(ProxyScheme::Socks5, "localhost", 1080)
        .unwrap()
        .with_bypass(vec!["*.local".into()])
        .unwrap();

    let json = serde_json::to_string(&p).unwrap();
    let back: ProxyConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(back, p);
}

#[test]
fn deserialising_invalid_data_is_rejected() {
    let json = r#"{"scheme":"http","host":"localhost","port":0,"bypass":[]}"#;
    assert!(serde_json::from_str::<ProxyConfig>(json).is_err());
}

#[test]
fn bypass_defaults_to_empty_when_missing_in_json() {
    let json = r#"{"scheme":"http","host":"localhost","port":8080}"#;
    let p: ProxyConfig = serde_json::from_str(json).unwrap();
    assert!(p.bypass().is_empty());
}

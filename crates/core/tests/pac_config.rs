//! Behaviour of `PacConfig` and `ProxyState::Pac` (ADR-0011 prerequisite 9).

use hopswitch_core::{PacConfig, ProxyState, ValidationError};

#[test]
fn accepts_http_https_and_file_urls() {
    for url in [
        "http://127.0.0.1:8123/hopswitch.pac",
        "https://example.com/proxy.pac",
        "file:///C:/proxy.pac",
    ] {
        let pac = PacConfig::new(url).expect(url);
        assert_eq!(pac.url(), url);
    }
}

#[test]
fn the_scheme_is_case_insensitive_but_the_url_is_kept_as_given() {
    let pac = PacConfig::new("HTTP://Example.com/P.pac").unwrap();
    assert_eq!(pac.url(), "HTTP://Example.com/P.pac");
}

#[test]
fn rejects_an_empty_url() {
    assert_eq!(
        PacConfig::new("").unwrap_err(),
        ValidationError::InvalidPacUrl(String::new())
    );
}

#[test]
fn rejects_other_schemes_and_bare_text() {
    for bad in [
        "ftp://host/p.pac",
        "javascript:alert(1)",
        "wpad",
        "//host/p.pac",
    ] {
        assert_eq!(
            PacConfig::new(bad).unwrap_err(),
            ValidationError::InvalidPacUrl(bad.to_owned()),
            "{bad}"
        );
    }
}

#[test]
fn rejects_a_url_without_anything_after_the_scheme() {
    for bad in ["http://", "https://", "file://"] {
        assert!(PacConfig::new(bad).is_err(), "{bad}");
    }
}

#[test]
fn rejects_whitespace_and_control_characters() {
    for bad in [
        " http://a/p.pac",
        "http://a/p.pac ",
        "http://a/ p.pac",
        "http://a/p.pac\n",
        "http://a/p\u{0}.pac",
    ] {
        assert!(PacConfig::new(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn rejects_a_url_longer_than_2048_characters() {
    let long = format!("http://a/{}", "x".repeat(2048));
    assert!(PacConfig::new(&long).is_err());
    let limit = format!("http://a/{}", "x".repeat(2048 - "http://a/".len()));
    assert_eq!(limit.len(), 2048);
    assert!(PacConfig::new(&limit).is_ok());
}

#[test]
fn displays_as_its_url() {
    let pac = PacConfig::new("http://127.0.0.1:1/p.pac").unwrap();
    assert_eq!(pac.to_string(), "http://127.0.0.1:1/p.pac");
}

#[test]
fn a_pac_state_round_trips_through_json() {
    let state = ProxyState::Pac(PacConfig::new("http://127.0.0.1:8123/p.pac").unwrap());
    let json = serde_json::to_string(&state).unwrap();
    assert_eq!(json, r#"{"pac":{"url":"http://127.0.0.1:8123/p.pac"}}"#);
    assert_eq!(serde_json::from_str::<ProxyState>(&json).unwrap(), state);
}

#[test]
fn a_stored_pac_state_with_an_invalid_url_is_refused() {
    let json = r#"{"pac":{"url":"not a url"}}"#;
    assert!(serde_json::from_str::<ProxyState>(json).is_err());
}

#[test]
fn a_pac_state_is_not_equal_to_the_other_states() {
    let pac = ProxyState::Pac(PacConfig::new("http://a/p.pac").unwrap());
    assert_ne!(pac, ProxyState::Disabled);
    assert_ne!(
        pac,
        ProxyState::Other {
            raw: String::new(),
            enabled: true
        }
    );
}

#[test]
fn parses_from_text_like_a_proxy_address_does() {
    let pac: PacConfig = "http://127.0.0.1:8123/p.pac".parse().unwrap();
    assert_eq!(pac.url(), "http://127.0.0.1:8123/p.pac");

    let err = "wpad".parse::<PacConfig>().unwrap_err();
    assert_eq!(err, ValidationError::InvalidPacUrl("wpad".to_owned()));
}

// ---------- recognising the address of Hopswitch's own PAC server ----------

#[test]
fn the_loopback_address_of_the_hopswitch_server_is_built_from_a_port() {
    let pac = PacConfig::loopback(8123);
    assert_eq!(pac.url(), "http://127.0.0.1:8123/hopswitch.pac");
    assert_eq!(pac.hopswitch_port(), Some(8123));
}

#[test]
fn only_the_exact_address_of_the_hopswitch_server_is_recognised() {
    let own = [
        ("http://127.0.0.1:1/hopswitch.pac", 1),
        ("http://127.0.0.1:65535/hopswitch.pac", 65535),
        ("HTTP://127.0.0.1:49152/hopswitch.pac", 49152),
    ];
    for (url, port) in own {
        assert_eq!(
            PacConfig::new(url).unwrap().hopswitch_port(),
            Some(port),
            "{url}"
        );
    }

    let not_own = [
        "http://localhost:8123/hopswitch.pac",
        "http://127.0.0.2:8123/hopswitch.pac",
        "http://[::1]:8123/hopswitch.pac",
        "https://127.0.0.1:8123/hopswitch.pac",
        "file:///hopswitch.pac",
        "http://127.0.0.1/hopswitch.pac",
        "http://127.0.0.1:0/hopswitch.pac",
        "http://127.0.0.1:65536/hopswitch.pac",
        "http://127.0.0.1:+80/hopswitch.pac",
        "http://127.0.0.1:80a/hopswitch.pac",
        "http://127.0.0.1:8123/other.pac",
        "http://127.0.0.1:8123/hopswitch.pac/",
        "http://127.0.0.1:8123/hopswitch.pac?x=1",
        "http://127.0.0.1:8123/hopswitch.pac#x",
        "http://127.0.0.1:8123",
        "http://user@127.0.0.1:8123/hopswitch.pac",
        "http://127.0.0.1@evil.example:8123/hopswitch.pac",
        "http://wpad.corp/hopswitch.pac",
    ];
    for url in not_own {
        assert_eq!(PacConfig::new(url).unwrap().hopswitch_port(), None, "{url}");
    }
}

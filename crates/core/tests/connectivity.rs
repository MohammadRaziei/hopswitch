//! Validation of the connectivity-test settings (what to test against, and how often).

use hopswitch_core::ConnectivitySettings;

#[test]
fn defaults_are_valid_and_sensible() {
    let s = ConnectivitySettings::default();

    assert!(s.proxy_target().starts_with("http://"));
    assert_ne!(s.dns_domain(), "");
    assert_eq!(s.timeout_ms(), 3000);
    assert_eq!(s.interval_secs(), 30);
    assert!(ConnectivitySettings::new(s.proxy_target(), s.dns_domain(), 3000, 30).is_ok());
}

#[test]
fn proxy_target_must_be_a_plain_http_or_https_url() {
    for bad in [
        "",
        "   ",
        "ftp://x.test/",
        "example.com",
        "http://",
        "http://a b/",
    ] {
        assert!(
            ConnectivitySettings::new(bad, "example.com", 3000, 30).is_err(),
            "{bad:?}"
        );
    }
    let ok = ConnectivitySettings::new("  HTTPS://probe.example/ping  ", "example.com", 3000, 30);
    assert_eq!(ok.unwrap().proxy_target(), "HTTPS://probe.example/ping");
}

#[test]
fn dns_domain_must_look_like_a_hostname() {
    for bad in [
        "",
        "http://example.com",
        "exa mple.com",
        "-bad.com",
        "a..b",
        "x/y",
    ] {
        assert!(
            ConnectivitySettings::new("http://a.test/", bad, 3000, 30).is_err(),
            "{bad:?}"
        );
    }
    assert!(ConnectivitySettings::new("http://a.test/", "Example.COM.", 3000, 30).is_ok());
}

#[test]
fn timeout_and_interval_have_sane_bounds() {
    let make = |t, i| ConnectivitySettings::new("http://a.test/", "example.com", t, i);

    assert!(make(499, 30).is_err());
    assert!(make(500, 30).is_ok());
    assert!(make(10_000, 30).is_ok());
    assert!(make(10_001, 30).is_err());
    assert!(make(3000, 4).is_err());
    assert!(make(3000, 5).is_ok());
    assert!(make(3000, 3600).is_ok());
    assert!(make(3000, 3601).is_err());
}

#[test]
fn loading_from_json_revalidates() {
    let bad = r#"{"proxy_target":"nope","dns_domain":"example.com","timeout_ms":3000,"interval_secs":30}"#;
    assert!(serde_json::from_str::<ConnectivitySettings>(bad).is_err());

    let good = serde_json::to_string(&ConnectivitySettings::default()).unwrap();
    let back: ConnectivitySettings = serde_json::from_str(&good).unwrap();
    assert_eq!(back, ConnectivitySettings::default());
}

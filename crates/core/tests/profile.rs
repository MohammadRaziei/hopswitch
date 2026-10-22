//! Behavioural tests for `ProxyProfile` and `DnsProfile`.

use hopswitch_core::{
    DnsConfig, DnsProfile, ProxyConfig, ProxyProfile, ProxyScheme, ValidationError, MAX_NAME_CHARS,
};

fn proxy() -> ProxyConfig {
    ProxyConfig::new(ProxyScheme::Socks5, "127.0.0.1", 1080).unwrap()
}

fn dns() -> DnsConfig {
    DnsConfig::from_strs(["1.1.1.1"]).unwrap()
}

#[test]
fn creates_a_proxy_profile() {
    let p = ProxyProfile::new("p1", "Work", proxy()).unwrap();

    assert_eq!(p.id(), "p1");
    assert_eq!(p.name(), "Work");
    assert_eq!(p.config(), &proxy());
}

#[test]
fn creates_a_dns_profile() {
    let p = DnsProfile::new("d1", "Cloudflare", dns()).unwrap();

    assert_eq!(p.id(), "d1");
    assert_eq!(p.name(), "Cloudflare");
    assert_eq!(p.config(), &dns());
}

#[test]
fn trims_the_name() {
    assert_eq!(
        ProxyProfile::new("p1", "  Work  ", proxy()).unwrap().name(),
        "Work"
    );
    assert_eq!(
        DnsProfile::new("d1", "  Home ", dns()).unwrap().name(),
        "Home"
    );
}

#[test]
fn rejects_empty_names_and_ids() {
    assert_eq!(
        ProxyProfile::new("p1", "   ", proxy()).unwrap_err(),
        ValidationError::EmptyName
    );
    assert_eq!(
        ProxyProfile::new(" ", "Work", proxy()).unwrap_err(),
        ValidationError::EmptyId
    );
    assert_eq!(
        DnsProfile::new("d1", "", dns()).unwrap_err(),
        ValidationError::EmptyName
    );
    assert_eq!(
        DnsProfile::new("", "Home", dns()).unwrap_err(),
        ValidationError::EmptyId
    );
}

#[test]
fn name_length_is_counted_in_characters_not_bytes() {
    let persian_ok = "پ".repeat(MAX_NAME_CHARS);
    assert!(ProxyProfile::new("p1", persian_ok.clone(), proxy()).is_ok());
    assert!(DnsProfile::new("d1", persian_ok, dns()).is_ok());

    let too_long = "پ".repeat(MAX_NAME_CHARS + 1);
    let expected = ValidationError::NameTooLong {
        max: MAX_NAME_CHARS,
    };
    assert_eq!(
        ProxyProfile::new("p1", too_long.clone(), proxy()).unwrap_err(),
        expected
    );
    assert_eq!(
        DnsProfile::new("d1", too_long, dns()).unwrap_err(),
        expected
    );
}

#[test]
fn serde_round_trip_preserves_the_value() {
    let p = ProxyProfile::new("p1", "Work", proxy()).unwrap();
    let back: ProxyProfile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back, p);

    let d = DnsProfile::new("d1", "Home", dns()).unwrap();
    let back: DnsProfile = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    assert_eq!(back, d);
}

#[test]
fn deserialising_an_invalid_profile_is_rejected() {
    let empty_name = r#"{"id":"d1","name":"","config":["1.1.1.1"]}"#;
    assert!(serde_json::from_str::<DnsProfile>(empty_name).is_err());

    let bad_proxy = r#"{"id":"p1","name":"Work","config":{"scheme":"socks5","host":"h","port":0}}"#;
    assert!(serde_json::from_str::<ProxyProfile>(bad_proxy).is_err());
}

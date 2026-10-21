//! Behavioural tests for `Profile`.

use hopswitch_core::{
    DnsConfig, Profile, ProxyConfig, ProxyScheme, ValidationError, MAX_NAME_CHARS,
};

fn proxy() -> ProxyConfig {
    ProxyConfig::new(ProxyScheme::Socks5, "127.0.0.1", 1080).unwrap()
}

fn dns() -> DnsConfig {
    DnsConfig::from_strs(["1.1.1.1"]).unwrap()
}

#[test]
fn creates_a_profile_with_proxy_and_dns() {
    let p = Profile::new("p1", "Work", Some(proxy()), Some(dns())).unwrap();

    assert_eq!(p.id(), "p1");
    assert_eq!(p.name(), "Work");
    assert_eq!(p.proxy(), Some(&proxy()));
    assert_eq!(p.dns(), Some(&dns()));
}

#[test]
fn allows_proxy_only_and_dns_only_profiles() {
    assert!(Profile::new("a", "Proxy only", Some(proxy()), None).is_ok());
    assert!(Profile::new("b", "DNS only", None, Some(dns())).is_ok());
}

#[test]
fn rejects_a_profile_with_nothing_configured() {
    let err = Profile::new("p1", "Empty", None, None).unwrap_err();
    assert_eq!(err, ValidationError::EmptyProfile);
}

#[test]
fn trims_the_name() {
    let p = Profile::new("p1", "  Work  ", Some(proxy()), None).unwrap();
    assert_eq!(p.name(), "Work");
}

#[test]
fn rejects_empty_names_and_ids() {
    assert_eq!(
        Profile::new("p1", "   ", Some(proxy()), None).unwrap_err(),
        ValidationError::EmptyName
    );
    assert_eq!(
        Profile::new(" ", "Work", Some(proxy()), None).unwrap_err(),
        ValidationError::EmptyId
    );
}

#[test]
fn name_length_is_counted_in_characters_not_bytes() {
    let persian_ok = "پ".repeat(MAX_NAME_CHARS);
    assert!(Profile::new("p1", persian_ok, Some(proxy()), None).is_ok());

    let too_long = "پ".repeat(MAX_NAME_CHARS + 1);
    assert_eq!(
        Profile::new("p1", too_long, Some(proxy()), None).unwrap_err(),
        ValidationError::NameTooLong {
            max: MAX_NAME_CHARS
        }
    );
}

#[test]
fn serde_round_trip_preserves_the_value() {
    let p = Profile::new("p1", "Work", Some(proxy()), Some(dns())).unwrap();
    let back: Profile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back, p);
}

#[test]
fn deserialising_an_invalid_profile_is_rejected() {
    let json = r#"{"id":"p1","name":"","proxy":null,"dns":["1.1.1.1"]}"#;
    assert!(serde_json::from_str::<Profile>(json).is_err());
}

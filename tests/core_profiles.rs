//! Behavioural tests for the `Profiles` collection.

use hopswitch::core::{
    DnsConfig, DnsProfile, Profiles, ProfilesError, ProxyConfig, ProxyProfile, ProxyScheme,
};

fn proxy(id: &str) -> ProxyProfile {
    ProxyProfile::new(
        id,
        id,
        ProxyConfig::new(ProxyScheme::Socks5, "127.0.0.1", 1080).unwrap(),
    )
    .unwrap()
}

fn dns(id: &str) -> DnsProfile {
    DnsProfile::new(id, id, DnsConfig::from_strs(["1.1.1.1"]).unwrap()).unwrap()
}

#[test]
fn starts_empty() {
    let p = Profiles::new();

    assert_eq!(p.proxy_profiles(), []);
    assert_eq!(p.dns_profiles(), []);
}

#[test]
fn adds_and_finds_profiles_by_id() {
    let mut p = Profiles::new();
    p.add_proxy(proxy("a")).unwrap();
    p.add_dns(dns("d")).unwrap();

    assert_eq!(p.proxy("a"), Some(&proxy("a")));
    assert_eq!(p.dns("d"), Some(&dns("d")));
    assert_eq!(p.proxy("zzz"), None);
    assert_eq!(p.proxy_profiles().len(), 1);
    assert_eq!(p.dns_profiles().len(), 1);
}

#[test]
fn keeps_insertion_order() {
    let mut p = Profiles::new();
    for id in ["c", "a", "b"] {
        p.add_proxy(proxy(id)).unwrap();
    }

    let ids: Vec<_> = p.proxy_profiles().iter().map(ProxyProfile::id).collect();
    assert_eq!(ids, ["c", "a", "b"]);
}

#[test]
fn rejects_a_duplicate_id_within_a_kind() {
    let mut p = Profiles::new();
    p.add_proxy(proxy("a")).unwrap();
    p.add_dns(dns("d")).unwrap();

    assert_eq!(
        p.add_proxy(proxy("a")).unwrap_err(),
        ProfilesError::DuplicateId("a".into())
    );
    assert_eq!(
        p.add_dns(dns("d")).unwrap_err(),
        ProfilesError::DuplicateId("d".into())
    );
    assert_eq!(p.proxy_profiles().len(), 1);
}

#[test]
fn the_same_id_may_exist_in_both_kinds() {
    let mut p = Profiles::new();

    p.add_proxy(proxy("x")).unwrap();
    p.add_dns(dns("x")).unwrap();
}

#[test]
fn replaces_an_existing_profile_in_place() {
    let mut p = Profiles::new();
    p.add_proxy(proxy("a")).unwrap();
    p.add_proxy(proxy("b")).unwrap();
    let edited = ProxyProfile::new(
        "a",
        "Renamed",
        ProxyConfig::new(ProxyScheme::Http, "h", 8080).unwrap(),
    )
    .unwrap();

    p.replace_proxy(edited.clone()).unwrap();

    assert_eq!(p.proxy("a"), Some(&edited));
    assert_eq!(p.proxy_profiles()[0].id(), "a", "position is kept");
}

#[test]
fn replacing_or_removing_an_unknown_id_is_an_error() {
    let mut p = Profiles::new();

    assert_eq!(
        p.replace_proxy(proxy("a")).unwrap_err(),
        ProfilesError::NotFound("a".into())
    );
    assert_eq!(
        p.replace_dns(dns("d")).unwrap_err(),
        ProfilesError::NotFound("d".into())
    );
    assert_eq!(
        p.remove_proxy("a").unwrap_err(),
        ProfilesError::NotFound("a".into())
    );
    assert_eq!(
        p.remove_dns("d").unwrap_err(),
        ProfilesError::NotFound("d".into())
    );
}

#[test]
fn removes_and_returns_the_profile() {
    let mut p = Profiles::new();
    p.add_proxy(proxy("a")).unwrap();
    p.add_dns(dns("d")).unwrap();

    assert_eq!(p.remove_proxy("a").unwrap(), proxy("a"));
    assert_eq!(p.remove_dns("d").unwrap(), dns("d"));
    assert!(p.proxy_profiles().is_empty() && p.dns_profiles().is_empty());
}

#[test]
fn serde_round_trip_preserves_everything() {
    let mut p = Profiles::new();
    p.add_proxy(proxy("a")).unwrap();
    p.add_proxy(proxy("b")).unwrap();
    p.add_dns(dns("d")).unwrap();

    let back: Profiles = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();

    assert_eq!(back, p);
}

#[test]
fn missing_lists_deserialise_as_empty() {
    let p: Profiles = serde_json::from_str("{}").unwrap();

    assert_eq!(p, Profiles::new());
}

#[test]
fn deserialising_duplicate_ids_is_rejected() {
    let one = r#"{"id":"a","name":"A","config":{"scheme":"socks5","host":"h","port":1080}}"#;
    let json = format!(r#"{{"proxies":[{one},{one}]}}"#);

    assert!(serde_json::from_str::<Profiles>(&json).is_err());
}

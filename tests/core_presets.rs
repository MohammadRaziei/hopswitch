//! Tests for the built-in DNS and proxy preset catalog.

use std::collections::HashSet;

use hopswitch::core::{
    dns_preset, dns_presets, proxy_preset, proxy_presets, DnsConfig, DnsProfile, Profiles,
    ProxyConfig, ProxyProfile, ProxyScheme, ValidationError,
};

#[test]
fn the_catalogs_are_not_empty() {
    assert!(!dns_presets().is_empty());
    assert!(!proxy_presets().is_empty());
}

#[test]
fn every_dns_preset_builds_a_valid_config() {
    for preset in dns_presets() {
        let config = preset
            .config()
            .unwrap_or_else(|e| panic!("preset `{}` is invalid: {e}", preset.id()));
        assert!(!config.servers().is_empty(), "preset `{}`", preset.id());
        assert_ne!(preset.name().trim(), "");
    }
}

#[test]
fn every_proxy_preset_builds_a_valid_config() {
    for preset in proxy_presets() {
        preset
            .config()
            .unwrap_or_else(|e| panic!("preset `{}` is invalid: {e}", preset.id()));
        assert_ne!(preset.name().trim(), "");
    }
}

#[test]
fn preset_ids_are_unique_lowercase_slugs() {
    let dns: Vec<_> = dns_presets()
        .iter()
        .map(hopswitch::core::DnsPreset::id)
        .collect();
    let proxy: Vec<_> = proxy_presets()
        .iter()
        .map(hopswitch::core::ProxyPreset::id)
        .collect();

    for ids in [dns, proxy] {
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "duplicate preset id");
        for id in ids {
            assert!(
                !id.is_empty()
                    && id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "`{id}` is not a lowercase slug"
            );
        }
    }
}

#[test]
fn well_known_dns_presets_have_their_documented_servers() {
    let expect = |id: &str, servers: &[&str]| {
        let preset = dns_preset(id).unwrap_or_else(|| panic!("missing `{id}`"));
        assert_eq!(
            preset.config().unwrap(),
            DnsConfig::from_strs(servers).unwrap()
        );
    };

    expect("cloudflare", &["1.1.1.1", "1.0.0.1"]);
    expect("google", &["8.8.8.8", "8.8.4.4"]);
    expect("quad9", &["9.9.9.9", "149.112.112.112"]);
    expect("opendns", &["208.67.222.222", "208.67.220.220"]);
    expect("adguard", &["94.140.14.14", "94.140.15.15"]);
    expect("shecan", &["178.22.122.100", "185.51.200.2"]);
}

#[test]
fn well_known_proxy_presets_use_the_default_local_ports() {
    let expect = |id: &str, scheme, port| {
        let preset = proxy_preset(id).unwrap_or_else(|| panic!("missing `{id}`"));
        assert_eq!(
            preset.config().unwrap(),
            ProxyConfig::new(scheme, "127.0.0.1", port).unwrap(),
            "{id}"
        );
    };

    expect("v2rayn-socks", ProxyScheme::Socks5, 10808);
    expect("v2rayn-http", ProxyScheme::Http, 10809);
    expect("clash", ProxyScheme::Http, 7890);
    expect("clash-verge", ProxyScheme::Http, 7897);
    expect("shadowsocks", ProxyScheme::Socks5, 1080);
    expect("tor", ProxyScheme::Socks5, 9050);
    expect("tor-browser", ProxyScheme::Socks5, 9150);
}

#[test]
fn proxy_presets_only_point_at_this_machine() {
    for preset in proxy_presets() {
        assert_eq!(
            preset.config().unwrap().host(),
            "127.0.0.1",
            "{}",
            preset.id()
        );
    }
}

#[test]
fn an_unknown_id_finds_nothing() {
    assert!(dns_preset("nope").is_none());
    assert!(proxy_preset("nope").is_none());
}

#[test]
fn a_dns_preset_becomes_a_profile_with_the_callers_id() {
    let preset = dns_preset("cloudflare").unwrap();

    let profile = preset.to_profile("my-id").unwrap();

    assert_eq!(profile.id(), "my-id");
    assert_eq!(profile.name(), preset.name());
    assert_eq!(profile.config(), &preset.config().unwrap());
}

#[test]
fn a_proxy_preset_becomes_a_profile_with_the_callers_id() {
    let preset = proxy_preset("v2rayn-socks").unwrap();

    let profile = preset.to_profile("my-id").unwrap();

    assert_eq!(profile.id(), "my-id");
    assert_eq!(profile.name(), preset.name());
    assert_eq!(profile.config(), &preset.config().unwrap());
}

#[test]
fn creating_a_profile_from_a_preset_still_validates_the_id() {
    assert_eq!(
        dns_preset("google").unwrap().to_profile(" ").unwrap_err(),
        ValidationError::EmptyId
    );
    assert_eq!(
        proxy_preset("tor").unwrap().to_profile("").unwrap_err(),
        ValidationError::EmptyId
    );
}

#[test]
fn a_preset_counts_as_saved_when_any_profile_has_the_same_settings() {
    let preset = dns_preset("cloudflare").unwrap();
    let mut profiles = Profiles::new();
    assert!(!preset.is_saved_in(&profiles));

    // Saved under a different id and a custom name: still the same settings.
    let custom = DnsProfile::new(
        "x1",
        "My DNS",
        DnsConfig::from_strs(["1.1.1.1", "1.0.0.1"]).unwrap(),
    )
    .unwrap();
    profiles.add_dns(custom).unwrap();

    assert!(preset.is_saved_in(&profiles));
    assert!(!dns_preset("google").unwrap().is_saved_in(&profiles));
}

#[test]
fn proxy_saved_detection_works_the_same_way() {
    let preset = proxy_preset("v2rayn-socks").unwrap();
    let mut profiles = Profiles::new();
    assert!(!preset.is_saved_in(&profiles));

    let custom = ProxyProfile::new(
        "x1",
        "Mine",
        ProxyConfig::new(ProxyScheme::Socks5, "127.0.0.1", 10808).unwrap(),
    )
    .unwrap();
    profiles.add_proxy(custom).unwrap();

    assert!(preset.is_saved_in(&profiles));
    assert!(!proxy_preset("clash").unwrap().is_saved_in(&profiles));
}

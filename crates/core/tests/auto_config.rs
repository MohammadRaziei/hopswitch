//! The persisted auto settings (ADR-0009, ADR-0010): proxy follows the selected
//! profile; DNS keeps its own options and pool.

use hopswitch_core::{AutoConfig, AutoKind, ConnectivitySettings, ProxyAuto};

#[test]
fn everything_is_off_and_empty_by_default() {
    let config = AutoConfig::default();

    assert!(!config.proxy.connect);
    assert_eq!(config.proxy.selected, None);
    assert!(!config.proxy.is_enabled());
    assert!(!config.dns.connect && !config.dns.switch && config.dns.pool.is_empty());
    assert!(!config.dns.is_enabled());
    assert_eq!(config.connectivity, ConnectivitySettings::default());
}

#[test]
fn proxy_automation_needs_both_the_flag_and_a_selected_profile() {
    let only_flag = ProxyAuto {
        connect: true,
        selected: None,
    };
    let only_selection = ProxyAuto {
        connect: false,
        selected: Some("smart".into()),
    };
    let both = ProxyAuto {
        connect: true,
        selected: Some("smart".into()),
    };

    assert!(!only_flag.is_enabled());
    assert!(!only_selection.is_enabled());
    assert!(both.is_enabled());
}

#[test]
fn dns_is_enabled_by_either_option_and_keeps_an_ordered_pool() {
    let mut dns = AutoKind {
        connect: true,
        ..AutoKind::default()
    };
    assert!(dns.is_enabled());
    assert!(AutoKind {
        switch: true,
        ..AutoKind::default()
    }
    .is_enabled());

    assert!(dns.add_to_pool("b"));
    assert!(dns.add_to_pool("a"));
    assert!(!dns.add_to_pool("b"), "already there");
    assert_eq!(dns.pool, ["b", "a"]);
    assert!(dns.remove_from_pool("b"));
    assert!(!dns.remove_from_pool("b"), "already gone");
    assert_eq!(dns.pool, ["a"]);
}

#[test]
fn round_trips_through_json_and_tolerates_missing_fields() {
    let mut config = AutoConfig::default();
    config.proxy.connect = true;
    config.proxy.selected = Some("smart".into());
    config.dns.switch = true;
    config.dns.add_to_pool("cloudflare");
    config.connectivity =
        ConnectivitySettings::new("http://probe.test/x", "probe.test", 1500, 10).unwrap();

    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(serde_json::from_str::<AutoConfig>(&json).unwrap(), config);

    assert_eq!(
        serde_json::from_str::<AutoConfig>("{}").unwrap(),
        AutoConfig::default()
    );
    let partial: AutoConfig = serde_json::from_str(r#"{"proxy":{"connect":true}}"#).unwrap();
    assert!(partial.proxy.connect && partial.proxy.selected.is_none());
}

#[test]
fn the_old_proxy_pool_and_switch_fields_are_ignored_not_fatal() {
    let old = r#"{"proxy":{"connect":true,"switch":true,"pool":["a","b"]},"dns":{"switch":true,"pool":["cf"]}}"#;

    let config: AutoConfig = serde_json::from_str(old).unwrap();

    assert!(config.proxy.connect, "the flag survives");
    assert_eq!(
        config.proxy.selected, None,
        "the old pool is not turned into a selection"
    );
    assert_eq!(config.dns.pool, ["cf"], "DNS is unchanged");
}

#[test]
fn invalid_connectivity_settings_in_a_file_are_rejected() {
    let bad = r#"{"connectivity":{"proxy_target":"nope","dns_domain":"example.com","timeout_ms":3000,"interval_secs":30}}"#;

    assert!(serde_json::from_str::<AutoConfig>(bad).is_err());
}

//! The persisted auto settings: what is on, and which profiles take part.

use hopswitch_core::{AutoConfig, AutoKind, ConnectivitySettings};

#[test]
fn everything_is_off_and_empty_by_default() {
    let config = AutoConfig::default();

    for kind in [&config.proxy, &config.dns] {
        assert!(!kind.connect && !kind.switch && kind.pool.is_empty());
        assert!(!kind.is_enabled());
    }
    assert_eq!(config.connectivity, ConnectivitySettings::default());
}

#[test]
fn a_kind_is_enabled_by_either_option() {
    let connect = AutoKind {
        connect: true,
        ..AutoKind::default()
    };
    let switch = AutoKind {
        switch: true,
        ..AutoKind::default()
    };

    assert!(connect.is_enabled() && switch.is_enabled());
}

#[test]
fn the_pool_keeps_order_and_ignores_duplicates() {
    let mut kind = AutoKind::default();

    assert!(kind.add_to_pool("b"));
    assert!(kind.add_to_pool("a"));
    assert!(!kind.add_to_pool("b"), "already there");
    assert_eq!(kind.pool, ["b", "a"]);

    assert!(kind.remove_from_pool("b"));
    assert!(!kind.remove_from_pool("b"), "already gone");
    assert_eq!(kind.pool, ["a"]);
}

#[test]
fn round_trips_through_json_and_tolerates_missing_fields() {
    let mut config = AutoConfig::default();
    config.proxy.connect = true;
    config.proxy.add_to_pool("fast");
    config.dns.switch = true;
    config.connectivity =
        ConnectivitySettings::new("http://probe.test/x", "probe.test", 1500, 10).unwrap();

    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(serde_json::from_str::<AutoConfig>(&json).unwrap(), config);

    assert_eq!(
        serde_json::from_str::<AutoConfig>("{}").unwrap(),
        AutoConfig::default()
    );
    let partial: AutoConfig = serde_json::from_str(r#"{"proxy":{"connect":true}}"#).unwrap();
    assert!(partial.proxy.connect && partial.proxy.pool.is_empty());
}

#[test]
fn invalid_connectivity_settings_in_a_file_are_rejected() {
    let bad = r#"{"connectivity":{"proxy_target":"nope","dns_domain":"example.com","timeout_ms":3000,"interval_secs":30}}"#;

    assert!(serde_json::from_str::<AutoConfig>(bad).is_err());
}

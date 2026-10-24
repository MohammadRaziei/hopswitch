//! Behavioural tests for `JsonFileAutoStore`.

use std::fs;

use hopswitch_core::{AutoConfig, AutoConfigError, AutoConfigStore, ConnectivitySettings};
use hopswitch_storage::JsonFileAutoStore;

fn sample() -> AutoConfig {
    let mut config = AutoConfig::default();
    config.proxy.connect = true;
    config.proxy.add_to_pool("محل-کار");
    config.proxy.add_to_pool("clash");
    config.dns.switch = true;
    config.dns.add_to_pool("cloudflare");
    config.connectivity =
        ConnectivitySettings::new("https://probe.test/up", "probe.test", 2500, 45).unwrap();
    config
}

#[test]
fn a_missing_file_means_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileAutoStore::new(dir.path().join("auto.json"));

    assert_eq!(store.load().unwrap(), AutoConfig::default());
    assert!(
        !dir.path().join("auto.json").exists(),
        "loading never creates the file"
    );
}

#[test]
fn saved_settings_come_back_identical_including_the_pool_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileAutoStore::new(dir.path().join("sub").join("auto.json"));

    store.save(&sample()).unwrap();

    assert_eq!(
        JsonFileAutoStore::new(dir.path().join("sub").join("auto.json"))
            .load()
            .unwrap(),
        sample()
    );
}

#[test]
fn the_file_is_versioned() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auto.json");
    JsonFileAutoStore::new(&path).save(&sample()).unwrap();

    let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

    assert_eq!(value["version"], 1);
    assert_eq!(value["proxy"]["pool"][1], "clash");
}

#[test]
fn a_damaged_file_is_reported_and_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auto.json");
    fs::write(&path, b"{ broken").unwrap();

    let result = JsonFileAutoStore::new(&path).load();

    assert!(
        matches!(result, Err(AutoConfigError::Corrupt(_))),
        "{result:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), b"{ broken");
}

#[test]
fn invalid_connectivity_values_in_the_file_count_as_damage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auto.json");
    fs::write(
        &path,
        r#"{"version":1,"connectivity":{"proxy_target":"x","dns_domain":"a.b","timeout_ms":3000,"interval_secs":30}}"#,
    )
    .unwrap();

    assert!(matches!(
        JsonFileAutoStore::new(&path).load(),
        Err(AutoConfigError::Corrupt(_))
    ));
}

#[test]
fn a_missing_or_newer_version_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auto.json");

    fs::write(&path, b"{}").unwrap();
    assert!(matches!(
        JsonFileAutoStore::new(&path).load(),
        Err(AutoConfigError::Corrupt(_))
    ));

    fs::write(&path, br#"{"version":2}"#).unwrap();
    assert_eq!(
        JsonFileAutoStore::new(&path).load(),
        Err(AutoConfigError::UnsupportedVersion {
            found: 2,
            supported: 1
        })
    );
}

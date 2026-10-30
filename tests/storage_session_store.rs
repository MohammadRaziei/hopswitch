//! Behavioural tests for `JsonFileSessionStore`.

use std::fs;
use std::sync::Arc;

use hopswitch::core::testing::InMemoryNetwork;
use hopswitch::core::{
    DnsConfig, DnsState, ProxyConfig, ProxyProfile, ProxyScheme, ProxyService, ProxyState, Session,
    SessionStore, SystemNetwork,
};
use hopswitch::storage::JsonFileSessionStore;

fn manual(host: &str) -> ProxyState {
    ProxyState::Manual(ProxyConfig::new(ProxyScheme::Http, host, 3128).unwrap())
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, JsonFileSessionStore) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-proxy.json");
    let store = JsonFileSessionStore::new(&path);
    (dir, path, store)
}

#[test]
fn nothing_stored_loads_as_none() {
    let (_dir, _path, store) = setup();

    assert_eq!(store.load::<ProxyState>().unwrap(), None);
}

#[test]
fn a_saved_session_loads_back_identically() {
    let (_dir, _path, store) = setup();
    let session = Session::new(manual("corp.example"), "p1");

    store.save(&session).unwrap();

    assert_eq!(store.load::<ProxyState>().unwrap(), Some(session));
}

#[test]
fn foreign_and_dns_states_round_trip_too() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileSessionStore::new(dir.path().join("session-dns.json"));
    let session = Session::new(
        DnsState::Other {
            raw: "resolvectl:eth0:1.1.1.1#cloudflare".into(),
        },
        "d1",
    );
    store.save(&session).unwrap();
    assert_eq!(store.load::<DnsState>().unwrap(), Some(session));

    let session = Session::new(
        DnsState::Manual(DnsConfig::from_strs(["9.9.9.9"]).unwrap()),
        "d2",
    );
    store.save(&session).unwrap();
    assert_eq!(store.load::<DnsState>().unwrap(), Some(session));
}

#[test]
fn saving_again_replaces_the_previous_session() {
    let (_dir, _path, store) = setup();
    store.save(&Session::new(manual("a.example"), "a")).unwrap();

    store.save(&Session::new(manual("b.example"), "b")).unwrap();

    let loaded = store.load::<ProxyState>().unwrap().unwrap();
    assert_eq!(loaded.active_id(), "b");
}

#[test]
fn clear_removes_the_session() {
    let (_dir, path, store) = setup();
    store.save(&Session::new(manual("a.example"), "a")).unwrap();

    SessionStore::<ProxyState>::clear(&store).unwrap();

    assert!(!path.exists());
    assert_eq!(store.load::<ProxyState>().unwrap(), None);
}

#[test]
fn clearing_when_nothing_is_stored_is_fine() {
    let (_dir, _path, store) = setup();

    SessionStore::<ProxyState>::clear(&store).unwrap();
}

#[test]
fn the_file_carries_a_schema_version() {
    let (_dir, path, store) = setup();
    store.save(&Session::new(manual("a.example"), "a")).unwrap();

    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["session"]["active_id"], "a");
}

#[test]
fn a_corrupt_session_is_an_error_and_is_never_deleted() {
    let (_dir, path, store) = setup();
    fs::write(&path, b"{ broken").unwrap();

    assert!(store.load::<ProxyState>().is_err());
    assert_eq!(
        fs::read(&path).unwrap(),
        b"{ broken",
        "the snapshot may still be needed"
    );
}

#[test]
fn a_newer_schema_version_is_refused() {
    let (_dir, path, store) = setup();
    fs::write(path, br#"{"version":99,"session":{}}"#).unwrap();

    assert!(store.load::<ProxyState>().is_err());
}

#[test]
fn an_unwritable_path_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    fs::create_dir(&path).unwrap();
    let store = JsonFileSessionStore::new(&path);

    assert!(store.save(&Session::new(manual("a.example"), "a")).is_err());
}

#[test]
fn after_a_restart_the_original_settings_can_still_be_restored() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-proxy.json");
    let users = manual("corp.example");
    let net = Arc::new(InMemoryNetwork::new().with_proxy(users.clone()));

    // First run: activate a profile, then "crash" (drop the service without deactivating).
    {
        let store = Arc::new(JsonFileSessionStore::new(&path));
        let mut service = ProxyService::new(net.clone(), store);
        let profile = ProxyProfile::new(
            "a",
            "A",
            ProxyConfig::new(ProxyScheme::Socks5, "127.0.0.1", 1080).unwrap(),
        )
        .unwrap();
        service.activate(&profile).unwrap();
        assert_ne!(net.read_proxy().unwrap(), users);
    }

    // Second run: load the persisted session and restore.
    let store = Arc::new(JsonFileSessionStore::new(&path));
    let session = store
        .load::<ProxyState>()
        .unwrap()
        .expect("a session was persisted");
    let mut service = ProxyService::new(net.clone(), store.clone());
    service.resume(session);
    service.deactivate().unwrap();

    assert_eq!(net.read_proxy().unwrap(), users);
    assert_eq!(store.load::<ProxyState>().unwrap(), None);
}

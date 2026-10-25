//! Behavioural tests for `JsonFileRepository`.

use std::fs;

use hopswitch_core::{
    DnsConfig, DnsProfile, ProfileRepository, Profiles, ProxyConfig, ProxyGroup, ProxyProfile,
    ProxyScheme, RepositoryError,
};
use hopswitch_storage::JsonFileRepository;

fn sample() -> Profiles {
    let mut p = Profiles::new();
    p.add_proxy(
        ProxyProfile::new(
            "p1",
            "محل کار",
            ProxyConfig::new(ProxyScheme::Socks5, "127.0.0.1", 10808).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    p.add_dns(
        DnsProfile::new(
            "d1",
            "Cloudflare",
            DnsConfig::from_strs(["1.1.1.1", "1.0.0.1"]).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    p
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, JsonFileRepository) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.json");
    let repo = JsonFileRepository::new(&path);
    (dir, path, repo)
}

#[test]
fn a_missing_file_loads_as_empty() {
    let (_dir, _path, repo) = setup();

    assert_eq!(repo.load().unwrap(), Profiles::new());
}

#[test]
fn saved_profiles_load_back_identically() {
    let (_dir, _path, repo) = setup();

    repo.save(&sample()).unwrap();

    assert_eq!(repo.load().unwrap(), sample());
}

#[test]
fn saving_creates_missing_directories() {
    let dir = tempfile::tempdir().unwrap();
    let repo = JsonFileRepository::new(dir.path().join("config").join("hopswitch").join("p.json"));

    repo.save(&sample()).unwrap();

    assert_eq!(repo.load().unwrap(), sample());
}

#[test]
fn the_file_carries_a_schema_version() {
    let (_dir, path, repo) = setup();

    repo.save(&sample()).unwrap();

    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(value["proxies"][0]["id"], "p1");
}

#[test]
fn saving_again_replaces_the_previous_content() {
    let (_dir, _path, repo) = setup();
    repo.save(&sample()).unwrap();

    repo.save(&Profiles::new()).unwrap();

    assert_eq!(repo.load().unwrap(), Profiles::new());
}

#[test]
fn invalid_json_is_reported_as_corrupt_and_left_untouched() {
    let (_dir, path, repo) = setup();
    fs::write(&path, b"{ not json").unwrap();

    assert!(matches!(repo.load(), Err(RepositoryError::Corrupt(_))));
    assert_eq!(fs::read(&path).unwrap(), b"{ not json");
}

#[test]
fn a_file_without_a_version_is_corrupt() {
    let (_dir, path, repo) = setup();
    fs::write(path, br#"{"proxies":[],"dns":[]}"#).unwrap();

    assert!(matches!(repo.load(), Err(RepositoryError::Corrupt(_))));
}

#[test]
fn valid_json_that_breaks_domain_rules_is_corrupt() {
    let (_dir, path, repo) = setup();
    let bad_port = r#"{"version":1,"proxies":[{"id":"p","name":"P","config":{"scheme":"socks5","host":"h","port":0}}]}"#;
    fs::write(path, bad_port).unwrap();

    assert!(matches!(repo.load(), Err(RepositoryError::Corrupt(_))));
}

#[test]
fn a_newer_schema_version_is_refused_not_treated_as_corrupt() {
    let (_dir, path, repo) = setup();
    fs::write(path, br#"{"version":99,"proxies":[],"dns":[]}"#).unwrap();

    assert_eq!(
        repo.load().unwrap_err(),
        RepositoryError::UnsupportedVersion {
            found: 99,
            supported: 2
        }
    );
}

#[test]
fn quarantine_moves_the_corrupt_file_aside_so_loading_starts_fresh() {
    let (dir, path, repo) = setup();
    fs::write(&path, b"garbage").unwrap();

    let moved = repo.quarantine_corrupt().unwrap();

    assert_eq!(moved, dir.path().join("profiles.json.corrupt"));
    assert_eq!(fs::read(&moved).unwrap(), b"garbage", "nothing is deleted");
    assert!(!path.exists());
    assert_eq!(repo.load().unwrap(), Profiles::new());
    repo.save(&sample()).unwrap();
    assert_eq!(repo.load().unwrap(), sample());
}

#[test]
fn repeated_quarantines_never_overwrite_an_earlier_one() {
    let (dir, path, repo) = setup();
    fs::write(&path, b"first").unwrap();
    let first = repo.quarantine_corrupt().unwrap();
    fs::write(&path, b"second").unwrap();

    let second = repo.quarantine_corrupt().unwrap();

    assert_ne!(first, second);
    assert_eq!(fs::read(&first).unwrap(), b"first");
    assert_eq!(fs::read(&second).unwrap(), b"second");
    assert!(second.starts_with(dir.path()));
}

#[test]
fn quarantining_when_there_is_no_file_is_an_error() {
    let (_dir, _path, repo) = setup();

    assert!(matches!(
        repo.quarantine_corrupt(),
        Err(RepositoryError::Io(_))
    ));
}

#[test]
fn an_unreadable_path_is_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.json");
    fs::create_dir(&path).unwrap(); // a directory where the file should be
    let repo = JsonFileRepository::new(&path);

    assert!(matches!(repo.load(), Err(RepositoryError::Io(_))));
    assert!(matches!(repo.save(&sample()), Err(RepositoryError::Io(_))));
}

#[test]
fn a_version_1_file_without_groups_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.json");
    let old = r#"{"version":1,"proxies":[{"id":"p","name":"P","config":{"scheme":"socks5","host":"h.test","port":1080}}],"dns":[]}"#;
    fs::write(&path, old).unwrap();

    let profiles = JsonFileRepository::new(&path).load().unwrap();

    assert_eq!(profiles.proxy_profiles().len(), 1);
    assert!(profiles.groups().is_empty());
}

#[test]
fn groups_survive_a_save_and_a_load() {
    let dir = tempfile::tempdir().unwrap();
    let repo = JsonFileRepository::new(dir.path().join("profiles.json"));
    let mut profiles = Profiles::new();
    for id in ["a", "b"] {
        let config: ProxyConfig = format!("socks5://{id}.test:1080").parse().unwrap();
        profiles
            .add_proxy(ProxyProfile::new(id, id, config).unwrap())
            .unwrap();
    }
    let group = ProxyGroup::new("smart", "Smart", ["b", "a"].map(String::from)).unwrap();
    profiles.add_group(group).unwrap();

    repo.save(&profiles).unwrap();

    assert_eq!(repo.load().unwrap(), profiles);
}

#[test]
fn a_group_pointing_at_a_missing_proxy_makes_the_file_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.json");
    let bad = r#"{"version":2,"proxies":[],"groups":[{"id":"g","name":"g","members":["ghost"],"policy":"fastest"}]}"#;
    fs::write(&path, bad).unwrap();

    let result = JsonFileRepository::new(&path).load();

    assert!(
        matches!(result, Err(RepositoryError::Corrupt(_))),
        "{result:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), bad.as_bytes(), "left untouched");
}

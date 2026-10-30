//! Proxy groups ("fastest of ..."): validation and integrity inside `Profiles`.

use hopswitch::core::{
    GroupPolicy, Profiles, ProfilesError, ProxyConfig, ProxyEntry, ProxyGroup, ProxyProfile,
    ValidationError,
};

fn proxy(id: &str) -> ProxyProfile {
    let config: ProxyConfig = format!("socks5://{id}.test:1080").parse().unwrap();
    ProxyProfile::new(id, id, config).unwrap()
}

fn group(id: &str, members: &[&str]) -> ProxyGroup {
    ProxyGroup::new(id, id, members.iter().map(|m| (*m).to_owned())).unwrap()
}

fn with_proxies(ids: &[&str]) -> Profiles {
    let mut profiles = Profiles::new();
    for id in ids {
        profiles.add_proxy(proxy(id)).unwrap();
    }
    profiles
}

// ---------- the group itself ----------

#[test]
fn a_group_keeps_its_members_in_order_and_defaults_to_fastest() {
    let g = ProxyGroup::new("smart", "  Smart  ", ["b", "a", "c"].map(String::from)).unwrap();

    assert_eq!((g.id(), g.name()), ("smart", "Smart"));
    assert_eq!(g.members(), ["b", "a", "c"]);
    assert_eq!(g.policy(), GroupPolicy::Fastest);
}

#[test]
fn a_group_needs_members_without_repeats() {
    assert_eq!(
        ProxyGroup::new("g", "g", Vec::<String>::new()),
        Err(ValidationError::NoGroupMembers)
    );
    assert_eq!(
        ProxyGroup::new("g", "g", ["a", "b", "a"].map(String::from)),
        Err(ValidationError::DuplicateGroupMember("a".into()))
    );
}

#[test]
fn a_group_has_the_same_identity_rules_as_other_profiles() {
    let members = || ["a"].map(String::from);

    assert_eq!(
        ProxyGroup::new("", "g", members()),
        Err(ValidationError::EmptyId)
    );
    assert_eq!(
        ProxyGroup::new("g", "   ", members()),
        Err(ValidationError::EmptyName)
    );
    assert!(ProxyGroup::new("g", "x".repeat(65), members()).is_err());
}

#[test]
fn a_group_round_trips_and_is_revalidated_when_loaded() {
    let g = group("smart", &["a", "b"]);
    let json = serde_json::to_string(&g).unwrap();

    assert!(json.contains(r#""policy":"fastest""#), "{json}");
    assert_eq!(serde_json::from_str::<ProxyGroup>(&json).unwrap(), g);

    let empty = r#"{"id":"g","name":"g","members":[],"policy":"fastest"}"#;
    assert!(serde_json::from_str::<ProxyGroup>(empty).is_err());
    let unknown_policy = r#"{"id":"g","name":"g","members":["a"],"policy":"random"}"#;
    assert!(serde_json::from_str::<ProxyGroup>(unknown_policy).is_err());
}

// ---------- inside Profiles ----------

#[test]
fn groups_are_added_in_order_and_found_by_id() {
    let mut profiles = with_proxies(&["a", "b", "c"]);

    profiles.add_group(group("fast", &["a", "b"])).unwrap();
    profiles.add_group(group("all", &["a", "b", "c"])).unwrap();

    let ids: Vec<_> = profiles.groups().iter().map(ProxyGroup::id).collect();
    assert_eq!(ids, ["fast", "all"]);
    assert_eq!(profiles.group("all").unwrap().members().len(), 3);
    assert!(profiles.group("nope").is_none());
}

#[test]
fn members_must_be_existing_plain_proxies() {
    let mut profiles = with_proxies(&["a"]);
    profiles.add_group(group("g1", &["a"])).unwrap();

    assert_eq!(
        profiles.add_group(group("g2", &["a", "ghost"])),
        Err(ProfilesError::UnknownMember {
            group: "g2".into(),
            member: "ghost".into()
        })
    );
    // a group is not a valid member: this is what makes cycles impossible
    assert_eq!(
        profiles.add_group(group("g3", &["g1"])),
        Err(ProfilesError::UnknownMember {
            group: "g3".into(),
            member: "g1".into()
        })
    );
}

#[test]
fn proxies_and_groups_share_one_id_namespace() {
    let mut profiles = with_proxies(&["a", "b"]);
    profiles.add_group(group("smart", &["a"])).unwrap();

    assert_eq!(
        profiles.add_group(group("a", &["b"])),
        Err(ProfilesError::DuplicateId("a".into()))
    );
    assert_eq!(
        profiles.add_group(group("smart", &["b"])),
        Err(ProfilesError::DuplicateId("smart".into()))
    );
    assert_eq!(
        profiles.add_proxy(proxy("smart")),
        Err(ProfilesError::DuplicateId("smart".into()))
    );
}

#[test]
fn a_proxy_used_by_a_group_cannot_be_removed_until_the_group_is_gone() {
    let mut profiles = with_proxies(&["a", "b"]);
    profiles.add_group(group("g1", &["a", "b"])).unwrap();
    profiles.add_group(group("g2", &["a"])).unwrap();

    assert_eq!(
        profiles.remove_proxy("a"),
        Err(ProfilesError::InUse {
            id: "a".into(),
            by: vec!["g1".into(), "g2".into()]
        })
    );
    assert_eq!(profiles.groups_using("a"), ["g1", "g2"]);
    assert!(profiles.proxy("a").is_some(), "nothing was removed");

    profiles.remove_group("g1").unwrap();
    profiles.remove_group("g2").unwrap();
    assert!(profiles.remove_proxy("a").is_ok());
}

#[test]
fn removing_or_replacing_an_unknown_group_is_an_error() {
    let mut profiles = with_proxies(&["a"]);

    assert_eq!(
        profiles.remove_group("x"),
        Err(ProfilesError::NotFound("x".into()))
    );
    assert_eq!(
        profiles.replace_group(group("x", &["a"])),
        Err(ProfilesError::NotFound("x".into()))
    );
}

#[test]
fn replacing_a_group_keeps_its_position_and_revalidates_members() {
    let mut profiles = with_proxies(&["a", "b", "c"]);
    profiles.add_group(group("g1", &["a"])).unwrap();
    profiles.add_group(group("g2", &["b"])).unwrap();

    profiles.replace_group(group("g1", &["b", "c"])).unwrap();
    assert_eq!(profiles.group("g1").unwrap().members(), ["b", "c"]);
    assert_eq!(profiles.groups()[0].id(), "g1");

    assert!(matches!(
        profiles.replace_group(group("g1", &["zzz"])),
        Err(ProfilesError::UnknownMember { .. })
    ));
    assert_eq!(
        profiles.group("g1").unwrap().members(),
        ["b", "c"],
        "unchanged after the error"
    );
}

#[test]
fn an_entry_is_either_a_proxy_or_a_group() {
    let mut profiles = with_proxies(&["a"]);
    profiles.add_group(group("smart", &["a"])).unwrap();

    assert!(matches!(profiles.proxy_entry("a"), Some(ProxyEntry::Proxy(p)) if p.id() == "a"));
    assert!(
        matches!(profiles.proxy_entry("smart"), Some(ProxyEntry::Group(g)) if g.id() == "smart")
    );
    assert!(profiles.proxy_entry("nope").is_none());
}

// ---------- persistence shape ----------

#[test]
fn profiles_with_groups_round_trip_and_old_files_still_load() {
    let mut profiles = with_proxies(&["a", "b"]);
    profiles.add_group(group("smart", &["b", "a"])).unwrap();

    let json = serde_json::to_string(&profiles).unwrap();
    assert_eq!(serde_json::from_str::<Profiles>(&json).unwrap(), profiles);

    let old = r#"{"proxies":[],"dns":[]}"#;
    assert_eq!(serde_json::from_str::<Profiles>(old).unwrap().groups(), []);
}

#[test]
fn a_file_that_breaks_the_group_rules_is_refused() {
    let proxy_json = serde_json::to_string(&proxy("a")).unwrap();
    let group_json = |id: &str, member: &str| {
        format!(r#"{{"id":"{id}","name":"{id}","members":["{member}"],"policy":"fastest"}}"#)
    };

    let unknown_member = format!(
        r#"{{"proxies":[{proxy_json}],"groups":[{}]}}"#,
        group_json("g", "ghost")
    );
    assert!(serde_json::from_str::<Profiles>(&unknown_member).is_err());

    let clashing_id = format!(
        r#"{{"proxies":[{proxy_json}],"groups":[{}]}}"#,
        group_json("a", "a")
    );
    assert!(serde_json::from_str::<Profiles>(&clashing_id).is_err());

    let duplicate_groups = format!(
        r#"{{"proxies":[{proxy_json}],"groups":[{},{}]}}"#,
        group_json("g", "a"),
        group_json("g", "a")
    );
    assert!(serde_json::from_str::<Profiles>(&duplicate_groups).is_err());
}

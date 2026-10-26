//! Rules profiles inside `Profiles`: shared id namespace, existing targets, protected targets.

use hopswitch_core::{
    Profiles, ProfilesError, ProxyConfig, ProxyGroup, ProxyProfile, Rule, RulesProfile, Target,
};

fn proxy(id: &str) -> ProxyProfile {
    let config: ProxyConfig = format!("socks5://{id}.test:1080").parse().unwrap();
    ProxyProfile::new(id, id, config).unwrap()
}

fn group(id: &str, members: &[&str]) -> ProxyGroup {
    ProxyGroup::new(id, id, members.iter().map(|m| (*m).to_owned())).unwrap()
}

fn to(id: &str) -> Target {
    Target::Proxy(id.to_owned())
}

fn rules(id: &str, rule_targets: &[(&str, Target)], default: Target) -> RulesProfile {
    let list = rule_targets
        .iter()
        .map(|(domain, target)| Rule::new(domain, target.clone()).unwrap())
        .collect();
    RulesProfile::new(id, id, list, default).unwrap()
}

fn base() -> Profiles {
    let mut profiles = Profiles::new();
    profiles.add_proxy(proxy("a")).unwrap();
    profiles.add_proxy(proxy("b")).unwrap();
    profiles.add_group(group("smart", &["a", "b"])).unwrap();
    profiles
}

#[test]
fn a_rules_profile_is_stored_and_found_by_id() {
    let mut profiles = base();
    profiles
        .add_rules(rules("iran", &[("a.ir", Target::Direct)], to("a")))
        .unwrap();

    assert_eq!(profiles.rules_profiles().len(), 1);
    assert_eq!(profiles.rules_profile("iran").unwrap().id(), "iran");
    assert!(profiles.rules_profile("nope").is_none());
}

#[test]
fn rules_profiles_keep_their_insertion_order() {
    let mut profiles = base();
    for id in ["z", "m", "a2"] {
        profiles.add_rules(rules(id, &[], Target::Direct)).unwrap();
    }
    let ids: Vec<&str> = profiles
        .rules_profiles()
        .iter()
        .map(RulesProfile::id)
        .collect();
    assert_eq!(ids, ["z", "m", "a2"]);
}

#[test]
fn targets_may_be_direct_a_proxy_or_a_group() {
    let mut profiles = base();
    let p = rules(
        "r",
        &[
            ("x.com", Target::Direct),
            ("y.com", to("a")),
            ("z.com", to("smart")),
        ],
        to("smart"),
    );
    assert_eq!(profiles.add_rules(p), Ok(()));
}

#[test]
fn a_rule_pointing_at_a_missing_proxy_is_refused() {
    let mut profiles = base();
    let err = profiles
        .add_rules(rules("r", &[("x.com", to("ghost"))], Target::Direct))
        .unwrap_err();
    assert_eq!(
        err,
        ProfilesError::UnknownTarget {
            profile: "r".into(),
            target: "ghost".into()
        }
    );
    assert_eq!(profiles.rules_profiles(), []);
}

#[test]
fn a_default_pointing_at_a_missing_proxy_is_refused() {
    let mut profiles = base();
    let err = profiles
        .add_rules(rules("r", &[], to("ghost")))
        .unwrap_err();
    assert!(
        matches!(err, ProfilesError::UnknownTarget { .. }),
        "{err:?}"
    );
}

#[test]
fn a_rules_profile_cannot_be_the_target_of_another() {
    let mut profiles = base();
    profiles
        .add_rules(rules("one", &[], Target::Direct))
        .unwrap();
    let err = profiles
        .add_rules(rules("two", &[], to("one")))
        .unwrap_err();
    assert_eq!(
        err,
        ProfilesError::UnknownTarget {
            profile: "two".into(),
            target: "one".into()
        }
    );
}

#[test]
fn proxies_groups_and_rules_profiles_share_one_id_namespace() {
    let mut profiles = base();
    profiles.add_rules(rules("r", &[], Target::Direct)).unwrap();

    for taken in ["a", "smart", "r"] {
        assert_eq!(
            profiles.add_rules(rules(taken, &[], Target::Direct)),
            Err(ProfilesError::DuplicateId(taken.into())),
            "rules as {taken}"
        );
    }
    assert_eq!(
        profiles.add_proxy(proxy("r")),
        Err(ProfilesError::DuplicateId("r".into()))
    );
    assert_eq!(
        profiles.add_group(group("r", &["a"])),
        Err(ProfilesError::DuplicateId("r".into()))
    );
}

#[test]
fn a_proxy_used_by_a_rule_or_a_default_cannot_be_removed() {
    let mut profiles = base();
    profiles
        .add_rules(rules("by-rule", &[("x.com", to("b"))], Target::Direct))
        .unwrap();
    profiles
        .add_rules(rules("by-default", &[], to("b")))
        .unwrap();

    assert_eq!(
        profiles.remove_proxy("b"),
        Err(ProfilesError::InUse {
            id: "b".into(),
            by: vec!["smart".into(), "by-rule".into(), "by-default".into()]
        })
    );
    assert!(profiles.proxy("b").is_some());
}

#[test]
fn a_group_used_by_a_rules_profile_cannot_be_removed() {
    let mut profiles = base();
    profiles
        .add_rules(rules("r", &[("x.com", to("smart"))], Target::Direct))
        .unwrap();

    assert_eq!(
        profiles.remove_group("smart"),
        Err(ProfilesError::InUse {
            id: "smart".into(),
            by: vec!["r".into()]
        })
    );
    assert!(profiles.group("smart").is_some());
}

#[test]
fn once_the_rules_profile_is_gone_the_proxy_can_go() {
    let mut profiles = base();
    profiles.add_rules(rules("r", &[], to("a"))).unwrap();
    profiles.remove_group("smart").unwrap();
    assert!(profiles.remove_proxy("a").is_err());

    profiles.remove_rules("r").unwrap();

    assert!(profiles.remove_proxy("a").is_ok());
}

#[test]
fn a_rules_profile_is_replaced_in_place_and_its_targets_are_checked() {
    let mut profiles = base();
    profiles
        .add_rules(rules("one", &[], Target::Direct))
        .unwrap();
    profiles
        .add_rules(rules("two", &[], Target::Direct))
        .unwrap();

    profiles
        .replace_rules(rules("one", &[("x.com", to("a"))], Target::Direct))
        .unwrap();
    assert_eq!(profiles.rules_profiles()[0].rules().len(), 1);
    assert_eq!(profiles.rules_profiles()[1].id(), "two");

    let err = profiles
        .replace_rules(rules("one", &[("x.com", to("ghost"))], Target::Direct))
        .unwrap_err();
    assert!(matches!(err, ProfilesError::UnknownTarget { .. }));
    assert_eq!(profiles.rules_profiles()[0].rules().len(), 1, "unchanged");

    assert_eq!(
        profiles.replace_rules(rules("none", &[], Target::Direct)),
        Err(ProfilesError::NotFound("none".into()))
    );
}

#[test]
fn removing_a_rules_profile_returns_it() {
    let mut profiles = base();
    profiles.add_rules(rules("r", &[], Target::Direct)).unwrap();

    assert_eq!(profiles.remove_rules("r").unwrap().id(), "r");
    assert_eq!(
        profiles.remove_rules("r"),
        Err(ProfilesError::NotFound("r".into()))
    );
}

#[test]
fn a_rules_id_is_not_a_proxy_entry() {
    let mut profiles = base();
    profiles.add_rules(rules("r", &[], Target::Direct)).unwrap();

    assert!(profiles.proxy_entry("r").is_none());
    assert!(profiles.proxy("r").is_none());
}

// ---------- persistence shape ----------

#[test]
fn profiles_with_rules_round_trip_through_json() {
    let mut profiles = base();
    profiles
        .add_rules(rules("iran", &[("a.ir", Target::Direct)], to("smart")))
        .unwrap();

    let json = serde_json::to_string(&profiles).unwrap();

    assert_eq!(serde_json::from_str::<Profiles>(&json).unwrap(), profiles);
}

#[test]
fn json_without_a_rules_field_still_loads() {
    let json = r#"{"proxies":[],"groups":[],"dns":[]}"#;
    assert_eq!(
        serde_json::from_str::<Profiles>(json)
            .unwrap()
            .rules_profiles(),
        []
    );
}

#[test]
fn json_whose_rules_point_at_a_missing_proxy_is_refused() {
    let json =
        r#"{"proxies":[],"rules":[{"id":"r","name":"r","rules":[],"default":{"proxy":"ghost"}}]}"#;
    assert!(serde_json::from_str::<Profiles>(json).is_err());
}

#[test]
fn json_that_reuses_an_id_across_kinds_is_refused() {
    let json = r#"{"proxies":[{"id":"x","name":"x","config":{"scheme":"socks5","host":"h.test","port":1}}],"rules":[{"id":"x","name":"x","rules":[],"default":"direct"}]}"#;
    let err = serde_json::from_str::<Profiles>(json).unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
}

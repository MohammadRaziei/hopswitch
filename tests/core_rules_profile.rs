//! Behaviour of `Rule`, `Target` and `RulesProfile` (ADR-0010 phase 2, ADR-0011).

use hopswitch::core::{Rule, RulesProfile, Target, ValidationError};

fn proxy(id: &str) -> Target {
    Target::Proxy(id.to_owned())
}

fn rule(domain: &str, target: Target) -> Rule {
    Rule::new(domain, target).unwrap()
}

fn profile(rules: Vec<Rule>, default: Target) -> RulesProfile {
    RulesProfile::new("rp", "Iran", rules, default).unwrap()
}

#[test]
fn a_rule_keeps_a_normalised_domain_and_its_target() {
    let r = Rule::new("  .Example.COM. ", Target::Direct).unwrap();
    assert_eq!(r.domain(), "example.com");
    assert_eq!(r.target(), &Target::Direct);
}

#[test]
fn a_rule_refuses_an_invalid_domain() {
    for bad in [
        "",
        "   ",
        "bad host",
        "under_score.com",
        "a..b",
        "http://x.com",
        "-x.com",
    ] {
        assert_eq!(
            Rule::new(bad, Target::Direct).unwrap_err(),
            ValidationError::InvalidRuleDomain(bad.to_owned()),
            "{bad:?}"
        );
    }
}

#[test]
fn a_profile_needs_an_id_and_a_name_like_every_other_profile() {
    assert_eq!(
        RulesProfile::new(" ", "n", vec![], Target::Direct).unwrap_err(),
        ValidationError::EmptyId
    );
    assert_eq!(
        RulesProfile::new("i", "  ", vec![], Target::Direct).unwrap_err(),
        ValidationError::EmptyName
    );
}

#[test]
fn the_name_is_trimmed() {
    let p = RulesProfile::new("i", "  Iran  ", vec![], Target::Direct).unwrap();
    assert_eq!(p.name(), "Iran");
    assert_eq!(p.id(), "i");
}

#[test]
fn the_same_domain_twice_is_refused_because_the_second_could_never_match() {
    let err = RulesProfile::new(
        "i",
        "n",
        vec![rule("a.com", Target::Direct), rule("A.com", proxy("p"))],
        Target::Direct,
    )
    .unwrap_err();
    assert_eq!(
        err,
        ValidationError::DuplicateRuleDomain("a.com".to_owned())
    );
}

#[test]
fn a_domain_and_its_sub_domain_may_both_have_rules() {
    let p = profile(
        vec![rule("a.com", proxy("p")), rule("b.a.com", Target::Direct)],
        Target::Direct,
    );
    assert_eq!(p.rules().len(), 2);
}

#[test]
fn the_default_target_answers_when_no_rule_matches() {
    let p = profile(vec![rule("a.com", Target::Direct)], proxy("p"));
    assert_eq!(p.target_for("other.org"), &proxy("p"));
    assert_eq!(p.target_for(""), &proxy("p"));
}

#[test]
fn a_rule_matches_its_domain_and_every_sub_domain() {
    let p = profile(vec![rule("a.com", Target::Direct)], proxy("p"));
    assert_eq!(p.target_for("a.com"), &Target::Direct);
    assert_eq!(p.target_for("www.a.com"), &Target::Direct);
    assert_eq!(p.target_for("x.y.a.com"), &Target::Direct);
}

#[test]
fn matching_respects_label_boundaries_case_and_a_trailing_dot() {
    let p = profile(vec![rule("a.com", Target::Direct)], proxy("p"));
    assert_eq!(p.target_for("nota.com"), &proxy("p"));
    assert_eq!(p.target_for("a.com.evil.org"), &proxy("p"));
    assert_eq!(p.target_for("WWW.A.Com."), &Target::Direct);
}

#[test]
fn the_first_matching_rule_wins_even_when_a_later_one_is_more_specific() {
    let p = profile(
        vec![rule("a.com", proxy("one")), rule("b.a.com", Target::Direct)],
        proxy("p"),
    );
    assert_eq!(p.target_for("b.a.com"), &proxy("one"));

    let reversed = profile(
        vec![rule("b.a.com", Target::Direct), rule("a.com", proxy("one"))],
        proxy("p"),
    );
    assert_eq!(reversed.target_for("b.a.com"), &Target::Direct);
    assert_eq!(reversed.target_for("c.a.com"), &proxy("one"));
}

#[test]
fn the_rule_order_is_kept() {
    let p = profile(
        vec![rule("z.com", Target::Direct), rule("a.com", proxy("p"))],
        Target::Direct,
    );
    let domains: Vec<&str> = p.rules().iter().map(Rule::domain).collect();
    assert_eq!(domains, ["z.com", "a.com"]);
}

#[test]
fn a_profile_round_trips_through_json() {
    let p = profile(
        vec![rule("a.ir", Target::Direct), rule("b.com", proxy("p"))],
        proxy("p"),
    );
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(serde_json::from_str::<RulesProfile>(&json).unwrap(), p);
}

#[test]
fn the_json_shape_is_stable() {
    let p = profile(vec![rule("a.ir", Target::Direct)], proxy("p"));
    assert_eq!(
        serde_json::to_string(&p).unwrap(),
        r#"{"id":"rp","name":"Iran","rules":[{"domain":"a.ir","target":"direct"}],"default":{"proxy":"p"}}"#
    );
}

#[test]
fn stored_data_that_breaks_the_rules_is_refused() {
    let bad_domain = r#"{"id":"i","name":"n","rules":[{"domain":"bad host","target":"direct"}],"default":"direct"}"#;
    assert!(serde_json::from_str::<RulesProfile>(bad_domain).is_err());

    let duplicate = r#"{"id":"i","name":"n","rules":[{"domain":"a.com","target":"direct"},{"domain":"a.com","target":"direct"}],"default":"direct"}"#;
    assert!(serde_json::from_str::<RulesProfile>(duplicate).is_err());

    let no_name = r#"{"id":"i","name":" ","rules":[],"default":"direct"}"#;
    assert!(serde_json::from_str::<RulesProfile>(no_name).is_err());
}

#[test]
fn the_rules_field_may_be_missing_in_stored_data() {
    let p: RulesProfile =
        serde_json::from_str(r#"{"id":"i","name":"n","default":"direct"}"#).unwrap();
    assert_eq!(p.rules(), []);
}

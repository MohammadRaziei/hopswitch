//! Turning a saved rules profile into a PAC plan: group targets become their fastest live member,
//! and a profile that could send traffic through SOCKS is refused (ADR-0011, fail closed).

use hopswitch::core::testing::FakeProber;
use hopswitch::core::{
    plan_rules_pac, ConnectivitySettings, Health, Profiles, ProxyConfig, ProxyGroup, ProxyProfile,
    Rule, RulesApplyError, RulesProfile, Target,
};

fn proxy(id: &str, url: &str) -> ProxyProfile {
    let config: ProxyConfig = url.parse().unwrap();
    ProxyProfile::new(id, id, config).unwrap()
}

fn alive(latency_ms: u32) -> Health {
    Health::Alive { latency_ms }
}

fn rules(rules: &[(&str, Target)], default: Target) -> RulesProfile {
    let rules = rules
        .iter()
        .map(|(domain, target)| Rule::new(domain, target.clone()).unwrap())
        .collect();
    RulesProfile::new("work", "Work", rules, default).unwrap()
}

fn via(id: &str) -> Target {
    Target::Proxy(id.to_owned())
}

/// The script for `profile`, or the reason there is none.
fn script(
    profile: &RulesProfile,
    profiles: &Profiles,
    prober: &FakeProber,
) -> Result<String, RulesApplyError> {
    let settings = ConnectivitySettings::default();
    plan_rules_pac(profile, profiles, prober, &settings).map(|plan| plan.generate().unwrap())
}

#[test]
fn direct_and_plain_proxy_targets_become_pac_answers_without_any_probe() {
    let mut profiles = Profiles::new();
    profiles
        .add_proxy(proxy("corp", "http://corp.test:3128"))
        .unwrap();
    let profile = rules(&[("bank.ir", Target::Direct)], via("corp"));
    let prober = FakeProber::new();

    let script = script(&profile, &profiles, &prober).unwrap();

    assert!(script.contains("PROXY corp.test:3128"), "{script}");
    assert!(script.contains("DIRECT"), "{script}");
    assert!(prober.calls().is_empty(), "{:?}", prober.calls());
}

#[test]
fn a_group_target_uses_its_fastest_live_member() {
    let mut profiles = Profiles::new();
    profiles
        .add_proxy(proxy("slow", "http://slow.test:3128"))
        .unwrap();
    profiles
        .add_proxy(proxy("quick", "http://quick.test:3128"))
        .unwrap();
    profiles
        .add_proxy(proxy("down", "http://down.test:3128"))
        .unwrap();
    profiles
        .add_group(
            ProxyGroup::new("best", "Best", ["slow", "quick", "down"].map(String::from)).unwrap(),
        )
        .unwrap();
    let profile = rules(&[("dev.example", via("best"))], Target::Direct);
    let prober = FakeProber::new()
        .with_proxy("http://slow.test:3128", alive(300))
        .with_proxy("http://quick.test:3128", alive(40));

    let script = script(&profile, &profiles, &prober).unwrap();

    assert!(script.contains("PROXY quick.test:3128"), "{script}");
    assert!(!script.contains("slow.test"), "{script}");
    assert!(!script.contains("down.test"), "{script}");
}

#[test]
fn a_group_used_twice_is_probed_once_and_an_unused_group_not_at_all() {
    let mut profiles = Profiles::new();
    for id in ["a", "b", "other"] {
        profiles
            .add_proxy(proxy(id, &format!("http://{id}.test:3128")))
            .unwrap();
    }
    profiles
        .add_group(ProxyGroup::new("used", "Used", ["a", "b"].map(String::from)).unwrap())
        .unwrap();
    profiles
        .add_group(ProxyGroup::new("idle", "Idle", ["other"].map(String::from)).unwrap())
        .unwrap();
    let profile = rules(&[("one.example", via("used"))], via("used"));
    let prober = FakeProber::new().with_proxy("http://a.test:3128", alive(10));

    script(&profile, &profiles, &prober).unwrap();

    let calls = prober.calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls.iter().all(|c| !c.contains("other.test")), "{calls:?}");
}

#[test]
fn a_group_with_no_live_member_is_an_error_that_names_it() {
    let mut profiles = Profiles::new();
    profiles
        .add_proxy(proxy("a", "http://a.test:3128"))
        .unwrap();
    profiles
        .add_group(ProxyGroup::new("best", "Best", ["a".to_owned()]).unwrap())
        .unwrap();
    let profile = rules(&[], via("best"));

    let error = script(&profile, &profiles, &FakeProber::new()).unwrap_err();

    assert_eq!(
        error,
        RulesApplyError::NoLiveMember {
            group: "Best".to_owned()
        }
    );
    assert!(error.to_string().contains("Best"), "{error}");
}

#[test]
fn a_socks_target_is_refused_and_named() {
    let mut profiles = Profiles::new();
    profiles
        .add_proxy(proxy("v2ray", "socks5://127.0.0.1:10808"))
        .unwrap();
    let profile = rules(&[("dev.example", via("v2ray"))], Target::Direct);

    let error = script(&profile, &profiles, &FakeProber::new()).unwrap_err();

    assert_eq!(
        error,
        RulesApplyError::Socks {
            proxies: vec!["v2ray".to_owned()]
        }
    );
    let message = error.to_string();
    assert!(message.contains("v2ray"), "{message}");
    assert!(message.to_lowercase().contains("http"), "{message}");
}

#[test]
fn a_group_with_a_socks_member_is_refused_even_when_that_member_is_not_the_fastest() {
    // The fastest member can change later; a script that may one day say SOCKS must not run.
    let mut profiles = Profiles::new();
    profiles
        .add_proxy(proxy("web", "http://web.test:3128"))
        .unwrap();
    profiles
        .add_proxy(proxy("tunnel", "socks5://tunnel.test:1080"))
        .unwrap();
    profiles
        .add_group(ProxyGroup::new("mix", "Mix", ["web", "tunnel"].map(String::from)).unwrap())
        .unwrap();
    let profile = rules(&[], via("mix"));
    let prober = FakeProber::new()
        .with_proxy("http://web.test:3128", alive(10))
        .with_proxy("socks5://tunnel.test:1080", alive(500));

    let error = script(&profile, &profiles, &prober).unwrap_err();

    assert_eq!(
        error,
        RulesApplyError::Socks {
            proxies: vec!["tunnel".to_owned()]
        }
    );
    assert!(prober.calls().is_empty(), "refused before any probe");
}

#[test]
fn a_target_that_is_not_saved_is_an_error() {
    let profile = rules(&[], via("ghost"));

    let error = script(&profile, &Profiles::new(), &FakeProber::new()).unwrap_err();

    assert_eq!(error, RulesApplyError::MissingTarget("ghost".to_owned()));
}

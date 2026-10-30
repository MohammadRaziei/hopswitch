//! Behaviour of the auto connect / auto switch decision engine.

use hopswitch::core::{AutoMode, AutoSettings, Autopilot, Decision, Health, Probe};

fn alive(id: &str, ms: u32) -> Probe {
    Probe::new(id, Health::Alive { latency_ms: ms })
}

fn dead(id: &str) -> Probe {
    Probe::new(id, Health::Dead)
}

fn pilot(mode: AutoMode) -> Autopilot {
    Autopilot::new(AutoSettings::new(mode))
}

fn activate(id: &str) -> Decision {
    Decision::Activate(id.to_owned())
}

#[test]
fn does_nothing_when_both_options_are_off() {
    let mut p = pilot(AutoMode::Off);

    assert_eq!(p.observe(None, &[alive("a", 10)]), Decision::Keep);
    assert_eq!(
        p.observe(Some("a"), &[dead("a"), dead("a"), dead("a")]),
        Decision::Keep
    );
}

// ---------- auto connect only ----------

#[test]
fn connect_picks_the_first_live_profile_in_the_users_order() {
    let mut p = pilot(AutoMode::Connect);

    let decision = p.observe(None, &[dead("a"), alive("b", 80), alive("c", 20)]);

    assert_eq!(decision, activate("b"));
}

#[test]
fn connect_stays_idle_while_nothing_answers() {
    let mut p = pilot(AutoMode::Connect);

    assert_eq!(p.observe(None, &[dead("a"), dead("b")]), Decision::Keep);
}

#[test]
fn connect_keeps_a_live_profile_even_if_another_is_faster() {
    let mut p = pilot(AutoMode::Connect);

    assert_eq!(
        p.observe(Some("a"), &[alive("a", 500), alive("b", 5)]),
        Decision::Keep
    );
}

#[test]
fn connect_disconnects_only_after_consecutive_failures() {
    let mut p = pilot(AutoMode::Connect);
    let probes = [dead("a"), alive("b", 10)];

    assert_eq!(p.observe(Some("a"), &probes), Decision::Keep);
    assert_eq!(p.observe(Some("a"), &probes), Decision::Keep);
    assert_eq!(p.observe(Some("a"), &probes), Decision::Deactivate);
}

#[test]
fn one_success_resets_the_failure_count() {
    let mut p = pilot(AutoMode::Connect);

    p.observe(Some("a"), &[dead("a")]);
    p.observe(Some("a"), &[dead("a")]);
    p.observe(Some("a"), &[alive("a", 30)]);

    assert_eq!(p.observe(Some("a"), &[dead("a")]), Decision::Keep);
    assert_eq!(p.observe(Some("a"), &[dead("a")]), Decision::Keep);
}

// ---------- auto switch only ----------

#[test]
fn switch_alone_never_connects_by_itself() {
    let mut p = pilot(AutoMode::Switch);

    assert_eq!(p.observe(None, &[alive("a", 10)]), Decision::Keep);
}

#[test]
fn switch_moves_to_a_clearly_faster_profile() {
    let mut p = pilot(AutoMode::Switch);

    assert_eq!(
        p.observe(Some("a"), &[alive("a", 400), alive("b", 100)]),
        activate("b")
    );
}

#[test]
fn switch_ignores_small_gains_to_avoid_flapping() {
    let mut p = pilot(AutoMode::Switch);

    // 20 ms better: below the absolute threshold.
    assert_eq!(
        p.observe(Some("a"), &[alive("a", 100), alive("b", 80)]),
        Decision::Keep
    );
    // 60 ms better but only 6 %: below the relative threshold.
    assert_eq!(
        p.observe(Some("a"), &[alive("a", 1000), alive("b", 940)]),
        Decision::Keep
    );
}

#[test]
fn switch_alone_never_disconnects() {
    let mut p = pilot(AutoMode::Switch);

    for _ in 0..10 {
        assert_eq!(
            p.observe(Some("a"), &[dead("a"), dead("b")]),
            Decision::Keep
        );
    }
}

#[test]
fn switch_fails_over_to_a_live_profile_after_repeated_failures() {
    let mut p = pilot(AutoMode::Switch);
    let probes = [dead("a"), alive("b", 300)];

    assert_eq!(p.observe(Some("a"), &probes), Decision::Keep);
    assert_eq!(p.observe(Some("a"), &probes), Decision::Keep);
    assert_eq!(p.observe(Some("a"), &probes), activate("b"));
}

// ---------- both ----------

#[test]
fn both_connect_to_the_fastest_live_profile() {
    let mut p = pilot(AutoMode::ConnectAndSwitch);

    assert_eq!(
        p.observe(None, &[alive("a", 90), dead("b"), alive("c", 25)]),
        activate("c")
    );
}

#[test]
fn both_disconnect_when_nothing_answers_for_long_enough() {
    let mut p = pilot(AutoMode::ConnectAndSwitch);
    let probes = [dead("a"), dead("b")];

    assert_eq!(p.observe(Some("a"), &probes), Decision::Keep);
    assert_eq!(p.observe(Some("a"), &probes), Decision::Keep);
    assert_eq!(p.observe(Some("a"), &probes), Decision::Deactivate);
}

#[test]
fn both_fail_over_instead_of_disconnecting_when_another_answers() {
    let mut p = pilot(AutoMode::ConnectAndSwitch);
    let probes = [dead("a"), alive("b", 200)];

    p.observe(Some("a"), &probes);
    p.observe(Some("a"), &probes);

    assert_eq!(p.observe(Some("a"), &probes), activate("b"));
}

// ---------- boundaries ----------

#[test]
fn a_profile_outside_the_pool_is_left_alone() {
    let mut p = pilot(AutoMode::ConnectAndSwitch);

    for _ in 0..5 {
        assert_eq!(
            p.observe(Some("manual"), &[dead("a"), alive("b", 1)]),
            Decision::Keep
        );
    }
}

#[test]
fn settings_reject_nonsense() {
    assert!(AutoSettings::new(AutoMode::ConnectAndSwitch)
        .with_failures(0)
        .is_err());
    assert!(AutoSettings::new(AutoMode::ConnectAndSwitch)
        .with_min_gain(50, 100)
        .is_err());

    let custom = AutoSettings::new(AutoMode::Connect)
        .with_failures(1)
        .and_then(|s| s.with_min_gain(0, 0))
        .unwrap();
    let mut p = Autopilot::new(custom);
    assert_eq!(p.observe(Some("a"), &[dead("a")]), Decision::Deactivate);
}

#[test]
fn reconfiguring_keeps_the_failure_streak() {
    let mut p = pilot(AutoMode::Connect);
    p.observe(Some("a"), &[dead("a")]);
    p.observe(Some("a"), &[dead("a")]);

    p.configure(AutoSettings::new(AutoMode::ConnectAndSwitch));

    assert_eq!(p.observe(Some("a"), &[dead("a")]), Decision::Deactivate);
}

// ---------- picking the fastest ----------

#[test]
fn the_fastest_live_probe_wins_and_ties_go_to_the_earlier_one() {
    let probes = [dead("a"), alive("b", 50), alive("c", 20), alive("d", 20)];

    let (winner, latency) = hopswitch::core::fastest_alive(&probes).unwrap();

    assert_eq!((winner.id.as_str(), latency), ("c", 20));
}

#[test]
fn nothing_is_fastest_when_everything_is_dead() {
    assert!(hopswitch::core::fastest_alive(&[dead("a"), dead("b")]).is_none());
    assert!(hopswitch::core::fastest_alive(&[]).is_none());
}

#[test]
fn each_mode_says_which_behaviours_it_turns_on() {
    let table = [
        (AutoMode::Off, false, false),
        (AutoMode::Connect, true, false),
        (AutoMode::Switch, false, true),
        (AutoMode::ConnectAndSwitch, true, true),
    ];
    for (mode, connects, switches) in table {
        assert_eq!(mode.connects(), connects, "{mode:?} connects");
        assert_eq!(mode.switches(), switches, "{mode:?} switches");
    }
}

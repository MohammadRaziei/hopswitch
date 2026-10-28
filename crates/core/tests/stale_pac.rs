//! Telling a PAC address that Hopswitch left behind from one that is still being served.

use std::time::Duration;

use hopswitch_core::testing::FakeProber;
use hopswitch_core::{
    stale_hopswitch_pac, Health, PacConfig, ProxyConfig, ProxyScheme, ProxyState,
};

const TIMEOUT: Duration = Duration::from_millis(700);

fn alive() -> Health {
    Health::Alive { latency_ms: 1 }
}

fn own(port: u16) -> ProxyState {
    ProxyState::Pac(PacConfig::loopback(port))
}

#[test]
fn an_own_address_nothing_listens_on_is_stale() {
    let prober = FakeProber::new();
    assert_eq!(
        stale_hopswitch_pac(&own(8123), &prober, TIMEOUT),
        Some(PacConfig::loopback(8123))
    );
}

#[test]
fn an_own_address_that_is_being_served_is_not_stale() {
    let prober = FakeProber::new().with_local_port(8123, alive());
    assert_eq!(stale_hopswitch_pac(&own(8123), &prober, TIMEOUT), None);
}

#[test]
fn only_the_port_in_the_address_is_asked_about() {
    let prober = FakeProber::new().with_local_port(9999, alive());
    assert!(stale_hopswitch_pac(&own(8123), &prober, TIMEOUT).is_some());
    assert_eq!(prober.calls(), ["local 127.0.0.1:8123 700ms"]);
}

#[test]
fn a_pac_script_somebody_else_set_is_never_judged() {
    let foreign = ProxyState::Pac(PacConfig::new("http://wpad.corp/proxy.pac").unwrap());
    let lookalike = ProxyState::Pac(PacConfig::new("http://localhost:8123/hopswitch.pac").unwrap());
    let prober = FakeProber::new();
    assert_eq!(stale_hopswitch_pac(&foreign, &prober, TIMEOUT), None);
    assert_eq!(stale_hopswitch_pac(&lookalike, &prober, TIMEOUT), None);
    assert_eq!(
        prober.calls(),
        Vec::<String>::new(),
        "nothing should have been probed"
    );
}

#[test]
fn states_that_are_not_a_pac_script_are_never_judged() {
    let manual =
        ProxyState::Manual(ProxyConfig::new(ProxyScheme::Http, "127.0.0.1", 8123).unwrap());
    let other = ProxyState::Other {
        raw: String::new(),
        enabled: true,
    };
    let prober = FakeProber::new();
    for state in [ProxyState::Disabled, manual, other] {
        assert_eq!(
            stale_hopswitch_pac(&state, &prober, TIMEOUT),
            None,
            "{state:?}"
        );
    }
    assert_eq!(prober.calls(), Vec::<String>::new());
}

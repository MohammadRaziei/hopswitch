//! Tests for the system-state types and the in-memory `SystemNetwork` fake.

use hopswitch::core::testing::InMemoryNetwork;
use hopswitch::core::{
    DnsConfig, DnsState, ProxyConfig, ProxyScheme, ProxyState, SystemError, SystemNetwork,
};

fn proxy() -> ProxyConfig {
    ProxyConfig::new(ProxyScheme::Http, "proxy.local", 3128).unwrap()
}

fn dns() -> DnsConfig {
    DnsConfig::from_strs(["9.9.9.9", "149.112.112.112"]).unwrap()
}

#[test]
fn a_fresh_system_has_no_proxy_and_automatic_dns() {
    let net = InMemoryNetwork::new();

    assert_eq!(net.read_proxy().unwrap(), ProxyState::Disabled);
    assert_eq!(net.read_dns().unwrap(), DnsState::Automatic);
}

#[test]
fn written_state_is_read_back_exactly() {
    let net = InMemoryNetwork::new();

    net.write_proxy(&ProxyState::Manual(proxy())).unwrap();
    net.write_dns(&DnsState::Manual(dns())).unwrap();

    assert_eq!(net.read_proxy().unwrap(), ProxyState::Manual(proxy()));
    assert_eq!(net.read_dns().unwrap(), DnsState::Manual(dns()));
}

#[test]
fn foreign_states_we_cannot_model_are_preserved_verbatim() {
    let pac = ProxyState::Other {
        raw: "pac:http://wpad/proxy.pac".into(),
        enabled: true,
    };
    let net = InMemoryNetwork::new().with_proxy(pac.clone());

    assert_eq!(net.read_proxy().unwrap(), pac);
}

#[test]
fn an_injected_write_failure_leaves_state_untouched_and_then_clears() {
    let net = InMemoryNetwork::new();
    net.fail_next_proxy_writes(1, SystemError::PermissionDenied("admin needed".into()));

    let err = net.write_proxy(&ProxyState::Manual(proxy())).unwrap_err();
    assert_eq!(err, SystemError::PermissionDenied("admin needed".into()));
    assert_eq!(net.read_proxy().unwrap(), ProxyState::Disabled);

    net.write_proxy(&ProxyState::Manual(proxy())).unwrap();
    assert_eq!(net.read_proxy().unwrap(), ProxyState::Manual(proxy()));
}

#[test]
fn failure_injection_is_per_kind() {
    let net = InMemoryNetwork::new();
    net.fail_next_dns_writes(1, SystemError::Unsupported("no dns".into()));

    assert!(net.write_proxy(&ProxyState::Manual(proxy())).is_ok());
    assert!(net.write_dns(&DnsState::Manual(dns())).is_err());
}

#[test]
fn counts_reads_and_writes_per_kind() {
    let net = InMemoryNetwork::new();
    net.read_proxy().unwrap();
    net.write_proxy(&ProxyState::Disabled).unwrap();
    net.write_proxy(&ProxyState::Disabled).unwrap();

    assert_eq!(net.proxy_read_count(), 1);
    assert_eq!(net.proxy_write_count(), 2);
    assert_eq!(net.dns_read_count(), 0);
    assert_eq!(net.dns_write_count(), 0);
}

#[test]
fn states_survive_a_json_round_trip() {
    let states = [
        ProxyState::Disabled,
        ProxyState::Manual(proxy()),
        ProxyState::Other {
            raw: "x".into(),
            enabled: true,
        },
        ProxyState::Other {
            raw: "y".into(),
            enabled: false,
        },
    ];
    for s in states {
        let back: ProxyState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    let states = [
        DnsState::Automatic,
        DnsState::Manual(dns()),
        DnsState::Other { raw: "y".into() },
    ];
    for s in states {
        let back: DnsState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}

#[test]
fn deserialising_a_manual_state_revalidates_the_config() {
    let json = r#"{"manual":{"scheme":"http","host":"h","port":0}}"#;
    assert!(serde_json::from_str::<ProxyState>(json).is_err());
}

#[test]
fn a_foreign_state_saved_before_the_enabled_flag_existed_counts_as_enabled() {
    // The safe reading of an old session: assume the foreign state was in use.
    let old = r#"{"other":{"raw":"whatever"}}"#;

    let state: ProxyState = serde_json::from_str(old).unwrap();

    assert_eq!(
        state,
        ProxyState::Other {
            raw: "whatever".into(),
            enabled: true
        }
    );
}

#[test]
fn the_enabled_flag_is_written_out() {
    let off = ProxyState::Other {
        raw: "r".into(),
        enabled: false,
    };

    let json = serde_json::to_string(&off).unwrap();

    assert!(json.contains(r#""enabled":false"#), "{json}");
}

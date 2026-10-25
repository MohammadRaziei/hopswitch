//! Behavioural tests for the proxy and DNS services, run against in-memory fakes.

use std::sync::{Arc, Mutex};

use hopswitch_core::testing::{InMemoryNetwork, InMemoryStore};
use hopswitch_core::{
    restore_all, DnsConfig, DnsProfile, DnsService, DnsState, ProxyConfig, ProxyProfile,
    ProxyScheme, ProxyService, ProxyState, ServiceError, Session, SessionStore, StoreError,
    SystemError, SystemNetwork,
};

fn proxy_config(host: &str) -> ProxyConfig {
    ProxyConfig::new(ProxyScheme::Socks5, host, 1080).unwrap()
}

fn proxy_profile(id: &str, host: &str) -> ProxyProfile {
    ProxyProfile::new(id, id, proxy_config(host)).unwrap()
}

fn dns_config(ip: &str) -> DnsConfig {
    DnsConfig::from_strs([ip]).unwrap()
}

fn dns_profile(id: &str, ip: &str) -> DnsProfile {
    DnsProfile::new(id, id, dns_config(ip)).unwrap()
}

/// The proxy the user already had before Hopswitch touched anything.
fn users_proxy() -> ProxyState {
    ProxyState::Manual(proxy_config("corp.example"))
}

fn denied() -> SystemError {
    SystemError::PermissionDenied("nope".into())
}

struct Proxy {
    net: Arc<InMemoryNetwork>,
    store: Arc<InMemoryStore<ProxyState>>,
    service: ProxyService,
}

fn proxy_setup() -> Proxy {
    let net = Arc::new(InMemoryNetwork::new().with_proxy(users_proxy()));
    let store = Arc::new(InMemoryStore::new());
    let service = ProxyService::new(net.clone(), store.clone());
    Proxy {
        net,
        store,
        service,
    }
}

// ---------- proxy: activation & snapshot ----------

#[test]
fn first_activation_snapshots_the_original_and_applies() {
    let mut p = proxy_setup();

    p.service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap();

    assert_eq!(
        p.net.read_proxy().unwrap(),
        ProxyState::Manual(proxy_config("a.example"))
    );
    assert_eq!(p.service.active_id(), Some("a"));
    let session = p.service.session().unwrap();
    assert_eq!(session.original(), &users_proxy());
    assert_eq!(p.store.saved().as_ref(), Some(session));
}

#[test]
fn the_snapshot_is_persisted_before_anything_is_applied() {
    struct Probe {
        net: Arc<InMemoryNetwork>,
        seen_at_save: Mutex<Vec<ProxyState>>,
    }
    impl SessionStore<ProxyState> for Probe {
        fn save(&self, _: &Session<ProxyState>) -> Result<(), StoreError> {
            self.seen_at_save
                .lock()
                .unwrap()
                .push(self.net.read_proxy().unwrap());
            Ok(())
        }
        fn clear(&self) -> Result<(), StoreError> {
            Ok(())
        }
    }

    let net = Arc::new(InMemoryNetwork::new().with_proxy(users_proxy()));
    let probe = Arc::new(Probe {
        net: net.clone(),
        seen_at_save: Mutex::new(Vec::new()),
    });
    let mut service = ProxyService::new(net.clone(), probe.clone());

    service.activate(&proxy_profile("a", "a.example")).unwrap();

    assert_eq!(*probe.seen_at_save.lock().unwrap(), vec![users_proxy()]);
}

#[test]
fn switching_keeps_the_original_snapshot() {
    let mut p = proxy_setup();

    p.service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap();
    p.service
        .activate(&proxy_profile("b", "b.example"))
        .unwrap();

    assert_eq!(
        p.net.read_proxy().unwrap(),
        ProxyState::Manual(proxy_config("b.example"))
    );
    assert_eq!(p.service.active_id(), Some("b"));
    assert_eq!(p.service.session().unwrap().original(), &users_proxy());
}

#[test]
fn activating_the_same_profile_twice_keeps_the_original() {
    let mut p = proxy_setup();
    let a = proxy_profile("a", "a.example");

    p.service.activate(&a).unwrap();
    p.service.activate(&a).unwrap();

    assert_eq!(p.service.session().unwrap().original(), &users_proxy());
    p.service.deactivate().unwrap();
    assert_eq!(p.net.read_proxy().unwrap(), users_proxy());
}

// ---------- proxy: deactivation ----------

#[test]
fn deactivate_restores_the_exact_previous_state_and_clears_the_session() {
    let mut p = proxy_setup();
    p.service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap();

    p.service.deactivate().unwrap();

    assert_eq!(p.net.read_proxy().unwrap(), users_proxy());
    assert_eq!(p.service.active_id(), None);
    assert!(p.store.saved().is_none());
}

#[test]
fn foreign_states_are_restored_verbatim() {
    let pac = ProxyState::Other {
        raw: "pac:http://wpad/proxy.pac".into(),
        enabled: true,
    };
    let net = Arc::new(InMemoryNetwork::new().with_proxy(pac.clone()));
    let mut service = ProxyService::new(net.clone(), Arc::new(InMemoryStore::new()));

    service.activate(&proxy_profile("a", "a.example")).unwrap();
    service.deactivate().unwrap();

    assert_eq!(net.read_proxy().unwrap(), pac);
}

#[test]
fn deactivating_when_nothing_is_active_does_nothing() {
    let mut p = proxy_setup();

    p.service.deactivate().unwrap();

    assert_eq!(p.net.proxy_write_count(), 0);
    assert_eq!(p.net.read_proxy().unwrap(), users_proxy());
}

#[test]
fn a_new_activation_after_deactivation_takes_a_fresh_snapshot() {
    let mut p = proxy_setup();
    p.service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap();
    p.service.deactivate().unwrap();

    let changed = ProxyState::Manual(proxy_config("changed.example"));
    p.net.write_proxy(&changed).unwrap();
    p.service
        .activate(&proxy_profile("b", "b.example"))
        .unwrap();
    p.service.deactivate().unwrap();

    assert_eq!(p.net.read_proxy().unwrap(), changed);
}

// ---------- proxy: failures ----------

#[test]
fn a_failed_first_activation_rolls_back_and_leaves_nothing_active() {
    let mut p = proxy_setup();
    p.net.fail_next_proxy_writes(1, denied());

    let err = p
        .service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap_err();

    assert_eq!(err, ServiceError::Apply(denied()));
    assert_eq!(p.net.read_proxy().unwrap(), users_proxy());
    assert_eq!(
        p.net.proxy_write_count(),
        2,
        "one failed apply + one rollback"
    );
    assert_eq!(p.service.active_id(), None);
    assert!(p.store.saved().is_none());
}

#[test]
fn a_failed_switch_leaves_the_previous_profile_active() {
    let mut p = proxy_setup();
    p.service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap();
    p.net.fail_next_proxy_writes(1, denied());

    let err = p
        .service
        .activate(&proxy_profile("b", "b.example"))
        .unwrap_err();

    assert_eq!(err, ServiceError::Apply(denied()));
    assert_eq!(
        p.net.read_proxy().unwrap(),
        ProxyState::Manual(proxy_config("a.example"))
    );
    assert_eq!(p.service.active_id(), Some("a"));
    assert_eq!(p.service.session().unwrap().original(), &users_proxy());
}

#[test]
fn when_the_rollback_also_fails_both_errors_are_reported_and_the_session_is_kept() {
    let mut p = proxy_setup();
    p.net.fail_next_proxy_writes(2, denied());

    let err = p
        .service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap_err();

    assert_eq!(
        err,
        ServiceError::RollbackFailed {
            apply: denied(),
            rollback: denied()
        }
    );
    // The snapshot is kept so the user can still restore later.
    assert!(p.service.session().is_some());
    p.service.deactivate().unwrap();
    assert_eq!(p.net.read_proxy().unwrap(), users_proxy());
}

#[test]
fn a_failed_restore_is_reported_and_can_be_retried() {
    let mut p = proxy_setup();
    p.service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap();
    p.net.fail_next_proxy_writes(1, denied());

    let err = p.service.deactivate().unwrap_err();

    assert_eq!(err, ServiceError::Restore(denied()));
    assert_eq!(
        p.service.active_id(),
        Some("a"),
        "still active until restored"
    );
    assert!(p.store.saved().is_some());

    p.service.deactivate().unwrap();
    assert_eq!(p.net.read_proxy().unwrap(), users_proxy());
    assert!(p.store.saved().is_none());
}

#[test]
fn a_read_failure_aborts_before_anything_is_changed() {
    let mut p = proxy_setup();
    p.net.fail_next_proxy_read(denied());

    let err = p
        .service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap_err();

    assert_eq!(err, ServiceError::Read(denied()));
    assert_eq!(p.net.proxy_write_count(), 0);
    assert_eq!(p.service.active_id(), None);
    assert_eq!(p.store.save_count(), 0);
}

#[test]
fn if_the_snapshot_cannot_be_persisted_nothing_is_applied() {
    let mut p = proxy_setup();
    p.store.fail_next_save(StoreError("disk full".into()));

    let err = p
        .service
        .activate(&proxy_profile("a", "a.example"))
        .unwrap_err();

    assert_eq!(err, ServiceError::Store(StoreError("disk full".into())));
    assert_eq!(p.net.proxy_write_count(), 0);
    assert_eq!(p.service.active_id(), None);
}

// ---------- resume after restart ----------

#[test]
fn a_persisted_session_can_be_resumed_and_then_restored() {
    let net =
        Arc::new(InMemoryNetwork::new().with_proxy(ProxyState::Manual(proxy_config("a.example"))));
    let store = Arc::new(InMemoryStore::new());
    let mut service = ProxyService::new(net.clone(), store.clone());

    service.resume(Session::new(users_proxy(), "a"));
    assert_eq!(service.active_id(), Some("a"));

    service.deactivate().unwrap();
    assert_eq!(net.read_proxy().unwrap(), users_proxy());
}

#[test]
fn sessions_survive_a_json_round_trip() {
    let session = Session::new(users_proxy(), "a");
    let back: Session<ProxyState> =
        serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
    assert_eq!(back, session);

    let session = Session::new(DnsState::Manual(dns_config("1.1.1.1")), "d");
    let back: Session<DnsState> =
        serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
    assert_eq!(back, session);
}

// ---------- DNS ----------

struct Both {
    net: Arc<InMemoryNetwork>,
    proxy: ProxyService,
    dns: DnsService,
    dns_store: Arc<InMemoryStore<DnsState>>,
}

fn both_setup() -> Both {
    let net = Arc::new(InMemoryNetwork::new().with_proxy(users_proxy()));
    let dns_store = Arc::new(InMemoryStore::new());
    Both {
        proxy: ProxyService::new(net.clone(), Arc::new(InMemoryStore::new())),
        dns: DnsService::new(net.clone(), dns_store.clone()),
        dns_store,
        net,
    }
}

#[test]
fn dns_activation_snapshots_applies_and_restores() {
    let mut b = both_setup();

    b.dns.activate(&dns_profile("d", "1.1.1.1")).unwrap();
    assert_eq!(
        b.net.read_dns().unwrap(),
        DnsState::Manual(dns_config("1.1.1.1"))
    );
    assert_eq!(b.dns.session().unwrap().original(), &DnsState::Automatic);

    b.dns.deactivate().unwrap();
    assert_eq!(b.net.read_dns().unwrap(), DnsState::Automatic);
    assert!(b.dns_store.saved().is_none());
}

// ---------- independence ----------

#[test]
fn turning_dns_off_never_touches_the_proxy_and_vice_versa() {
    let mut b = both_setup();
    b.proxy.activate(&proxy_profile("a", "a.example")).unwrap();
    b.dns.activate(&dns_profile("d", "1.1.1.1")).unwrap();
    let proxy_writes = b.net.proxy_write_count();
    let dns_writes = b.net.dns_write_count();

    b.dns.deactivate().unwrap();
    assert_eq!(b.net.proxy_write_count(), proxy_writes);
    assert_eq!(
        b.net.read_proxy().unwrap(),
        ProxyState::Manual(proxy_config("a.example"))
    );
    assert_eq!(b.proxy.active_id(), Some("a"));

    b.dns.activate(&dns_profile("d", "1.1.1.1")).unwrap();
    let dns_writes_after = b.net.dns_write_count();
    assert!(dns_writes_after > dns_writes);
    b.proxy.deactivate().unwrap();
    assert_eq!(b.net.dns_write_count(), dns_writes_after);
    assert_eq!(b.dns.active_id(), Some("d"));
}

#[test]
fn a_dns_failure_does_not_disturb_an_active_proxy() {
    let mut b = both_setup();
    b.proxy.activate(&proxy_profile("a", "a.example")).unwrap();
    b.net.fail_next_dns_writes(1, denied());

    assert!(b.dns.activate(&dns_profile("d", "1.1.1.1")).is_err());

    assert_eq!(b.proxy.active_id(), Some("a"));
    assert_eq!(
        b.net.read_proxy().unwrap(),
        ProxyState::Manual(proxy_config("a.example"))
    );
}

// ---------- restore on quit ----------

#[test]
fn restore_all_restores_both_kinds() {
    let mut b = both_setup();
    b.proxy.activate(&proxy_profile("a", "a.example")).unwrap();
    b.dns.activate(&dns_profile("d", "1.1.1.1")).unwrap();

    restore_all(&mut b.proxy, &mut b.dns).unwrap();

    assert_eq!(b.net.read_proxy().unwrap(), users_proxy());
    assert_eq!(b.net.read_dns().unwrap(), DnsState::Automatic);
}

#[test]
fn restore_all_still_restores_dns_when_the_proxy_restore_fails() {
    let mut b = both_setup();
    b.proxy.activate(&proxy_profile("a", "a.example")).unwrap();
    b.dns.activate(&dns_profile("d", "1.1.1.1")).unwrap();
    b.net.fail_next_proxy_writes(1, denied());

    let err = restore_all(&mut b.proxy, &mut b.dns).unwrap_err();

    assert_eq!(err.proxy, Some(ServiceError::Restore(denied())));
    assert_eq!(err.dns, None);
    assert_eq!(b.net.read_dns().unwrap(), DnsState::Automatic);
}

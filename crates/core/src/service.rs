//! Activation services: one for the system proxy, one for system DNS.
//!
//! Both follow the same rules and are fully independent of each other:
//!
//! * the first activation snapshots the real state and persists it *before*
//!   changing anything;
//! * switching A → B keeps the original snapshot;
//! * deactivating restores the snapshot exactly;
//! * a failed apply is rolled back to the state it started from.

use std::fmt::{self, Debug};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

use crate::{
    DnsProfile, DnsState, ProxyProfile, ProxyState, Session, SessionStore, StoreError, SystemError,
    SystemNetwork,
};

/// Why a service operation failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServiceError {
    /// The current system state could not be read; nothing was changed.
    #[error("could not read the current system settings: {0}")]
    Read(#[source] SystemError),
    /// Applying failed; the previous state was restored.
    #[error("could not apply the settings: {0}")]
    Apply(#[source] SystemError),
    /// Applying failed and so did restoring; the system may be inconsistent.
    #[error("applying failed ({apply}) and rolling back also failed ({rollback})")]
    RollbackFailed {
        /// The original apply failure.
        apply: SystemError,
        /// The failure while rolling back.
        rollback: SystemError,
    },
    /// Restoring the original settings failed; the session is kept for a retry.
    #[error("could not restore the original settings: {0}")]
    Restore(#[source] SystemError),
    /// The session could not be persisted or cleared.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Describes one kind of setting (proxy or DNS) to the generic [`NetworkService`].
pub trait Kind {
    /// The system state for this kind.
    type State: Clone + PartialEq + Debug + Serialize + DeserializeOwned + Send + Sync + 'static;
    /// The saved profile for this kind.
    type Profile;

    /// Reads the current state.
    fn read(network: &dyn SystemNetwork) -> Result<Self::State, SystemError>;
    /// Writes `state`.
    fn write(network: &dyn SystemNetwork, state: &Self::State) -> Result<(), SystemError>;
    /// The profile's id.
    fn id(profile: &Self::Profile) -> &str;
    /// The state the profile asks for.
    fn target(profile: &Self::Profile) -> Self::State;
}

/// Marker for the system proxy.
#[derive(Debug, Clone, Copy)]
pub struct ProxyKind;

/// Marker for system DNS.
#[derive(Debug, Clone, Copy)]
pub struct DnsKind;

impl Kind for ProxyKind {
    type State = ProxyState;
    type Profile = ProxyProfile;

    fn read(network: &dyn SystemNetwork) -> Result<ProxyState, SystemError> {
        network.read_proxy()
    }

    fn write(network: &dyn SystemNetwork, state: &ProxyState) -> Result<(), SystemError> {
        network.write_proxy(state)
    }

    fn id(profile: &ProxyProfile) -> &str {
        profile.id()
    }

    fn target(profile: &ProxyProfile) -> ProxyState {
        ProxyState::Manual(profile.config().clone())
    }
}

impl Kind for DnsKind {
    type State = DnsState;
    type Profile = DnsProfile;

    fn read(network: &dyn SystemNetwork) -> Result<DnsState, SystemError> {
        network.read_dns()
    }

    fn write(network: &dyn SystemNetwork, state: &DnsState) -> Result<(), SystemError> {
        network.write_dns(state)
    }

    fn id(profile: &DnsProfile) -> &str {
        profile.id()
    }

    fn target(profile: &DnsProfile) -> DnsState {
        DnsState::Manual(profile.config().clone())
    }
}

/// Activates and deactivates profiles of one [`Kind`].
pub struct NetworkService<K: Kind> {
    network: Arc<dyn SystemNetwork>,
    store: Arc<dyn SessionStore<K::State>>,
    session: Option<Session<K::State>>,
}

/// Service for the system proxy.
pub type ProxyService = NetworkService<ProxyKind>;
/// Service for system DNS.
pub type DnsService = NetworkService<DnsKind>;

impl<K: Kind> std::fmt::Debug for NetworkService<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The system and the store are trait objects; whether something is active is what matters.
        f.debug_struct("NetworkService")
            .field("active", &self.session.is_some())
            .finish_non_exhaustive()
    }
}

impl<K: Kind> NetworkService<K> {
    /// A service with nothing active.
    pub fn new(network: Arc<dyn SystemNetwork>, store: Arc<dyn SessionStore<K::State>>) -> Self {
        Self {
            network,
            store,
            session: None,
        }
    }

    /// Continues a session persisted by an earlier run, without touching the system.
    pub fn resume(&mut self, session: Session<K::State>) {
        self.session = Some(session);
    }

    /// Id of the active profile, if any.
    pub fn active_id(&self) -> Option<&str> {
        self.session.as_ref().map(Session::active_id)
    }

    /// The current session (original snapshot + active id), if any.
    pub fn session(&self) -> Option<&Session<K::State>> {
        self.session.as_ref()
    }

    /// Applies `profile`, snapshotting the real state first if nothing is active.
    ///
    /// On failure the system is returned to the state it was in before the call.
    /// Activating while another profile is active switches without losing the
    /// original snapshot.
    pub fn activate(&mut self, profile: &K::Profile) -> Result<(), ServiceError> {
        let id = K::id(profile);
        let before = K::read(&*self.network).map_err(ServiceError::Read)?;

        let first_activation = self.session.is_none();
        if first_activation {
            // Persist the snapshot before the first change so a crash can't lose it.
            let session = Session::new(before.clone(), id);
            self.store.save(&session)?;
            self.session = Some(session);
        }

        if let Err(apply) = K::write(&*self.network, &K::target(profile)) {
            return Err(match K::write(&*self.network, &before) {
                Ok(()) => {
                    if first_activation {
                        self.session = None;
                        // Best effort: the system already equals the snapshot, so a stale
                        // stored session is harmless and the apply error matters more.
                        let _ = self.store.clear();
                    }
                    ServiceError::Apply(apply)
                }
                Err(rollback) => ServiceError::RollbackFailed { apply, rollback },
            });
        }

        if let Some(session) = self.session.as_mut() {
            if session.active_id() != id {
                session.set_active_id(id);
                self.store.save(session)?;
            }
        }
        Ok(())
    }

    /// Restores the original snapshot and clears the session.
    ///
    /// Does nothing if nothing is active. If restoring fails the session is kept,
    /// so the call can be retried.
    pub fn deactivate(&mut self) -> Result<(), ServiceError> {
        let Some(session) = self.session.as_ref() else {
            return Ok(());
        };
        K::write(&*self.network, session.original()).map_err(ServiceError::Restore)?;
        self.session = None;
        self.store.clear()?;
        Ok(())
    }
}

/// Which parts of [`restore_all`] failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreAllError {
    /// The proxy restore failure, if any.
    pub proxy: Option<ServiceError>,
    /// The DNS restore failure, if any.
    pub dns: Option<ServiceError>,
}

impl fmt::Display for RestoreAllError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.proxy, &self.dns) {
            (Some(p), Some(d)) => write!(f, "proxy restore failed ({p}); DNS restore failed ({d})"),
            (Some(p), None) => write!(f, "proxy restore failed ({p})"),
            (None, Some(d)) => write!(f, "DNS restore failed ({d})"),
            (None, None) => write!(f, "nothing failed"),
        }
    }
}

impl std::error::Error for RestoreAllError {}

/// Restores both proxy and DNS ("restore on quit").
///
/// Always attempts both, even if the first fails.
pub fn restore_all(
    proxy: &mut ProxyService,
    dns: &mut DnsService,
) -> Result<(), Box<RestoreAllError>> {
    let proxy = proxy.deactivate().err();
    let dns = dns.deactivate().err();
    if proxy.is_none() && dns.is_none() {
        Ok(())
    } else {
        Err(Box::new(RestoreAllError { proxy, dns }))
    }
}

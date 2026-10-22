//! Test doubles for the ports in `core`, usable from other crates' tests.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::{DnsState, ProxyState, Session, SessionStore, StoreError, SystemError, SystemNetwork};

/// One independently-faultable setting (proxy or DNS).
struct Channel<S> {
    state: S,
    reads: usize,
    writes: usize,
    read_fault: Option<SystemError>,
    write_fault: Option<(usize, SystemError)>,
}

impl<S: Clone> Channel<S> {
    fn new(state: S) -> Self {
        Self {
            state,
            reads: 0,
            writes: 0,
            read_fault: None,
            write_fault: None,
        }
    }

    fn read(&mut self) -> Result<S, SystemError> {
        self.reads += 1;
        match self.read_fault.take() {
            Some(error) => Err(error),
            None => Ok(self.state.clone()),
        }
    }

    fn write(&mut self, state: &S) -> Result<(), SystemError> {
        self.writes += 1;
        if let Some((remaining, error)) = self.write_fault.take() {
            if remaining > 1 {
                self.write_fault = Some((remaining - 1, error.clone()));
            }
            return Err(error);
        }
        self.state = state.clone();
        Ok(())
    }
}

struct Inner {
    proxy: Channel<ProxyState>,
    dns: Channel<DnsState>,
}

/// An in-memory [`SystemNetwork`] with failure injection and call counters.
///
/// Starts with no proxy and automatic DNS.
pub struct InMemoryNetwork {
    inner: Mutex<Inner>,
}

impl Default for InMemoryNetwork {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryNetwork {
    /// A fresh system: proxy disabled, DNS automatic.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                proxy: Channel::new(ProxyState::Disabled),
                dns: Channel::new(DnsState::Automatic),
            }),
        }
    }

    /// Starts with the given proxy state.
    #[must_use]
    pub fn with_proxy(self, state: ProxyState) -> Self {
        self.lock().proxy.state = state;
        self
    }

    /// Starts with the given DNS state.
    #[must_use]
    pub fn with_dns(self, state: DnsState) -> Self {
        self.lock().dns.state = state;
        self
    }

    /// Makes the next `count` proxy writes fail with `error` (state is untouched).
    pub fn fail_next_proxy_writes(&self, count: usize, error: SystemError) {
        self.lock().proxy.write_fault = (count > 0).then_some((count, error));
    }

    /// Makes the next `count` DNS writes fail with `error` (state is untouched).
    pub fn fail_next_dns_writes(&self, count: usize, error: SystemError) {
        self.lock().dns.write_fault = (count > 0).then_some((count, error));
    }

    /// Makes the next proxy read fail with `error`.
    pub fn fail_next_proxy_read(&self, error: SystemError) {
        self.lock().proxy.read_fault = Some(error);
    }

    /// Makes the next DNS read fail with `error`.
    pub fn fail_next_dns_read(&self, error: SystemError) {
        self.lock().dns.read_fault = Some(error);
    }

    /// Number of proxy reads so far (including failed ones).
    pub fn proxy_read_count(&self) -> usize {
        self.lock().proxy.reads
    }

    /// Number of proxy writes so far (including failed ones).
    pub fn proxy_write_count(&self) -> usize {
        self.lock().proxy.writes
    }

    /// Number of DNS reads so far (including failed ones).
    pub fn dns_read_count(&self) -> usize {
        self.lock().dns.reads
    }

    /// Number of DNS writes so far (including failed ones).
    pub fn dns_write_count(&self) -> usize {
        self.lock().dns.writes
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SystemNetwork for InMemoryNetwork {
    fn read_proxy(&self) -> Result<ProxyState, SystemError> {
        self.lock().proxy.read()
    }

    fn write_proxy(&self, state: &ProxyState) -> Result<(), SystemError> {
        self.lock().proxy.write(state)
    }

    fn read_dns(&self) -> Result<DnsState, SystemError> {
        self.lock().dns.read()
    }

    fn write_dns(&self, state: &DnsState) -> Result<(), SystemError> {
        self.lock().dns.write(state)
    }
}

struct StoreInner<S> {
    saved: Option<Session<S>>,
    saves: usize,
    clears: usize,
    save_fault: Option<StoreError>,
    clear_fault: Option<StoreError>,
}

/// An in-memory [`SessionStore`] with failure injection and call counters.
pub struct InMemoryStore<S> {
    inner: Mutex<StoreInner<S>>,
}

impl<S> Default for InMemoryStore<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> InMemoryStore<S> {
    /// An empty store.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(StoreInner {
                saved: None,
                saves: 0,
                clears: 0,
                save_fault: None,
                clear_fault: None,
            }),
        }
    }

    /// Makes the next `save` fail with `error`.
    pub fn fail_next_save(&self, error: StoreError) {
        self.lock().save_fault = Some(error);
    }

    /// Makes the next `clear` fail with `error`.
    pub fn fail_next_clear(&self, error: StoreError) {
        self.lock().clear_fault = Some(error);
    }

    /// Number of `save` calls so far (including failed ones).
    pub fn save_count(&self) -> usize {
        self.lock().saves
    }

    /// Number of `clear` calls so far (including failed ones).
    pub fn clear_count(&self) -> usize {
        self.lock().clears
    }

    fn lock(&self) -> MutexGuard<'_, StoreInner<S>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<S: Clone> InMemoryStore<S> {
    /// The currently stored session, if any.
    pub fn saved(&self) -> Option<Session<S>> {
        self.lock().saved.clone()
    }
}

impl<S: Clone + Send> SessionStore<S> for InMemoryStore<S> {
    fn save(&self, session: &Session<S>) -> Result<(), StoreError> {
        let mut inner = self.lock();
        inner.saves += 1;
        if let Some(error) = inner.save_fault.take() {
            return Err(error);
        }
        inner.saved = Some(session.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        let mut inner = self.lock();
        inner.clears += 1;
        if let Some(error) = inner.clear_fault.take() {
            return Err(error);
        }
        inner.saved = None;
        Ok(())
    }
}

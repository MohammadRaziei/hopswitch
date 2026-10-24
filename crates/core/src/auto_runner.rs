//! One round of auto connect / auto switch: probe the pool, decide, apply.
//!
//! The decision logic is [`Autopilot`]; applying goes through the normal
//! services, so the original settings are always snapshotted and restorable.
//! Proxy and DNS are independent: a failure in one never blocks the other.

use crate::{
    probe_dns_profiles, probe_proxies, AutoConfig, AutoKind, Autopilot, Decision, DnsService,
    Probe, Prober, Profiles, ProxyService,
};

/// What happened for one kind during a round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindReport {
    /// The probe results of the pool (empty if the kind is off).
    pub probes: Vec<Probe>,
    /// What the engine decided.
    pub decision: Decision,
    /// Why applying the decision failed, if it did.
    pub error: Option<String>,
}

impl KindReport {
    fn idle() -> Self {
        Self {
            probes: Vec::new(),
            decision: Decision::Keep,
            error: None,
        }
    }
}

/// The outcome of one round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickReport {
    /// The proxy part.
    pub proxy: KindReport,
    /// The DNS part.
    pub dns: KindReport,
}

/// Remembers failure streaks between rounds and runs them.
#[derive(Debug, Clone)]
pub struct AutoRunner {
    proxy: Autopilot,
    dns: Autopilot,
    decisive: bool,
}

impl Default for AutoRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoRunner {
    /// A runner that gives up on a profile only after several failures in a row.
    pub fn new() -> Self {
        let settings = AutoKind::default().settings();
        Self {
            proxy: Autopilot::new(settings),
            dns: Autopilot::new(settings),
            decisive: false,
        }
    }

    /// For a single round with no memory of earlier ones: one failure is enough.
    pub fn decisive() -> Self {
        Self {
            decisive: true,
            ..Self::new()
        }
    }

    /// Runs one round with the current `config` and `profiles`.
    pub fn tick(
        &mut self,
        proxy: &mut ProxyService,
        dns: &mut DnsService,
        prober: &dyn Prober,
        profiles: &Profiles,
        config: &AutoConfig,
    ) -> TickReport {
        TickReport {
            proxy: self.tick_proxy(proxy, prober, profiles, config),
            dns: self.tick_dns(dns, prober, profiles, config),
        }
    }

    fn settings_for(&self, kind: &AutoKind) -> crate::AutoSettings {
        let settings = kind.settings();
        if self.decisive {
            settings.with_failures(1).unwrap_or(settings)
        } else {
            settings
        }
    }

    fn tick_proxy(
        &mut self,
        service: &mut ProxyService,
        prober: &dyn Prober,
        profiles: &Profiles,
        config: &AutoConfig,
    ) -> KindReport {
        if !config.proxy.is_enabled() {
            return KindReport::idle();
        }
        let pool: Vec<_> = config
            .proxy
            .pool
            .iter()
            .filter_map(|id| profiles.proxy(id).cloned())
            .collect();
        let results = probe_proxies(prober, &pool, &config.connectivity);
        self.proxy.configure(self.settings_for(&config.proxy));
        let decision = self.proxy.observe(service.active_id(), &results);
        let result = match &decision {
            Decision::Keep => Ok(()),
            Decision::Activate(id) => pool
                .iter()
                .find(|profile| profile.id() == id)
                .map_or(Ok(()), |profile| service.activate(profile)),
            Decision::Deactivate => service.deactivate(),
        };
        KindReport {
            probes: results,
            decision,
            error: result.err().map(|e| e.to_string()),
        }
    }

    fn tick_dns(
        &mut self,
        service: &mut DnsService,
        prober: &dyn Prober,
        profiles: &Profiles,
        config: &AutoConfig,
    ) -> KindReport {
        if !config.dns.is_enabled() {
            return KindReport::idle();
        }
        let pool: Vec<_> = config
            .dns
            .pool
            .iter()
            .filter_map(|id| profiles.dns(id).cloned())
            .collect();
        let results = probe_dns_profiles(prober, &pool, &config.connectivity);
        self.dns.configure(self.settings_for(&config.dns));
        let decision = self.dns.observe(service.active_id(), &results);
        let result = match &decision {
            Decision::Keep => Ok(()),
            Decision::Activate(id) => pool
                .iter()
                .find(|profile| profile.id() == id)
                .map_or(Ok(()), |profile| service.activate(profile)),
            Decision::Deactivate => service.deactivate(),
        };
        KindReport {
            probes: results,
            decision,
            error: result.err().map(|e| e.to_string()),
        }
    }
}

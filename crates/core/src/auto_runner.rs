//! One round of auto connect / auto switch: probe the pool, decide, apply.
//!
//! The decision logic is [`Autopilot`]; applying goes through the normal
//! services, so the original settings are always snapshotted and restorable.
//! Proxy and DNS are independent: a failure in one never blocks the other.
//!
//! The proxy follows the *selected* profile (ADR-0010): a plain proxy is connected
//! while it answers; a group is connected to its fastest live member, with the same
//! hysteresis and failover as before. DNS follows its own options and pool.

use crate::{
    probe_dns_profiles, probe_proxies, AutoConfig, AutoSettings, Autopilot, Decision, DnsService,
    Probe, Prober, Profiles, ProxyAuto, ProxyEntry, ProxyProfile, ProxyService,
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
        let settings = AutoSettings::new(false, false);
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

    fn decisive_if_asked(&self, settings: AutoSettings) -> AutoSettings {
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
        let Some((pool, settings)) = proxy_plan(profiles, &config.proxy) else {
            return KindReport::idle();
        };
        let results = probe_proxies(prober, &pool, &config.connectivity);
        self.proxy.configure(self.decisive_if_asked(settings));
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
        self.dns
            .configure(self.decisive_if_asked(config.dns.settings()));
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

/// What the proxy automation works on: the profiles to probe (in priority order) and
/// how to decide. `None` when there is nothing to do.
fn proxy_plan(profiles: &Profiles, auto: &ProxyAuto) -> Option<(Vec<ProxyProfile>, AutoSettings)> {
    if !auto.is_enabled() {
        return None;
    }
    match profiles.proxy_entry(auto.selected.as_deref()?)? {
        // One proxy: connect while it answers, give up after repeated failures.
        ProxyEntry::Proxy(proxy) => Some((vec![proxy.clone()], AutoSettings::new(true, false))),
        // A group: the fastest live member, with switching and failover.
        ProxyEntry::Group(group) => {
            let members = group
                .members()
                .iter()
                .filter_map(|id| profiles.proxy(id).cloned())
                .collect();
            Some((members, AutoSettings::new(true, true)))
        }
    }
}

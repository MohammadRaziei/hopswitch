//! From a saved [`RulesProfile`] to the [`PacPlan`] that carries it to the system (ADR-0011).
//!
//! Two decisions live here, because this is the code that builds the plan for the system:
//!
//! * a group target becomes whichever member answers fastest *right now* (the same choice
//!   `proxy use <group>` makes);
//! * a profile that could ever send traffic through SOCKS is refused (fail closed): Windows' own
//!   PAC engine answers "no proxy" for a `SOCKS5` answer, so the traffic would silently go direct.
//!   "Could ever" means every member of a used group, not only the one that is fastest today, since
//!   the fastest member changes and a script must not turn unsafe later.

use std::collections::HashMap;

use thiserror::Error;

use crate::core::{
    fastest_alive, probe_proxies, ConnectivitySettings, PacError, PacPlan, PacRoute, Prober,
    Profiles, ProxyEntry, ProxyGroup, ProxyProfile, ProxyScheme, RulesProfile, Target,
};

/// Why a rules profile cannot be turned into a PAC script.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RulesApplyError {
    /// A target is not a saved proxy or group.
    #[error("the profile points at `{0}`, which is not a saved proxy or group")]
    MissingTarget(String),
    /// A target is, or a group target contains, a SOCKS proxy.
    #[error(
        "refused: {} would carry traffic over SOCKS, and Windows' own proxy engine ignores SOCKS in a PAC \
         script (the traffic would silently go direct instead of through the proxy). Use the HTTP \
         port of the same tool instead (v2rayN: 10809, Clash: 7890)",
        .proxies.join(", ")
    )]
    Socks {
        /// Names of the SOCKS proxies, in the order the profile reaches them.
        proxies: Vec<String>,
    },
    /// None of a group's members answers, so there is no route to put in the script.
    #[error("no member of group `{group}` answers right now; nothing was changed")]
    NoLiveMember {
        /// The group's name.
        group: String,
    },
    /// The script could not be built.
    #[error(transparent)]
    Pac(#[from] PacError),
}

/// What the profile's targets stand for.
struct Used<'a> {
    /// Plain proxies named as a target, and groups named as a target, first use first.
    proxies: Vec<&'a ProxyProfile>,
    groups: Vec<&'a ProxyGroup>,
}

fn used_by<'a>(
    profile: &RulesProfile,
    profiles: &'a Profiles,
) -> Result<Used<'a>, RulesApplyError> {
    let mut used = Used {
        proxies: Vec::new(),
        groups: Vec::new(),
    };
    let targets = std::iter::once(profile.default_target())
        .chain(profile.rules().iter().map(crate::core::Rule::target));
    for target in targets {
        let Target::Proxy(id) = target else { continue };
        match profiles.proxy_entry(id) {
            Some(ProxyEntry::Proxy(p)) if !used.proxies.iter().any(|u| u.id() == p.id()) => {
                used.proxies.push(p);
            }
            Some(ProxyEntry::Group(g)) if !used.groups.iter().any(|u| u.id() == g.id()) => {
                used.groups.push(g);
            }
            Some(_) => {}
            None => return Err(RulesApplyError::MissingTarget(id.clone())),
        }
    }
    Ok(used)
}

fn members_of<'a>(group: &ProxyGroup, profiles: &'a Profiles) -> Vec<&'a ProxyProfile> {
    group
        .members()
        .iter()
        .filter_map(|id| profiles.proxy(id))
        .collect()
}

/// Builds the PAC plan for `profile`. Nothing is probed unless the profile uses a group.
///
/// # Errors
/// See [`RulesApplyError`]. SOCKS is checked first, before anything is probed.
pub fn plan_rules_pac(
    profile: &RulesProfile,
    profiles: &Profiles,
    prober: &dyn Prober,
    settings: &ConnectivitySettings,
) -> Result<PacPlan, RulesApplyError> {
    let used = used_by(profile, profiles)?;

    let mut socks: Vec<String> = Vec::new();
    let candidates = used.proxies.iter().copied().chain(
        used.groups
            .iter()
            .flat_map(|group| members_of(group, profiles)),
    );
    for candidate in candidates {
        let is_socks = matches!(
            candidate.config().scheme(),
            ProxyScheme::Socks4 | ProxyScheme::Socks5
        );
        if is_socks && !socks.iter().any(|name| name == candidate.name()) {
            socks.push(candidate.name().to_owned());
        }
    }
    if !socks.is_empty() {
        return Err(RulesApplyError::Socks { proxies: socks });
    }

    let mut chosen: HashMap<&str, ProxyProfile> = HashMap::new();
    for group in &used.groups {
        let members: Vec<ProxyProfile> = members_of(group, profiles).into_iter().cloned().collect();
        let answers = probe_proxies(prober, &members, settings);
        let best = fastest_alive(&answers)
            .and_then(|(probe, _)| members.iter().find(|m| m.id() == probe.id))
            .ok_or_else(|| RulesApplyError::NoLiveMember {
                group: group.name().to_owned(),
            })?;
        chosen.insert(group.id(), best.clone());
    }

    let resolve = |target: &Target| match target {
        Target::Direct => Some(PacRoute::Direct),
        Target::Proxy(id) => match profiles.proxy_entry(id)? {
            ProxyEntry::Proxy(p) => Some(PacRoute::Proxy(p.config().clone())),
            ProxyEntry::Group(g) => chosen
                .get(g.id())
                .map(|p| PacRoute::Proxy(p.config().clone())),
        },
    };
    Ok(PacPlan::from_profile(profile, resolve)?)
}

//! The user's saved proxy and DNS profiles, as one validated collection.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DnsProfile, ProxyGroup, ProxyProfile, RulesProfile, Target};

/// Why a collection operation failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProfilesError {
    /// A profile with this id already exists in that list.
    #[error("a profile with id `{0}` already exists")]
    DuplicateId(String),
    /// No profile has this id.
    #[error("no profile with id `{0}`")]
    NotFound(String),
    /// A group lists a member that is not a saved proxy (a group is not a valid member).
    #[error("group `{group}` lists `{member}`, which is not a saved proxy")]
    UnknownMember {
        /// The group.
        group: String,
        /// The missing member.
        member: String,
    },
    /// A rules profile sends traffic to a target that is not a saved proxy or group (a rules
    /// profile is not a valid target).
    #[error("rules profile `{profile}` points at `{target}`, which is not a saved proxy or group")]
    UnknownTarget {
        /// The rules profile.
        profile: String,
        /// The missing target.
        target: String,
    },
    /// The proxy or group cannot be removed because groups or rules profiles still use it.
    #[error("`{id}` is used by: {}", by.join(", "))]
    InUse {
        /// The proxy or group.
        id: String,
        /// Ids of the groups and rules profiles that use it.
        by: Vec<String>,
    },
}

/// What a proxy id stands for: a plain proxy or a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyEntry<'a> {
    /// A plain proxy.
    Proxy(&'a ProxyProfile),
    /// A group of proxies.
    Group(&'a ProxyGroup),
}

/// All saved profiles. Proxy, group and rules-profile ids share one namespace, DNS ids have their
/// own; insertion order is preserved. Every group member is an existing plain proxy and every
/// rules target is an existing proxy or group. Can only be built valid, including via serde.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProfilesDto")]
pub struct Profiles {
    proxies: Vec<ProxyProfile>,
    groups: Vec<ProxyGroup>,
    rules: Vec<RulesProfile>,
    dns: Vec<DnsProfile>,
}

/// Untrusted wire representation; converted through the uniqueness check.
#[derive(Deserialize)]
struct ProfilesDto {
    #[serde(default)]
    proxies: Vec<ProxyProfile>,
    #[serde(default)]
    groups: Vec<ProxyGroup>,
    #[serde(default)]
    rules: Vec<RulesProfile>,
    #[serde(default)]
    dns: Vec<DnsProfile>,
}

impl TryFrom<ProfilesDto> for Profiles {
    type Error = ProfilesError;

    fn try_from(dto: ProfilesDto) -> Result<Self, Self::Error> {
        ensure_unique(
            dto.proxies
                .iter()
                .map(ProxyProfile::id)
                .chain(dto.groups.iter().map(ProxyGroup::id))
                .chain(dto.rules.iter().map(RulesProfile::id)),
        )?;
        ensure_unique(dto.dns.iter().map(DnsProfile::id))?;
        let profiles = Self {
            proxies: dto.proxies,
            groups: dto.groups,
            rules: dto.rules,
            dns: dto.dns,
        };
        for group in &profiles.groups {
            profiles.check_members(group)?;
        }
        for rules in &profiles.rules {
            profiles.check_targets(rules)?;
        }
        Ok(profiles)
    }
}

fn ensure_unique<'a>(ids: impl Iterator<Item = &'a str>) -> Result<(), ProfilesError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(ProfilesError::DuplicateId(id.to_owned()));
        }
    }
    Ok(())
}

impl Profiles {
    /// An empty collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// All proxy profiles, in insertion order.
    pub fn proxy_profiles(&self) -> &[ProxyProfile] {
        &self.proxies
    }

    /// All DNS profiles, in insertion order.
    pub fn dns_profiles(&self) -> &[DnsProfile] {
        &self.dns
    }

    /// All proxy groups, in insertion order.
    pub fn groups(&self) -> &[ProxyGroup] {
        &self.groups
    }

    /// All rules profiles, in insertion order.
    pub fn rules_profiles(&self) -> &[RulesProfile] {
        &self.rules
    }

    /// The rules profile with this id.
    pub fn rules_profile(&self, id: &str) -> Option<&RulesProfile> {
        self.rules.iter().find(|r| r.id() == id)
    }

    /// The proxy group with this id.
    pub fn group(&self, id: &str) -> Option<&ProxyGroup> {
        self.groups.iter().find(|g| g.id() == id)
    }

    /// Whatever `id` names among proxies and groups.
    pub fn proxy_entry(&self, id: &str) -> Option<ProxyEntry<'_>> {
        self.proxy(id)
            .map(ProxyEntry::Proxy)
            .or_else(|| self.group(id).map(ProxyEntry::Group))
    }

    /// Ids of the groups that list the proxy `id` as a member.
    pub fn groups_using(&self, id: &str) -> Vec<&str> {
        self.groups
            .iter()
            .filter(|g| g.members().iter().any(|m| m == id))
            .map(ProxyGroup::id)
            .collect()
    }

    /// Whether a proxy, group or rules profile already has this id.
    fn id_taken(&self, id: &str) -> bool {
        self.proxy_entry(id).is_some() || self.rules_profile(id).is_some()
    }

    /// Ids of the rules profiles that send traffic to the proxy or group `id`.
    fn rules_using(&self, id: &str) -> Vec<&str> {
        let is_it = |target: &Target| matches!(target, Target::Proxy(t) if t == id);
        self.rules
            .iter()
            .filter(|r| is_it(r.default_target()) || r.rules().iter().any(|x| is_it(x.target())))
            .map(RulesProfile::id)
            .collect()
    }

    fn check_targets(&self, profile: &RulesProfile) -> Result<(), ProfilesError> {
        let targets = profile
            .rules()
            .iter()
            .map(crate::Rule::target)
            .chain(std::iter::once(profile.default_target()));
        for target in targets {
            if let Target::Proxy(id) = target {
                if self.proxy_entry(id).is_none() {
                    return Err(ProfilesError::UnknownTarget {
                        profile: profile.id().to_owned(),
                        target: id.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn check_members(&self, group: &ProxyGroup) -> Result<(), ProfilesError> {
        match group.members().iter().find(|m| self.proxy(m).is_none()) {
            Some(member) => Err(ProfilesError::UnknownMember {
                group: group.id().to_owned(),
                member: member.clone(),
            }),
            None => Ok(()),
        }
    }

    /// The proxy profile with this id.
    pub fn proxy(&self, id: &str) -> Option<&ProxyProfile> {
        self.proxies.iter().find(|p| p.id() == id)
    }

    /// The DNS profile with this id.
    pub fn dns(&self, id: &str) -> Option<&DnsProfile> {
        self.dns.iter().find(|p| p.id() == id)
    }

    /// Appends a proxy profile; its id must be new.
    pub fn add_proxy(&mut self, profile: ProxyProfile) -> Result<(), ProfilesError> {
        if self.id_taken(profile.id()) {
            return Err(ProfilesError::DuplicateId(profile.id().to_owned()));
        }
        self.proxies.push(profile);
        Ok(())
    }

    /// Appends a group; its id must be new (among proxies and groups) and every
    /// member must be a saved proxy.
    pub fn add_group(&mut self, group: ProxyGroup) -> Result<(), ProfilesError> {
        if self.id_taken(group.id()) {
            return Err(ProfilesError::DuplicateId(group.id().to_owned()));
        }
        self.check_members(&group)?;
        self.groups.push(group);
        Ok(())
    }

    /// Replaces the group with the same id, keeping its position.
    pub fn replace_group(&mut self, group: ProxyGroup) -> Result<(), ProfilesError> {
        let index = self
            .groups
            .iter()
            .position(|g| g.id() == group.id())
            .ok_or_else(|| ProfilesError::NotFound(group.id().to_owned()))?;
        self.check_members(&group)?;
        self.groups[index] = group;
        Ok(())
    }

    /// Removes and returns the group with this id. Refused while a rules profile uses it.
    pub fn remove_group(&mut self, id: &str) -> Result<ProxyGroup, ProfilesError> {
        let by: Vec<String> = self
            .rules_using(id)
            .into_iter()
            .map(str::to_owned)
            .collect();
        if !by.is_empty() {
            return Err(ProfilesError::InUse {
                id: id.to_owned(),
                by,
            });
        }
        let index = self
            .groups
            .iter()
            .position(|g| g.id() == id)
            .ok_or_else(|| ProfilesError::NotFound(id.to_owned()))?;
        Ok(self.groups.remove(index))
    }

    /// Appends a rules profile; its id must be new (among proxies, groups and rules profiles) and
    /// every target must be a saved proxy or group.
    pub fn add_rules(&mut self, profile: RulesProfile) -> Result<(), ProfilesError> {
        if self.id_taken(profile.id()) {
            return Err(ProfilesError::DuplicateId(profile.id().to_owned()));
        }
        self.check_targets(&profile)?;
        self.rules.push(profile);
        Ok(())
    }

    /// Replaces the rules profile with the same id, keeping its position.
    pub fn replace_rules(&mut self, profile: RulesProfile) -> Result<(), ProfilesError> {
        let index = self
            .rules
            .iter()
            .position(|r| r.id() == profile.id())
            .ok_or_else(|| ProfilesError::NotFound(profile.id().to_owned()))?;
        self.check_targets(&profile)?;
        self.rules[index] = profile;
        Ok(())
    }

    /// Removes and returns the rules profile with this id.
    pub fn remove_rules(&mut self, id: &str) -> Result<RulesProfile, ProfilesError> {
        let index = self
            .rules
            .iter()
            .position(|r| r.id() == id)
            .ok_or_else(|| ProfilesError::NotFound(id.to_owned()))?;
        Ok(self.rules.remove(index))
    }

    /// Appends a DNS profile; its id must be new.
    pub fn add_dns(&mut self, profile: DnsProfile) -> Result<(), ProfilesError> {
        if self.dns(profile.id()).is_some() {
            return Err(ProfilesError::DuplicateId(profile.id().to_owned()));
        }
        self.dns.push(profile);
        Ok(())
    }

    /// Replaces the proxy profile with the same id, keeping its position.
    pub fn replace_proxy(&mut self, profile: ProxyProfile) -> Result<(), ProfilesError> {
        let slot = self
            .proxies
            .iter_mut()
            .find(|p| p.id() == profile.id())
            .ok_or_else(|| ProfilesError::NotFound(profile.id().to_owned()))?;
        *slot = profile;
        Ok(())
    }

    /// Replaces the DNS profile with the same id, keeping its position.
    pub fn replace_dns(&mut self, profile: DnsProfile) -> Result<(), ProfilesError> {
        let slot = self
            .dns
            .iter_mut()
            .find(|p| p.id() == profile.id())
            .ok_or_else(|| ProfilesError::NotFound(profile.id().to_owned()))?;
        *slot = profile;
        Ok(())
    }

    /// Removes and returns the proxy profile with this id. Refused while a group or a rules
    /// profile uses it.
    pub fn remove_proxy(&mut self, id: &str) -> Result<ProxyProfile, ProfilesError> {
        let by: Vec<String> = self
            .groups_using(id)
            .into_iter()
            .chain(self.rules_using(id))
            .map(str::to_owned)
            .collect();
        if !by.is_empty() {
            return Err(ProfilesError::InUse {
                id: id.to_owned(),
                by,
            });
        }
        let index = self
            .proxies
            .iter()
            .position(|p| p.id() == id)
            .ok_or_else(|| ProfilesError::NotFound(id.to_owned()))?;
        Ok(self.proxies.remove(index))
    }

    /// Removes and returns the DNS profile with this id.
    pub fn remove_dns(&mut self, id: &str) -> Result<DnsProfile, ProfilesError> {
        let index = self
            .dns
            .iter()
            .position(|p| p.id() == id)
            .ok_or_else(|| ProfilesError::NotFound(id.to_owned()))?;
        Ok(self.dns.remove(index))
    }
}

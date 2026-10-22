//! The user's saved proxy and DNS profiles, as one validated collection.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DnsProfile, ProxyProfile};

/// Why a collection operation failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProfilesError {
    /// A profile with this id already exists in that list.
    #[error("a profile with id `{0}` already exists")]
    DuplicateId(String),
    /// No profile has this id.
    #[error("no profile with id `{0}`")]
    NotFound(String),
}

/// All saved profiles. Ids are unique within each kind (proxy / DNS) and
/// insertion order is preserved. Can only be built valid, including via serde.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProfilesDto")]
pub struct Profiles {
    proxies: Vec<ProxyProfile>,
    dns: Vec<DnsProfile>,
}

/// Untrusted wire representation; converted through the uniqueness check.
#[derive(Deserialize)]
struct ProfilesDto {
    #[serde(default)]
    proxies: Vec<ProxyProfile>,
    #[serde(default)]
    dns: Vec<DnsProfile>,
}

impl TryFrom<ProfilesDto> for Profiles {
    type Error = ProfilesError;

    fn try_from(dto: ProfilesDto) -> Result<Self, Self::Error> {
        ensure_unique(dto.proxies.iter().map(ProxyProfile::id))?;
        ensure_unique(dto.dns.iter().map(DnsProfile::id))?;
        Ok(Self {
            proxies: dto.proxies,
            dns: dto.dns,
        })
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
        if self.proxy(profile.id()).is_some() {
            return Err(ProfilesError::DuplicateId(profile.id().to_owned()));
        }
        self.proxies.push(profile);
        Ok(())
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

    /// Removes and returns the proxy profile with this id.
    pub fn remove_proxy(&mut self, id: &str) -> Result<ProxyProfile, ProfilesError> {
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

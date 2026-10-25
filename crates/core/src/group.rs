//! A proxy group: "use the best of these proxies" (ADR-0010).
//!
//! A group is its own aggregate next to [`crate::ProxyProfile`]. Its members are
//! ids of *plain* proxies, never of other groups, so cycles cannot be represented;
//! [`crate::Profiles`] checks that the members exist.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::profile::validate_identity;
use crate::ValidationError;

/// How a group chooses among its members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupPolicy {
    /// The live member with the lowest latency (ties go to the earlier member).
    #[default]
    Fastest,
}

/// A named list of proxies of which the best one is used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProxyGroupDto")]
pub struct ProxyGroup {
    id: String,
    name: String,
    members: Vec<String>,
    policy: GroupPolicy,
}

/// Untrusted wire representation; converted through [`ProxyGroup::new`].
#[derive(Deserialize)]
struct ProxyGroupDto {
    id: String,
    name: String,
    members: Vec<String>,
    #[serde(default)]
    policy: GroupPolicy,
}

impl TryFrom<ProxyGroupDto> for ProxyGroup {
    type Error = ValidationError;

    fn try_from(dto: ProxyGroupDto) -> Result<Self, Self::Error> {
        let mut group = Self::new(dto.id, dto.name, dto.members)?;
        group.policy = dto.policy;
        Ok(group)
    }
}

impl ProxyGroup {
    /// Builds a validated group with the default policy. Members are proxy ids,
    /// in priority order; there must be at least one and none may repeat.
    pub fn new(
        id: impl Into<String>,
        name: impl AsRef<str>,
        members: impl IntoIterator<Item = String>,
    ) -> Result<Self, ValidationError> {
        let (id, name) = validate_identity(id, name.as_ref())?;
        let members: Vec<String> = members.into_iter().collect();
        if members.is_empty() {
            return Err(ValidationError::NoGroupMembers);
        }
        let mut seen = HashSet::new();
        for member in &members {
            if !seen.insert(member.as_str()) {
                return Err(ValidationError::DuplicateGroupMember(member.clone()));
            }
        }
        Ok(Self {
            id,
            name,
            members,
            policy: GroupPolicy::default(),
        })
    }

    /// Stable identifier (shares one namespace with proxy ids).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name (trimmed).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Ids of the member proxies, in priority order.
    pub fn members(&self) -> &[String] {
        &self.members
    }

    /// How the group chooses among its members.
    pub fn policy(&self) -> GroupPolicy {
        self.policy
    }
}

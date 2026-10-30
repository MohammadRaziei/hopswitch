//! Per-domain routing: a [`RulesProfile`] is an ordered list of [`Rule`]s plus a default
//! [`Target`] (ADR-0010 phase 2, ADR-0011).
//!
//! Only the model lives here. Which targets exist is checked by [`crate::core::Profiles`], and turning a
//! profile into something the system understands (a PAC script) is a separate step.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::core::profile::validate_identity;
use crate::core::rule_list::{is_same_or_under, normalise};
use crate::core::ValidationError;

/// Where traffic for a matching domain goes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// Straight out, without any proxy. Built in: nothing to save.
    Direct,
    /// A saved proxy or proxy group, by id. [`crate::core::Profiles`] checks that it exists.
    Proxy(String),
}

/// One routing rule: a domain suffix and where it goes. The domain matches itself and every
/// sub-domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RuleDto")]
pub struct Rule {
    domain: String,
    target: Target,
}

/// Untrusted wire representation; converted through [`Rule::new`].
#[derive(Deserialize)]
struct RuleDto {
    domain: String,
    target: Target,
}

impl TryFrom<RuleDto> for Rule {
    type Error = ValidationError;

    fn try_from(dto: RuleDto) -> Result<Self, Self::Error> {
        Self::new(&dto.domain, dto.target)
    }
}

impl Rule {
    /// Builds a rule. The domain is trimmed, lower-cased and may be written with a leading dot
    /// (`.example.com`) or a trailing one.
    ///
    /// # Errors
    /// [`ValidationError::InvalidRuleDomain`] when the text is not a valid domain name.
    pub fn new(domain: &str, target: Target) -> Result<Self, ValidationError> {
        let trimmed = domain.trim();
        let bare = trimmed.strip_prefix('.').unwrap_or(trimmed);
        let domain =
            normalise(bare).ok_or_else(|| ValidationError::InvalidRuleDomain(domain.to_owned()))?;
        Ok(Self { domain, target })
    }

    /// The normalised domain suffix.
    #[must_use]
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Where matching traffic goes.
    #[must_use]
    pub fn target(&self) -> &Target {
        &self.target
    }
}

/// A named set of rules: the first rule whose domain matches decides, otherwise the default target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RulesProfileDto")]
pub struct RulesProfile {
    id: String,
    name: String,
    rules: Vec<Rule>,
    #[serde(rename = "default")]
    default_target: Target,
}

/// Untrusted wire representation; converted through [`RulesProfile::new`].
#[derive(Deserialize)]
struct RulesProfileDto {
    id: String,
    name: String,
    #[serde(default)]
    rules: Vec<Rule>,
    default: Target,
}

impl TryFrom<RulesProfileDto> for RulesProfile {
    type Error = ValidationError;

    fn try_from(dto: RulesProfileDto) -> Result<Self, Self::Error> {
        Self::new(dto.id, &dto.name, dto.rules, dto.default)
    }
}

impl RulesProfile {
    /// Builds a validated profile. `rules` are in priority order.
    ///
    /// # Errors
    /// An empty id or name, a name that is too long, or a domain that appears in two rules (the
    /// second could never match).
    pub fn new(
        id: impl Into<String>,
        name: &str,
        rules: Vec<Rule>,
        default_target: Target,
    ) -> Result<Self, ValidationError> {
        let (id, name) = validate_identity(id, name)?;
        let mut seen = HashSet::new();
        for rule in &rules {
            if !seen.insert(rule.domain()) {
                return Err(ValidationError::DuplicateRuleDomain(rule.domain.clone()));
            }
        }
        Ok(Self {
            id,
            name,
            rules,
            default_target,
        })
    }

    /// Stable identifier (shares one namespace with proxy and group ids).
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name (trimmed).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The rules, in priority order.
    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Where traffic goes when no rule matches.
    #[must_use]
    pub fn default_target(&self) -> &Target {
        &self.default_target
    }

    /// Where traffic to `host` goes: the target of the first rule that matches it, else the
    /// default. Case and a trailing dot are ignored.
    #[must_use]
    pub fn target_for(&self, host: &str) -> &Target {
        let lowered = host.to_ascii_lowercase();
        let host = lowered.strip_suffix('.').unwrap_or(&lowered);
        if host.is_empty() {
            return &self.default_target;
        }
        self.rules
            .iter()
            .find(|rule| is_same_or_under(host, &rule.domain))
            .map_or(&self.default_target, Rule::target)
    }
}

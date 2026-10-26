//! Public domain lists as rule data (ADR-0011).
//!
//! A [`RuleList`] is a normalised set of domain suffixes: an entry matches itself and every
//! sub-domain. Importing never fails as a whole; lines that are not valid domain names are
//! dropped and counted in the [`ImportReport`], so one bad line cannot lose a list of 100 000.

use std::collections::HashSet;

use thiserror::Error;

const MAX_NAME_LEN: usize = 253;
const MAX_LABEL_LEN: usize = 63;

/// How a list file writes its entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFormat {
    /// One bare domain per line (`example.com`). A leading dot is invalid.
    Plain,
    /// One dotted suffix per line (`.example.com`). The leading dot is optional.
    Dotted,
}

/// What an import kept and what it dropped. `kept + duplicates + covered` is the number of valid
/// lines; blank lines and `#` comments are not counted at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Entries in the resulting list.
    pub kept: usize,
    /// Lines that are not valid domain names.
    pub invalid: usize,
    /// Valid lines that repeat an earlier entry (after normalising).
    pub duplicates: usize,
    /// Valid entries dropped because a parent domain is already in the list.
    pub covered: usize,
}

/// Why a rule could not be added to a [`RuleList`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleListError {
    /// The text is not a valid domain name.
    #[error("invalid domain: {0}")]
    InvalidDomain(String),
}

/// A normalised, sorted set of domain suffixes in which no entry is covered by another.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleList {
    domains: Vec<String>,
}

impl RuleList {
    /// Reads a list file. Lower-cases, strips a trailing dot, drops invalid lines, duplicates and
    /// entries covered by a parent, and sorts the rest.
    #[must_use]
    pub fn import(text: &str, format: ListFormat) -> (Self, ImportReport) {
        let mut report = ImportReport::default();
        let mut seen: HashSet<String> = HashSet::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let candidate = match format {
                ListFormat::Plain => line,
                ListFormat::Dotted => line.strip_prefix('.').unwrap_or(line),
            };
            match normalise(candidate) {
                Some(name) => {
                    if !seen.insert(name) {
                        report.duplicates += 1;
                    }
                }
                None => report.invalid += 1,
            }
        }
        let mut domains: Vec<String> = seen
            .iter()
            .filter(|name| !has_parent_in(name, &seen))
            .cloned()
            .collect();
        domains.sort_unstable();
        report.covered = seen.len() - domains.len();
        report.kept = domains.len();
        (Self { domains }, report)
    }

    /// The entries, sorted, without leading dots.
    #[must_use]
    pub fn domains(&self) -> &[String] {
        &self.domains
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.domains.len()
    }

    /// Whether the list has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }

    /// Whether `host` is an entry or a sub-domain of one. Case and a trailing dot are ignored.
    #[must_use]
    pub fn matches(&self, host: &str) -> bool {
        let lowered = host.to_ascii_lowercase();
        let host = lowered.strip_suffix('.').unwrap_or(&lowered);
        let found = suffixes(host).any(|suffix| {
            self.domains
                .binary_search_by(|entry| entry.as_str().cmp(suffix))
                .is_ok()
        });
        found
    }

    /// Adds one suffix rule and drops the entries it now covers (for example `ir` replaces tens of
    /// thousands of `*.ir` names). Returns how many entries were removed; adding a name that a
    /// parent already covers changes nothing.
    ///
    /// # Errors
    /// [`RuleListError::InvalidDomain`] when `domain` is not a valid name; the list is unchanged.
    pub fn add_suffix(&mut self, domain: &str) -> Result<usize, RuleListError> {
        let name =
            normalise(domain).ok_or_else(|| RuleListError::InvalidDomain(domain.to_owned()))?;
        if self.matches(&name) {
            return Ok(0);
        }
        let before = self.domains.len();
        self.domains.retain(|entry| !is_same_or_under(entry, &name));
        let removed = before - self.domains.len();
        let at = self
            .domains
            .binary_search(&name)
            .unwrap_or_else(|insert_at| insert_at);
        self.domains.insert(at, name);
        Ok(removed)
    }
}

/// `name` itself followed by each parent, longest first: `a.b.c`, `b.c`, `c`.
fn suffixes(name: &str) -> impl Iterator<Item = &str> {
    std::iter::once(name).chain(name.match_indices('.').map(|(at, _)| &name[at + 1..]))
}

fn has_parent_in(name: &str, set: &HashSet<String>) -> bool {
    suffixes(name).skip(1).any(|parent| set.contains(parent))
}

fn is_same_or_under(entry: &str, parent: &str) -> bool {
    entry == parent
        || entry
            .strip_suffix(parent)
            .is_some_and(|rest| rest.ends_with('.'))
}

/// Lower-cases, removes one trailing dot, and checks the name. `None` when invalid.
fn normalise(raw: &str) -> Option<String> {
    let name = raw.to_ascii_lowercase();
    let name = name.strip_suffix('.').unwrap_or(&name);
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return None;
    }
    name.split('.').all(valid_label).then(|| name.to_owned())
}

fn valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= MAX_LABEL_LEN
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

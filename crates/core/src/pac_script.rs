//! Generates the PAC script that carries per-domain rules to the system (ADR-0011 decision 5).
//!
//! The script is old JavaScript (ES3: no `let`, arrow functions or trailing commas) because Windows'
//! PAC engine is old. User rules are checked first, one by one, in order. Each domain list is a
//! table grouped by top-level domain and then by the first two letters of the rest, with each group
//! stored as one string (`,rest,rest,`) that is searched with `indexOf`: that is what keeps a
//! 127 000-domain list under 650 KB and loadable by `WinHTTP`, where one object key per domain is
//! rejected (measured, ADR-0011). Plain host names (no dot) always go direct.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use thiserror::Error;

use crate::{ProxyConfig, ProxyScheme, Rule, RuleList, RulesProfile, Target, ValidationError};

/// The most bytes a generated script may have. Beyond this some PAC clients (Chromium's fetcher is
/// reported to cap near 1 MB; not verified) refuse the script, so it is not produced at all.
pub const PAC_MAX_BYTES: usize = 1_000_000;

/// The size a script should stay under; bigger ones still work but leave less room.
pub const PAC_TARGET_BYTES: usize = 750_000;

/// What a PAC script answers for a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacRoute {
    /// `DIRECT`.
    Direct,
    /// Through this proxy.
    Proxy(ProxyConfig),
}

impl PacRoute {
    /// The text a PAC engine understands: `DIRECT`, `PROXY host:port`, `SOCKS5 host:port`, ...
    #[must_use]
    pub fn answer(&self) -> String {
        match self {
            Self::Direct => "DIRECT".to_owned(),
            Self::Proxy(config) => {
                let keyword = match config.scheme() {
                    ProxyScheme::Http => "PROXY",
                    ProxyScheme::Https => "HTTPS",
                    ProxyScheme::Socks4 => "SOCKS4",
                    ProxyScheme::Socks5 => "SOCKS5",
                };
                let host = config.host();
                if host.contains(':') {
                    format!("{keyword} [{host}]:{}", config.port())
                } else {
                    format!("{keyword} {host}:{}", config.port())
                }
            }
        }
    }
}

/// Why a script could not be produced.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PacError {
    /// The script would be bigger than [`PAC_MAX_BYTES`].
    #[error("the PAC script would be {bytes} bytes, over the {max} byte limit")]
    TooLarge {
        /// Size the script would have.
        bytes: usize,
        /// The limit.
        max: usize,
    },
    /// A rules profile points at a target that could not be turned into a route.
    #[error("no route for target `{0}`")]
    UnresolvedTarget(String),
}

/// What a PAC script decides, already resolved to routes: the user's rules (in order), then domain
/// lists (in order), then a default. The first layer that matches a host answers.
#[derive(Debug, Clone)]
pub struct PacPlan {
    default: PacRoute,
    rules: Vec<(String, PacRoute)>,
    lists: Vec<(RuleList, PacRoute)>,
}

impl PacPlan {
    /// A plan that answers `default` for everything that is not a plain host name.
    #[must_use]
    pub fn new(default: PacRoute) -> Self {
        Self {
            default,
            rules: Vec::new(),
            lists: Vec::new(),
        }
    }

    /// Adds a user rule after the ones already added. A domain matches itself and its sub-domains.
    ///
    /// # Errors
    /// [`ValidationError::InvalidRuleDomain`] when `domain` is not a valid domain name.
    pub fn with_rule(mut self, domain: &str, route: PacRoute) -> Result<Self, ValidationError> {
        let rule = Rule::new(domain, Target::Direct)?;
        self.rules.push((rule.domain().to_owned(), route));
        Ok(self)
    }

    /// Adds a domain list below every user rule and the lists added before it.
    #[must_use]
    pub fn with_list(mut self, list: RuleList, route: PacRoute) -> Self {
        self.lists.push((list, route));
        self
    }

    /// The user rules and the default of `profile`, with each target turned into a route by
    /// `resolve` (a group, for one, becomes whichever member is best at that moment).
    ///
    /// # Errors
    /// [`PacError::UnresolvedTarget`] when `resolve` has no route for a target.
    pub fn from_profile(
        profile: &RulesProfile,
        resolve: impl Fn(&Target) -> Option<PacRoute>,
    ) -> Result<Self, PacError> {
        let route = |target: &Target| {
            resolve(target).ok_or_else(|| match target {
                Target::Direct => PacError::UnresolvedTarget("direct".to_owned()),
                Target::Proxy(id) => PacError::UnresolvedTarget(id.clone()),
            })
        };
        let mut plan = Self::new(route(profile.default_target())?);
        for rule in profile.rules() {
            plan.rules
                .push((rule.domain().to_owned(), route(rule.target())?));
        }
        Ok(plan)
    }

    /// The script.
    ///
    /// # Errors
    /// [`PacError::TooLarge`] when it would be over [`PAC_MAX_BYTES`].
    pub fn generate(&self) -> Result<String, PacError> {
        let mut answers: Vec<String> = Vec::new();
        let mut index_of = |route: &PacRoute| {
            let answer = route.answer();
            answers
                .iter()
                .position(|a| *a == answer)
                .unwrap_or_else(|| {
                    answers.push(answer);
                    answers.len() - 1
                })
        };
        let default = index_of(&self.default);
        let rule_answers: Vec<usize> = self.rules.iter().map(|(_, r)| index_of(r)).collect();
        let list_answers: Vec<usize> = self.lists.iter().map(|(_, r)| index_of(r)).collect();

        let mut out = String::new();
        let quoted: Vec<String> = answers.iter().map(|a| js_string(a)).collect();
        let _ = writeln!(out, "var A = [{}];", quoted.join(","));
        if !self.rules.is_empty() {
            let suffixes: Vec<String> = self.rules.iter().map(|(d, _)| js_string(d)).collect();
            let indexes: Vec<String> = rule_answers.iter().map(ToString::to_string).collect();
            let _ = writeln!(out, "var RS = [{}];", suffixes.join(","));
            let _ = writeln!(out, "var RA = [{}];", indexes.join(","));
        }
        for (n, (list, _)) in self.lists.iter().enumerate() {
            let _ = writeln!(out, "var T{n} = {};", table(list));
        }
        if !self.lists.is_empty() {
            out.push_str(IN_LIST);
        }
        out.push_str("function FindProxyForURL(url, host) {\n");
        out.push_str("  if (isPlainHostName(host)) return \"DIRECT\";\n");
        out.push_str("  host = host.toLowerCase();\n");
        out.push_str(
            "  if (host.charAt(host.length - 1) == \".\") host = host.substring(0, host.length - 1);\n",
        );
        if !self.rules.is_empty() {
            out.push_str(RULE_LOOP);
        }
        if !self.lists.is_empty() {
            out.push_str("  var parts = host.split(\".\");\n");
            for (n, answer) in list_answers.iter().enumerate() {
                let _ = writeln!(out, "  if (inList(T{n}, parts)) return A[{answer}];");
            }
        }
        let _ = writeln!(out, "  return A[{default}];\n}}");

        if out.len() > PAC_MAX_BYTES {
            return Err(PacError::TooLarge {
                bytes: out.len(),
                max: PAC_MAX_BYTES,
            });
        }
        Ok(out)
    }
}

/// Rule `i` matches when `host` is the suffix or ends with `.suffix`.
const RULE_LOOP: &str = "  for (var i = 0; i < RS.length; i++) {
    var s = RS[i];
    var at = host.length - s.length;
    if (at >= 0 && host.substring(at) == s && (at == 0 || host.charAt(at - 1) == \".\")) return A[RA[i]];
  }
";

/// A table maps a top-level domain to `1` (everything under it) or to groups of `,rest,rest,`.
const IN_LIST: &str = "var has = Object.prototype.hasOwnProperty;
function inList(t, parts) {
  var tld = parts[parts.length - 1];
  if (!has.call(t, tld)) return false;
  var groups = t[tld];
  if (groups === 1) return true;
  for (var i = 0; i < parts.length - 1; i++) {
    var rest = parts.slice(i, parts.length - 1).join(\".\");
    var group = groups[rest.substr(0, 2)];
    if (group && group.indexOf(\",\" + rest + \",\") >= 0) return true;
  }
  return false;
}
";

enum Tld<'a> {
    Whole,
    Groups(BTreeMap<&'a str, Vec<&'a str>>),
}

/// `{"com":{"ab":",abc,ab.x,"},"ir":1}`, in sorted order so the script is reproducible.
fn table(list: &RuleList) -> String {
    let mut tlds: BTreeMap<&str, Tld<'_>> = BTreeMap::new();
    for domain in list.domains() {
        match domain.rsplit_once('.') {
            None => {
                tlds.insert(domain, Tld::Whole);
            }
            Some((rest, tld)) => {
                let entry = tlds
                    .entry(tld)
                    .or_insert_with(|| Tld::Groups(BTreeMap::new()));
                if let Tld::Groups(groups) = entry {
                    groups
                        .entry(&rest[..rest.len().min(2)])
                        .or_default()
                        .push(rest);
                }
            }
        }
    }
    let body: Vec<String> = tlds
        .iter()
        .map(|(tld, kind)| match kind {
            Tld::Whole => format!("{}:1", js_string(tld)),
            Tld::Groups(groups) => {
                let items: Vec<String> = groups
                    .iter()
                    .map(|(key, rests)| {
                        format!(
                            "{}:{}",
                            js_string(key),
                            js_string(&format!(",{},", rests.join(",")))
                        )
                    })
                    .collect();
                format!("{}:{{{}}}", js_string(tld), items.join(","))
            }
        })
        .collect();
    format!("{{{}}}", body.join(","))
}

/// A JavaScript string literal that stays ASCII whatever it holds.
fn js_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0_u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
    out
}

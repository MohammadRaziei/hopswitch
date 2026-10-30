//! `hopswitch rules ...`: per-domain routing (ADR-0010 phase 2, ADR-0011).
//!
//! Managing profiles only edits `profiles.json`. `rules use` is different: the system is pointed at
//! a PAC script that **this process** serves on the loopback interface, so the process has to stay
//! alive. [`start_rules`] does the work and returns a [`RulesRun`]; `rules use` on the command line
//! keeps one open until Enter is pressed. If the process is killed instead, the saved session is
//! still there and `hopswitch off` (or `proxy off`) restores the original settings.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::core::{
    plan_rules_pac, AutoConfigStore, PacConfig, PacEndpoint, ProfileRepository, Profiles,
    ProxyService, ProxyState, Rule, RulesProfile, Target,
};
use crate::storage::{JsonFileAutoStore, JsonFileRepository};

use crate::cli::app::{find_saved, open_service, say, slug, unique_id};
use crate::cli::command::RulesCommand;
use crate::cli::{CliError, Context};

fn load_profiles(ctx: &Context) -> Result<Profiles, CliError> {
    let path = ctx.dir.join("profiles.json");
    JsonFileRepository::new(&path)
        .load()
        .map_err(|e| CliError::failure(format!("{e} (file: {})", path.display())))
}

fn save_profiles(ctx: &Context, profiles: &Profiles) -> Result<(), CliError> {
    JsonFileRepository::new(ctx.dir.join("profiles.json"))
        .save(profiles)
        .map_err(CliError::failure)
}

fn auto_store(ctx: &Context) -> (JsonFileAutoStore, PathBuf) {
    let path = ctx.dir.join("auto.json");
    (JsonFileAutoStore::new(&path), path)
}

/// The id of the rules profile `key` names (an id, else a name ignoring case).
fn find_profile(profiles: &Profiles, key: &str) -> Result<String, CliError> {
    find_saved(
        profiles.rules_profiles().iter().map(|r| (r.id(), r.name())),
        key,
    )
    .ok_or_else(|| {
        CliError::failure(format!(
            "no rules profile `{key}`; see `hopswitch rules list`"
        ))
    })
}

/// What `direct` or a saved proxy or group stands for.
fn parse_target(profiles: &Profiles, text: &str) -> Result<Target, CliError> {
    if text.eq_ignore_ascii_case("direct") {
        return Ok(Target::Direct);
    }
    let everything = profiles
        .proxy_profiles()
        .iter()
        .map(|p| (p.id(), p.name()))
        .chain(profiles.groups().iter().map(|g| (g.id(), g.name())));
    find_saved(everything, text).map(Target::Proxy).ok_or_else(|| {
        CliError::failure(format!(
            "no saved proxy or group `{text}`; use `direct` for no proxy, or see `hopswitch proxy list`"
        ))
    })
}

fn label(target: &Target) -> &str {
    match target {
        Target::Direct => "direct",
        Target::Proxy(id) => id,
    }
}

/// Runs every `rules` subcommand except `use`.
pub(crate) fn execute(
    ctx: &Context,
    command: RulesCommand,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    match command {
        RulesCommand::List => list(ctx, out),
        RulesCommand::Show(key) => show(ctx, &key, out),
        RulesCommand::Add { name, default } => add(ctx, &name, default.as_deref(), out),
        RulesCommand::Remove(key) => remove(ctx, &key, out),
        RulesCommand::SetDefault { profile, target } => {
            edit(ctx, &profile, out, |profiles, current| {
                let target = parse_target(profiles, &target)?;
                let line = format!("Unmatched traffic now goes {}.", label(&target));
                Ok((current.rules().to_vec(), target, line))
            })
        }
        RulesCommand::AddRule {
            profile,
            domain,
            target,
        } => edit(ctx, &profile, out, |profiles, current| {
            let target = parse_target(profiles, &target)?;
            let rule = Rule::new(&domain, target.clone())
                .map_err(|_| CliError::failure(format!("`{domain}` is not a valid domain")))?;
            let line = format!("Added rule: {} -> {}.", rule.domain(), label(&target));
            let mut rules = current.rules().to_vec();
            rules.push(rule);
            Ok((rules, current.default_target().clone(), line))
        }),
        RulesCommand::RemoveRule { profile, domain } => edit(ctx, &profile, out, |_, current| {
            let wanted = Rule::new(&domain, Target::Direct)
                .map_err(|_| CliError::failure(format!("`{domain}` is not a valid domain")))?;
            let mut rules = current.rules().to_vec();
            let before = rules.len();
            rules.retain(|rule| rule.domain() != wanted.domain());
            if rules.len() == before {
                return Err(CliError::failure(format!(
                    "`{}` has no rule for {}",
                    current.id(),
                    wanted.domain()
                )));
            }
            Ok((
                rules,
                current.default_target().clone(),
                format!("Removed the rule for {}.", wanted.domain()),
            ))
        }),
        RulesCommand::Use(_) => Err(CliError::failure("`rules use` runs until it is stopped")),
    }
}

fn applied_id(ctx: &Context) -> Result<Option<String>, CliError> {
    let service: ProxyService = open_service(ctx, "session-proxy.json")?;
    Ok(service.active_id().map(str::to_owned))
}

fn list(ctx: &Context, out: &mut dyn Write) -> Result<(), CliError> {
    let profiles = load_profiles(ctx)?;
    if profiles.rules_profiles().is_empty() {
        return say(
            out,
            "No rules profiles. Create one with `hopswitch rules add <name>`.",
        );
    }
    let applied = applied_id(ctx)?;
    for profile in profiles.rules_profiles() {
        let marker = if applied.as_deref() == Some(profile.id()) {
            '*'
        } else {
            ' '
        };
        say(
            out,
            format!(
                "{marker} {}  {}  {} {}, default {}",
                profile.id(),
                profile.name(),
                profile.rules().len(),
                if profile.rules().len() == 1 {
                    "rule"
                } else {
                    "rules"
                },
                label(profile.default_target())
            ),
        )?;
    }
    Ok(())
}

fn show(ctx: &Context, key: &str, out: &mut dyn Write) -> Result<(), CliError> {
    let profiles = load_profiles(ctx)?;
    let id = find_profile(&profiles, key)?;
    let profile = profiles
        .rules_profile(&id)
        .ok_or_else(|| CliError::failure("the profile disappeared"))?;
    say(out, format!("{} ({})", profile.name(), profile.id()))?;
    for (n, rule) in profile.rules().iter().enumerate() {
        say(
            out,
            format!("  {}. {} -> {}", n + 1, rule.domain(), label(rule.target())),
        )?;
    }
    say(
        out,
        format!("  default -> {}", label(profile.default_target())),
    )
}

fn add(
    ctx: &Context,
    name: &str,
    default: Option<&str>,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let mut profiles = load_profiles(ctx)?;
    let default = match default {
        Some(text) => parse_target(&profiles, text)?,
        None => Target::Direct,
    };
    let id = unique_id(&slug(name), |id| {
        profiles.proxy_entry(id).is_some() || profiles.rules_profile(id).is_some()
    });
    let profile =
        RulesProfile::new(&id, name, Vec::new(), default.clone()).map_err(CliError::failure)?;
    profiles.add_rules(profile).map_err(CliError::failure)?;
    save_profiles(ctx, &profiles)?;
    say(
        out,
        format!("Saved rules profile `{id}` (default: {}).", label(&default)),
    )
}

fn remove(ctx: &Context, key: &str, out: &mut dyn Write) -> Result<(), CliError> {
    let mut profiles = load_profiles(ctx)?;
    let id = find_profile(&profiles, key)?;
    if applied_id(ctx)?.as_deref() == Some(id.as_str()) {
        return Err(CliError::failure(format!(
            "`{id}` is applied; stop `hopswitch rules use` (or run `hopswitch proxy off`) first"
        )));
    }
    profiles.remove_rules(&id).map_err(CliError::failure)?;
    save_profiles(ctx, &profiles)?;
    say(out, format!("Removed rules profile `{id}`."))
}

/// What an edit produces: the new rules, the new default and the line to print.
type Edited = (Vec<Rule>, Target, String);

/// Rebuilds one profile through its validating constructor and saves it in place.
fn edit(
    ctx: &Context,
    key: &str,
    out: &mut dyn Write,
    change: impl FnOnce(&Profiles, &RulesProfile) -> Result<Edited, CliError>,
) -> Result<(), CliError> {
    let mut profiles = load_profiles(ctx)?;
    let id = find_profile(&profiles, key)?;
    let current = profiles
        .rules_profile(&id)
        .ok_or_else(|| CliError::failure("the profile disappeared"))?
        .clone();
    let (rules, default, line) = change(&profiles, &current)?;
    let updated = RulesProfile::new(current.id(), current.name(), rules, default)
        .map_err(CliError::failure)?;
    profiles.replace_rules(updated).map_err(CliError::failure)?;
    save_profiles(ctx, &profiles)?;
    say(out, line)
}

/// A rules profile that is applied: the system points at a PAC script this value serves.
pub struct RulesRun {
    ctx: Context,
    id: String,
    name: String,
    service: ProxyService,
    endpoint: Box<dyn PacEndpoint>,
    script: String,
    notes: Vec<String>,
}

impl std::fmt::Debug for RulesRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RulesRun")
            .field("profile", &self.id)
            .finish_non_exhaustive()
    }
}

/// Builds the script for the profile `id` from what is saved and probed right now.
fn build_script(ctx: &Context, id: &str) -> Result<String, CliError> {
    let profiles = load_profiles(ctx)?;
    let profile = profiles
        .rules_profile(id)
        .ok_or_else(|| CliError::failure(format!("the rules profile `{id}` is gone")))?;
    let (store, path) = auto_store(ctx);
    let config = store
        .load()
        .map_err(|e| CliError::failure(format!("{e} (file: {})", path.display())))?;
    let plan = plan_rules_pac(profile, &profiles, &*ctx.prober, &config.connectivity)
        .map_err(CliError::failure)?;
    plan.generate().map_err(CliError::failure)
}

/// Applies the rules profile `key`: builds its script, starts serving it and points the system's
/// proxy at it. The first change to the system is saved before it is made, as for any profile.
///
/// # Errors
/// A [`CliError`] when the profile is unknown, would use SOCKS, has a group nobody answers for, or
/// the server or the system refuses. Nothing is changed in that case.
pub fn start_rules(ctx: &Context, key: &str) -> Result<RulesRun, CliError> {
    let profiles = load_profiles(ctx)?;
    let id = find_profile(&profiles, key)?;
    let name = profiles
        .rules_profile(&id)
        .map_or_else(|| id.clone(), |p| p.name().to_owned());
    let script = build_script(ctx, &id)?;
    let mut service = open_service(ctx, "session-proxy.json")?;
    let endpoint = ctx.pac.start(&script).map_err(CliError::failure)?;
    service
        .activate_state(&id, &ProxyState::Pac(endpoint.url()))
        .map_err(CliError::failure)?;

    let mut notes = Vec::new();
    let (store, _) = auto_store(ctx);
    if store.load().is_ok_and(|config| config.proxy.connect) {
        notes.push(
            "Auto proxy connect is on and would replace this script with a single proxy; turn it \
             off with `hopswitch auto proxy connect off`."
                .to_owned(),
        );
    }
    Ok(RulesRun {
        ctx: ctx.clone(),
        id,
        name,
        service,
        endpoint,
        script,
        notes,
    })
}

impl RulesRun {
    /// The profile's display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where the system finds the script.
    #[must_use]
    pub fn url(&self) -> PacConfig {
        self.endpoint.url()
    }

    /// Things the person should know about, such as automation that would undo this.
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Rebuilds the script from what is saved and probed now and serves it if it changed. Returns
    /// whether it changed. When no script can be built (a proxy went away, a SOCKS proxy was added)
    /// the old one keeps being served and the error says why.
    ///
    /// # Errors
    /// A [`CliError`] naming the reason.
    pub fn refresh(&mut self) -> Result<bool, CliError> {
        let script = build_script(&self.ctx, &self.id)?;
        if script == self.script {
            return Ok(false);
        }
        self.endpoint.set_script(&script);
        self.script = script;
        Ok(true)
    }

    /// Restores the original settings, then stops serving. If restoring fails the session stays
    /// saved (`hopswitch off` retries) and the script keeps being served until this value drops.
    ///
    /// # Errors
    /// A [`CliError`] when the original settings could not be restored.
    pub fn stop(mut self) -> Result<(), CliError> {
        self.service.deactivate().map_err(CliError::failure)
    }
}

/// `hopswitch rules use`: applies the profile and keeps serving it until Enter is pressed.
pub(crate) fn use_until_enter(
    ctx: &Context,
    key: &str,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let mut run = start_rules(ctx, key)?;
    say(
        out,
        format!(
            "Rules profile {} is applied; the script is served at {}.",
            run.name(),
            run.url()
        ),
    )?;
    for note in run.notes() {
        say(out, note)?;
    }
    say(
        out,
        "Keep this running. Press Enter to stop and restore the original settings; if it is \
         killed instead, run `hopswitch off`.",
    )?;

    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    // A closed standard input (a service, `nohup`) is not a request to stop.
    thread::spawn(move || {
        let mut line = String::new();
        if matches!(std::io::stdin().lock().read_line(&mut line), Ok(n) if n > 0) {
            flag.store(true, Ordering::SeqCst);
        }
    });

    let (store, _) = auto_store(ctx);
    let interval = store
        .load()
        .map_or(30, |config| config.connectivity.interval_secs())
        .max(1);
    let mut reported: Option<String> = None;
    'serving: loop {
        for _ in 0..interval * 10 {
            if stop.load(Ordering::SeqCst) {
                break 'serving;
            }
            thread::sleep(Duration::from_millis(100));
        }
        match run.refresh() {
            Ok(true) => {
                reported = None;
                say(out, "The rules changed; the new script is being served.")?;
            }
            Ok(false) => reported = None,
            Err(error) => {
                let message = error.to_string();
                if reported.as_deref() != Some(message.as_str()) {
                    say(out, format!("Keeping the old script: {message}"))?;
                    reported = Some(message);
                }
            }
        }
    }
    run.stop()?;
    say(out, "Stopped; the original settings are back.")
}

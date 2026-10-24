//! Executing a parsed [`Command`] against injected dependencies.
//!
//! Every call to [`run`] behaves like a fresh process: it reopens the session
//! files, resumes the services and only then acts. Profiles are loaded lazily, so
//! `off` keeps working even when `profiles.json` is damaged.

use std::fmt::Display;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hopswitch_core::{
    dns_preset, dns_presets, probe_dns_profiles, probe_proxies, proxy_preset, proxy_presets,
    restore_all, AutoConfig, AutoConfigStore, AutoKind, AutoRunner, ConnectivitySettings, Decision,
    DnsConfig, DnsProfile, DnsService, DnsState, Health, Kind, KindReport, NetworkService, Probe,
    Prober, ProfileRepository, Profiles, ProxyConfig, ProxyProfile, ProxyService, ProxyState,
    SessionStore, SystemNetwork, TickReport,
};
use hopswitch_storage::{JsonFileAutoStore, JsonFileRepository, JsonFileSessionStore};

use crate::command::{
    parse, AutoCommand, AutoOption, AutoTarget, Command, DnsCommand, PingOptions, PingScope,
    ProxyCommand,
};
use crate::CliError;

const USAGE: &str = "\
hopswitch - switch the system proxy and DNS

usage:
  hopswitch status                          show the current proxy and DNS
  hopswitch presets                         list the built-in presets
  hopswitch proxy use <name|preset>         apply a saved profile or preset
  hopswitch proxy off                       restore the original proxy
  hopswitch proxy list                      list saved proxy profiles
  hopswitch proxy add <name> <scheme://host:port> [--bypass a,b]
  hopswitch proxy remove <name>
  hopswitch dns use <name|preset>           apply a saved profile or preset
  hopswitch dns off                         restore the original DNS
  hopswitch dns list                        list saved DNS profiles
  hopswitch dns add <name> <ip[,ip...]>
  hopswitch dns remove <name>
  hopswitch ping [proxy|dns] [--url U] [--domain D] [--timeout MS]
                                            test every saved profile and show latency
  hopswitch auto status                     show the auto settings and pools
  hopswitch auto proxy|dns connect on|off   connect when something answers, disconnect when not
  hopswitch auto proxy|dns switch on|off    stay on the fastest profile of the pool
  hopswitch auto proxy|dns pool add|remove <name>
  hopswitch auto set [--url U] [--domain D] [--timeout MS] [--interval SECS]
  hopswitch auto run [--once]               apply the auto settings (Ctrl-C to stop)
  hopswitch off                             restore both proxy and DNS
  hopswitch help                            show this text";

/// Everything the CLI needs from the outside world.
pub struct Context {
    /// The settings folder (profiles and sessions live here).
    pub dir: PathBuf,
    /// The system whose proxy and DNS are switched.
    pub network: Arc<dyn SystemNetwork>,
    /// What measures whether a proxy or DNS server answers (`ping`).
    pub prober: Arc<dyn Prober>,
}

/// Runs one command line and prints the result to `out`.
pub fn run(args: &[String], ctx: &Context, out: &mut dyn Write) -> Result<(), CliError> {
    match parse(args)? {
        Command::Help => say(out, USAGE),
        Command::Auto(AutoCommand::Run { once }) => auto_run(ctx, once, out),
        command => App::open(ctx)?.execute(command, out),
    }
}

struct App {
    network: Arc<dyn SystemNetwork>,
    prober: Arc<dyn Prober>,
    repository: JsonFileRepository,
    profiles_path: PathBuf,
    auto_store: JsonFileAutoStore,
    auto_path: PathBuf,
    proxy: ProxyService,
    dns: DnsService,
}

impl App {
    fn open(ctx: &Context) -> Result<Self, CliError> {
        let profiles_path = ctx.dir.join("profiles.json");
        let auto_path = ctx.dir.join("auto.json");
        Ok(Self {
            auto_store: JsonFileAutoStore::new(&auto_path),
            auto_path,
            network: ctx.network.clone(),
            prober: ctx.prober.clone(),
            repository: JsonFileRepository::new(&profiles_path),
            profiles_path,
            proxy: open_service(ctx, "session-proxy.json")?,
            dns: open_service(ctx, "session-dns.json")?,
        })
    }

    fn profiles(&self) -> Result<Profiles, CliError> {
        self.repository
            .load()
            .map_err(|e| CliError::failure(format!("{e} (file: {})", self.profiles_path.display())))
    }

    fn auto_config(&self) -> Result<AutoConfig, CliError> {
        self.auto_store
            .load()
            .map_err(|e| CliError::failure(format!("{e} (file: {})", self.auto_path.display())))
    }

    fn save_auto(&self, config: &AutoConfig) -> Result<(), CliError> {
        self.auto_store.save(config).map_err(CliError::failure)
    }

    /// Drops a deleted profile from its pool. A damaged settings file is left alone:
    /// a stale id in a pool is ignored by the runner anyway.
    fn prune_pool(&self, target: AutoTarget, id: &str) {
        if let Ok(mut config) = self.auto_store.load() {
            let kind = match target {
                AutoTarget::Proxy => &mut config.proxy,
                AutoTarget::Dns => &mut config.dns,
            };
            if kind.remove_from_pool(id) {
                let _ = self.auto_store.save(&config);
            }
        }
    }

    fn save(&self, profiles: &Profiles) -> Result<(), CliError> {
        self.repository.save(profiles).map_err(CliError::failure)
    }

    fn execute(&mut self, command: Command, out: &mut dyn Write) -> Result<(), CliError> {
        match command {
            Command::Help => say(out, USAGE),
            Command::Status => self.status(out),
            Command::Presets => self.presets(out),
            Command::Off => self.off(out),
            Command::Ping(options) => self.ping(&options, out),
            Command::Auto(command) => self.auto_command(command, out),
            Command::Proxy(command) => self.proxy_command(command, out),
            Command::Dns(command) => self.dns_command(command, out),
        }
    }

    // ---------- proxy ----------

    fn proxy_command(
        &mut self,
        command: ProxyCommand,
        out: &mut dyn Write,
    ) -> Result<(), CliError> {
        match command {
            ProxyCommand::List => self.list_proxies(out),
            ProxyCommand::Use(key) => self.use_proxy(&key, out),
            ProxyCommand::Off => self.proxy_off(out),
            ProxyCommand::Add { name, spec, bypass } => self.add_proxy(&name, &spec, bypass, out),
            ProxyCommand::Remove(key) => self.remove_proxy(&key, out),
        }
    }

    fn list_proxies(&self, out: &mut dyn Write) -> Result<(), CliError> {
        let profiles = self.profiles()?;
        if profiles.proxy_profiles().is_empty() {
            return say(out, "No saved proxy profiles. See `hopswitch presets`.");
        }
        for profile in profiles.proxy_profiles() {
            let mark = marker(self.proxy.active_id() == Some(profile.id()));
            say(
                out,
                format!(
                    "{mark} {}  {}  {}",
                    profile.id(),
                    profile.name(),
                    profile.config()
                ),
            )?;
        }
        Ok(())
    }

    fn use_proxy(&mut self, key: &str, out: &mut dyn Write) -> Result<(), CliError> {
        let mut profiles = self.profiles()?;
        let (profile, is_new) = resolve_proxy(&profiles, key)?;
        self.proxy.activate(&profile).map_err(CliError::failure)?;
        if is_new {
            profiles
                .add_proxy(profile.clone())
                .map_err(CliError::failure)?;
            self.save(&profiles)?;
        }
        say(
            out,
            format!("Proxy is now {} ({}).", profile.name(), profile.config()),
        )
    }

    fn proxy_off(&mut self, out: &mut dyn Write) -> Result<(), CliError> {
        if self.proxy.active_id().is_none() {
            return say(out, "No proxy is active; nothing to restore.");
        }
        self.proxy.deactivate().map_err(CliError::failure)?;
        say(out, "Proxy restored to its original settings.")
    }

    fn add_proxy(
        &self,
        name: &str,
        spec: &str,
        bypass: Vec<String>,
        out: &mut dyn Write,
    ) -> Result<(), CliError> {
        let config = spec.parse::<ProxyConfig>().map_err(|e| {
            CliError::failure(format!(
                "invalid proxy URL `{spec}` (expected scheme://host:port): {e}"
            ))
        })?;
        let config = if bypass.is_empty() {
            config
        } else {
            config.with_bypass(bypass).map_err(CliError::failure)?
        };
        let mut profiles = self.profiles()?;
        let id = unique_id(&slug(name), |id| profiles.proxy(id).is_some());
        let profile = ProxyProfile::new(&id, name, config).map_err(CliError::failure)?;
        profiles.add_proxy(profile).map_err(CliError::failure)?;
        self.save(&profiles)?;
        say(out, format!("Saved proxy profile `{id}`."))
    }

    fn remove_proxy(&self, key: &str, out: &mut dyn Write) -> Result<(), CliError> {
        let mut profiles = self.profiles()?;
        let id = find_saved(
            profiles.proxy_profiles().iter().map(|p| (p.id(), p.name())),
            key,
        )
        .ok_or_else(|| CliError::failure(format!("no saved proxy profile `{key}`")))?;
        if self.proxy.active_id() == Some(id.as_str()) {
            return Err(CliError::failure(format!(
                "`{id}` is active; run `hopswitch proxy off` first"
            )));
        }
        profiles.remove_proxy(&id).map_err(CliError::failure)?;
        self.save(&profiles)?;
        self.prune_pool(AutoTarget::Proxy, &id);
        say(out, format!("Removed proxy profile `{id}`."))
    }

    // ---------- dns ----------

    fn dns_command(&mut self, command: DnsCommand, out: &mut dyn Write) -> Result<(), CliError> {
        match command {
            DnsCommand::List => self.list_dns(out),
            DnsCommand::Use(key) => self.use_dns(&key, out),
            DnsCommand::Off => self.dns_off(out),
            DnsCommand::Add { name, servers } => self.add_dns(&name, &servers, out),
            DnsCommand::Remove(key) => self.remove_dns(&key, out),
        }
    }

    fn list_dns(&self, out: &mut dyn Write) -> Result<(), CliError> {
        let profiles = self.profiles()?;
        if profiles.dns_profiles().is_empty() {
            return say(out, "No saved DNS profiles. See `hopswitch presets`.");
        }
        for profile in profiles.dns_profiles() {
            let mark = marker(self.dns.active_id() == Some(profile.id()));
            say(
                out,
                format!(
                    "{mark} {}  {}  {}",
                    profile.id(),
                    profile.name(),
                    servers_text(profile.config())
                ),
            )?;
        }
        Ok(())
    }

    fn use_dns(&mut self, key: &str, out: &mut dyn Write) -> Result<(), CliError> {
        let mut profiles = self.profiles()?;
        let (profile, is_new) = resolve_dns(&profiles, key)?;
        self.dns.activate(&profile).map_err(CliError::failure)?;
        if is_new {
            profiles
                .add_dns(profile.clone())
                .map_err(CliError::failure)?;
            self.save(&profiles)?;
        }
        say(
            out,
            format!(
                "DNS is now {} ({}).",
                profile.name(),
                servers_text(profile.config())
            ),
        )
    }

    fn dns_off(&mut self, out: &mut dyn Write) -> Result<(), CliError> {
        if self.dns.active_id().is_none() {
            return say(out, "No DNS profile is active; nothing to restore.");
        }
        self.dns.deactivate().map_err(CliError::failure)?;
        say(out, "DNS restored to its original settings.")
    }

    fn add_dns(&self, name: &str, servers: &[String], out: &mut dyn Write) -> Result<(), CliError> {
        let config = DnsConfig::from_strs(servers).map_err(CliError::failure)?;
        let mut profiles = self.profiles()?;
        let id = unique_id(&slug(name), |id| profiles.dns(id).is_some());
        let profile = DnsProfile::new(&id, name, config).map_err(CliError::failure)?;
        profiles.add_dns(profile).map_err(CliError::failure)?;
        self.save(&profiles)?;
        say(out, format!("Saved DNS profile `{id}`."))
    }

    fn remove_dns(&self, key: &str, out: &mut dyn Write) -> Result<(), CliError> {
        let mut profiles = self.profiles()?;
        let id = find_saved(
            profiles.dns_profiles().iter().map(|p| (p.id(), p.name())),
            key,
        )
        .ok_or_else(|| CliError::failure(format!("no saved DNS profile `{key}`")))?;
        if self.dns.active_id() == Some(id.as_str()) {
            return Err(CliError::failure(format!(
                "`{id}` is active; run `hopswitch dns off` first"
            )));
        }
        profiles.remove_dns(&id).map_err(CliError::failure)?;
        self.save(&profiles)?;
        self.prune_pool(AutoTarget::Dns, &id);
        say(out, format!("Removed DNS profile `{id}`."))
    }

    // ---------- both ----------

    fn off(&mut self, out: &mut dyn Write) -> Result<(), CliError> {
        if self.proxy.active_id().is_none() && self.dns.active_id().is_none() {
            return say(out, "Nothing to restore: neither proxy nor DNS is active.");
        }
        restore_all(&mut self.proxy, &mut self.dns).map_err(CliError::failure)?;
        say(out, "Restored the original proxy and DNS settings.")
    }

    fn status(&self, out: &mut dyn Write) -> Result<(), CliError> {
        // A damaged profile file must not break `status`; the id is shown instead.
        let profiles = self.profiles().unwrap_or_default();

        let proxy = match self.proxy.active_id() {
            Some(id) => profiles.proxy(id).map_or_else(
                || format!("active: {id}"),
                |p| format!("active: {} ({})", p.name(), p.config()),
            ),
            None => match self.network.read_proxy() {
                Ok(ProxyState::Disabled) => "off".to_owned(),
                Ok(ProxyState::Manual(config)) => format!("{config} (not set by Hopswitch)"),
                Ok(ProxyState::Other { .. }) => {
                    "custom setup (not managed by Hopswitch)".to_owned()
                }
                Err(e) => format!("unavailable ({e})"),
            },
        };
        let dns = match self.dns.active_id() {
            Some(id) => profiles.dns(id).map_or_else(
                || format!("active: {id}"),
                |p| format!("active: {} ({})", p.name(), servers_text(p.config())),
            ),
            None => match self.network.read_dns() {
                Ok(DnsState::Automatic) => "automatic (from the network)".to_owned(),
                Ok(DnsState::Manual(config)) => {
                    format!("{} (not set by Hopswitch)", servers_text(&config))
                }
                Ok(DnsState::Other { .. }) => "custom setup (not managed by Hopswitch)".to_owned(),
                Err(e) => format!("unavailable ({e})"),
            },
        };
        say(out, format!("proxy: {proxy}"))?;
        say(out, format!("dns: {dns}"))
    }

    fn ping(&self, options: &PingOptions, out: &mut dyn Write) -> Result<(), CliError> {
        let defaults = self.auto_config()?.connectivity;
        let settings = ConnectivitySettings::new(
            options.url.as_deref().unwrap_or(defaults.proxy_target()),
            options.domain.as_deref().unwrap_or(defaults.dns_domain()),
            options.timeout_ms.unwrap_or(defaults.timeout_ms()),
            defaults.interval_secs(),
        )
        .map_err(CliError::failure)?;
        let profiles = self.profiles()?;
        let mut printed = false;

        let proxies = profiles.proxy_profiles();
        if options.scope != PingScope::Dns && !proxies.is_empty() {
            say(out, format!("proxy (test: {}):", settings.proxy_target()))?;
            let probes = probe_proxies(&*self.prober, proxies, &settings);
            for (profile, probe) in proxies.iter().zip(&probes) {
                let active = self.proxy.active_id() == Some(profile.id());
                let target = profile.config().to_string();
                say(
                    out,
                    ping_line(active, profile.id(), profile.name(), &target, probe),
                )?;
            }
            printed = true;
        }

        let dns = profiles.dns_profiles();
        if options.scope != PingScope::Proxy && !dns.is_empty() {
            say(out, format!("dns (test: {}):", settings.dns_domain()))?;
            let probes = probe_dns_profiles(&*self.prober, dns, &settings);
            for (profile, probe) in dns.iter().zip(&probes) {
                let active = self.dns.active_id() == Some(profile.id());
                let target = servers_text(profile.config());
                say(
                    out,
                    ping_line(active, profile.id(), profile.name(), &target, probe),
                )?;
            }
            printed = true;
        }

        if printed {
            Ok(())
        } else {
            say(out, "No saved profiles to test. See `hopswitch presets`, then `hopswitch proxy use <preset>`.")
        }
    }

    // ---------- auto ----------

    fn auto_command(&self, command: AutoCommand, out: &mut dyn Write) -> Result<(), CliError> {
        match command {
            AutoCommand::Status => self.auto_status(out),
            AutoCommand::Set {
                url,
                domain,
                timeout_ms,
                interval_secs,
            } => self.auto_set(
                url.as_deref(),
                domain.as_deref(),
                timeout_ms,
                interval_secs,
                out,
            ),
            AutoCommand::Toggle { target, option, on } => self.auto_toggle(target, option, on, out),
            AutoCommand::PoolAdd { target, name } => self.pool_add(target, &name, out),
            AutoCommand::PoolRemove { target, name } => self.pool_remove(target, &name, out),
            AutoCommand::Run { .. } => Err(CliError::failure(
                "`auto run` is started from the command line only",
            )),
        }
    }

    fn auto_status(&self, out: &mut dyn Write) -> Result<(), CliError> {
        let config = self.auto_config()?;
        for (label, kind) in [("proxy", &config.proxy), ("dns", &config.dns)] {
            say(
                out,
                format!(
                    "{label}: connect {}, switch {}",
                    on_off(kind.connect),
                    on_off(kind.switch)
                ),
            )?;
            let pool = if kind.pool.is_empty() {
                "(empty)".to_owned()
            } else {
                kind.pool.join(", ")
            };
            say(out, format!("  pool: {pool}"))?;
        }
        let test = &config.connectivity;
        say(
            out,
            format!(
                "test: {}, {}, timeout {} ms, every {} s",
                test.proxy_target(),
                test.dns_domain(),
                test.timeout_ms(),
                test.interval_secs()
            ),
        )
    }

    fn auto_set(
        &self,
        url: Option<&str>,
        domain: Option<&str>,
        timeout_ms: Option<u32>,
        interval_secs: Option<u32>,
        out: &mut dyn Write,
    ) -> Result<(), CliError> {
        let mut config = self.auto_config()?;
        let current = &config.connectivity;
        config.connectivity = ConnectivitySettings::new(
            url.unwrap_or(current.proxy_target()),
            domain.unwrap_or(current.dns_domain()),
            timeout_ms.unwrap_or(current.timeout_ms()),
            interval_secs.unwrap_or(current.interval_secs()),
        )
        .map_err(CliError::failure)?;
        self.save_auto(&config)?;
        say(out, "Test settings saved.")
    }

    fn auto_toggle(
        &self,
        target: AutoTarget,
        option: AutoOption,
        on: bool,
        out: &mut dyn Write,
    ) -> Result<(), CliError> {
        let mut config = self.auto_config()?;
        let kind = kind_mut(&mut config, target);
        match option {
            AutoOption::Connect => kind.connect = on,
            AutoOption::Switch => kind.switch = on,
        }
        self.save_auto(&config)?;
        let what = match option {
            AutoOption::Connect => "connect",
            AutoOption::Switch => "switch",
        };
        say(
            out,
            format!(
                "Auto {what} for {} is now {}.",
                target_name(target),
                on_off(on)
            ),
        )
    }

    fn pool_add(
        &self,
        target: AutoTarget,
        name: &str,
        out: &mut dyn Write,
    ) -> Result<(), CliError> {
        let profiles = self.profiles()?;
        let id = match target {
            AutoTarget::Proxy => find_saved(
                profiles.proxy_profiles().iter().map(|p| (p.id(), p.name())),
                name,
            ),
            AutoTarget::Dns => find_saved(
                profiles.dns_profiles().iter().map(|p| (p.id(), p.name())),
                name,
            ),
        }
        .ok_or_else(|| {
            CliError::failure(format!("no saved {} profile `{name}`", target_name(target)))
        })?;
        let mut config = self.auto_config()?;
        if kind_mut(&mut config, target).add_to_pool(&id) {
            self.save_auto(&config)?;
            say(
                out,
                format!("Added `{id}` to the {} pool.", target_name(target)),
            )
        } else {
            say(
                out,
                format!("`{id}` is already in the {} pool.", target_name(target)),
            )
        }
    }

    fn pool_remove(
        &self,
        target: AutoTarget,
        name: &str,
        out: &mut dyn Write,
    ) -> Result<(), CliError> {
        let profiles = self.profiles()?;
        // A deleted profile can still be removed by its id.
        let id = match target {
            AutoTarget::Proxy => find_saved(
                profiles.proxy_profiles().iter().map(|p| (p.id(), p.name())),
                name,
            ),
            AutoTarget::Dns => find_saved(
                profiles.dns_profiles().iter().map(|p| (p.id(), p.name())),
                name,
            ),
        }
        .unwrap_or_else(|| name.to_owned());
        let mut config = self.auto_config()?;
        if kind_mut(&mut config, target).remove_from_pool(&id) {
            self.save_auto(&config)?;
            say(
                out,
                format!("Removed `{id}` from the {} pool.", target_name(target)),
            )
        } else {
            Err(CliError::failure(format!(
                "`{name}` is not in the {} pool",
                target_name(target)
            )))
        }
    }

    fn presets(&self, out: &mut dyn Write) -> Result<(), CliError> {
        let profiles = self.profiles().unwrap_or_default();

        say(out, "DNS presets (hopswitch dns use <id>):")?;
        for preset in dns_presets() {
            let saved = if preset.is_saved_in(&profiles) {
                " [saved]"
            } else {
                ""
            };
            say(
                out,
                format!(
                    "  {}  {}  {}{saved}",
                    preset.id(),
                    preset.name(),
                    preset.servers().join(", ")
                ),
            )?;
        }
        say(out, "Proxy presets (hopswitch proxy use <id>):")?;
        for preset in proxy_presets() {
            let target = preset
                .config()
                .map_or_else(|e| format!("invalid ({e})"), |config| config.to_string());
            let saved = if preset.is_saved_in(&profiles) {
                " [saved]"
            } else {
                ""
            };
            say(
                out,
                format!("  {}  {}  {target}{saved}", preset.id(), preset.name()),
            )?;
        }
        Ok(())
    }
}

// ---------- helpers ----------

/// Opens one service and resumes the session an earlier run left behind.
fn open_service<K: Kind>(ctx: &Context, file: &str) -> Result<NetworkService<K>, CliError>
where
    JsonFileSessionStore: SessionStore<K::State>,
{
    let store = JsonFileSessionStore::new(ctx.dir.join(file));
    let session = store.load::<K::State>().map_err(CliError::failure)?;
    let mut service = NetworkService::new(ctx.network.clone(), Arc::new(store));
    if let Some(session) = session {
        service.resume(session);
    }
    Ok(service)
}

fn say(out: &mut dyn Write, text: impl Display) -> Result<(), CliError> {
    writeln!(out, "{text}").map_err(CliError::failure)
}

fn ping_line(active: bool, id: &str, name: &str, target: &str, probe: &Probe) -> String {
    let result = match probe.health {
        Health::Alive { latency_ms } => format!("{latency_ms} ms"),
        Health::Dead => "unreachable".to_owned(),
    };
    format!("{} {id}  {name}  {target}  {result}", marker(active))
}

fn marker(active: bool) -> char {
    if active {
        '*'
    } else {
        ' '
    }
}

fn servers_text(config: &DnsConfig) -> String {
    config
        .servers()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// `My Work!` becomes `my-work`; Unicode letters and digits (Persian too) are kept.
fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in name.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch);
        } else {
            pending_dash = true;
        }
    }
    if out.is_empty() {
        "profile".to_owned()
    } else {
        out
    }
}

/// `base`, or `base-2`, `base-3`, ... until `taken` says the id is free.
fn unique_id(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    (2..=u32::MAX)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| base.to_owned())
}

/// The id of the saved entry whose id equals `key`, else whose name does (ignoring case).
fn find_saved<'a>(
    entries: impl Iterator<Item = (&'a str, &'a str)> + Clone,
    key: &str,
) -> Option<String> {
    let mut by_id = entries.clone().filter(|(id, _)| *id == key);
    if let Some((id, _)) = by_id.next() {
        return Some(id.to_owned());
    }
    let wanted = key.to_lowercase();
    entries
        .into_iter()
        .find(|(_, name)| name.to_lowercase() == wanted)
        .map(|(id, _)| id.to_owned())
}

fn unknown(kind: &str, key: &str) -> CliError {
    CliError::failure(format!(
        "no {kind} profile or preset named `{key}`; see `hopswitch {kind} list` and `hopswitch presets`"
    ))
}

/// Finds what `proxy use <key>` means; the flag says the profile is not saved yet.
fn resolve_proxy(profiles: &Profiles, key: &str) -> Result<(ProxyProfile, bool), CliError> {
    let saved = find_saved(
        profiles.proxy_profiles().iter().map(|p| (p.id(), p.name())),
        key,
    );
    if let Some(profile) = saved.and_then(|id| profiles.proxy(&id)) {
        return Ok((profile.clone(), false));
    }
    let preset = proxy_preset(&key.to_lowercase()).ok_or_else(|| unknown("proxy", key))?;
    let config = preset.config().map_err(CliError::failure)?;
    if let Some(existing) = profiles
        .proxy_profiles()
        .iter()
        .find(|p| p.config() == &config)
    {
        return Ok((existing.clone(), false));
    }
    let id = unique_id(preset.id(), |id| profiles.proxy(id).is_some());
    Ok((preset.to_profile(id).map_err(CliError::failure)?, true))
}

/// Finds what `dns use <key>` means; the flag says the profile is not saved yet.
fn resolve_dns(profiles: &Profiles, key: &str) -> Result<(DnsProfile, bool), CliError> {
    let saved = find_saved(
        profiles.dns_profiles().iter().map(|p| (p.id(), p.name())),
        key,
    );
    if let Some(profile) = saved.and_then(|id| profiles.dns(&id)) {
        return Ok((profile.clone(), false));
    }
    let preset = dns_preset(&key.to_lowercase()).ok_or_else(|| unknown("dns", key))?;
    let config = preset.config().map_err(CliError::failure)?;
    if let Some(existing) = profiles
        .dns_profiles()
        .iter()
        .find(|p| p.config() == &config)
    {
        return Ok((existing.clone(), false));
    }
    let id = unique_id(preset.id(), |id| profiles.dns(id).is_some());
    Ok((preset.to_profile(id).map_err(CliError::failure)?, true))
}

// ---------- auto run ----------

/// Runs the automatic modes. Services are reopened every round so that a
/// `hopswitch off` from another terminal is noticed.
fn auto_run(ctx: &Context, once: bool, out: &mut dyn Write) -> Result<(), CliError> {
    // One round has no memory of earlier ones, so one failure must be enough.
    let mut runner = if once {
        AutoRunner::decisive()
    } else {
        AutoRunner::new()
    };
    // A problem that persists (say, DNS needing administrator rights) is reported once, not every round.
    let mut last_lines: Vec<String> = Vec::new();
    loop {
        let mut app = App::open(ctx)?;
        let config = app.auto_config()?;
        if !config.proxy.is_enabled() && !config.dns.is_enabled() {
            return say(
                out,
                "Auto connect and auto switch are both off for proxy and DNS; nothing to do. \
                 See `hopswitch auto status`.",
            );
        }
        let profiles = app.profiles()?;
        let report = runner.tick(
            &mut app.proxy,
            &mut app.dns,
            &*app.prober,
            &profiles,
            &config,
        );

        let events = tick_events(&report, &profiles);
        if once || events.lines != last_lines {
            for line in &events.lines {
                say(out, line)?;
            }
        }
        if once {
            if events.lines.is_empty() {
                say(out, "Nothing to change.")?;
            }
            return match events.errors {
                0 => Ok(()),
                n => Err(CliError::failure(format!("{n} automatic change(s) failed"))),
            };
        }
        out.flush().map_err(CliError::failure)?;
        last_lines = events.lines;
        std::thread::sleep(Duration::from_secs(u64::from(
            config.connectivity.interval_secs(),
        )));
    }
}

struct TickEvents {
    lines: Vec<String>,
    errors: usize,
}

fn tick_events(report: &TickReport, profiles: &Profiles) -> TickEvents {
    let mut events = TickEvents {
        lines: Vec::new(),
        errors: 0,
    };
    let proxy_name = |id: &str| {
        profiles
            .proxy(id)
            .map_or_else(|| id.to_owned(), |p| p.name().to_owned())
    };
    let dns_name = |id: &str| {
        profiles
            .dns(id)
            .map_or_else(|| id.to_owned(), |p| p.name().to_owned())
    };
    kind_events("proxy", &report.proxy, &proxy_name, &mut events);
    kind_events("dns", &report.dns, &dns_name, &mut events);
    events
}

fn kind_events(
    label: &str,
    report: &KindReport,
    name_of: &dyn Fn(&str) -> String,
    events: &mut TickEvents,
) {
    let line =
        match (&report.decision, &report.error) {
            (Decision::Keep, _) => return,
            (Decision::Activate(id), None) => {
                let latency = report.probes.iter().find(|p| p.id == *id).map_or_else(
                    String::new,
                    |p| match p.health {
                        Health::Alive { latency_ms } => format!(" ({latency_ms} ms)"),
                        Health::Dead => String::new(),
                    },
                );
                format!("{label}: now using {}{latency}", name_of(id))
            }
            (Decision::Activate(id), Some(error)) => {
                format!("{label}: could not switch to {}: {error}", name_of(id))
            }
            (Decision::Deactivate, None) => {
                format!("{label}: nothing answers; restored the original settings")
            }
            (Decision::Deactivate, Some(error)) => {
                format!("{label}: could not restore the original settings: {error}")
            }
        };
    if report.error.is_some() {
        events.errors += 1;
    }
    events.lines.push(line);
}

fn kind_mut(config: &mut AutoConfig, target: AutoTarget) -> &mut AutoKind {
    match target {
        AutoTarget::Proxy => &mut config.proxy,
        AutoTarget::Dns => &mut config.dns,
    }
}

fn target_name(target: AutoTarget) -> &'static str {
    match target {
        AutoTarget::Proxy => "proxy",
        AutoTarget::Dns => "DNS",
    }
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

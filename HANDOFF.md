# HANDOFF — Hopswitch (state at 2026-10-30, step 07zn, 549 tests green; see the git log for HEAD)

> **Paste-in starter for the new chat** (the user writes Persian, informal; attach the project zip,
> this file; **do not ask him to paste CI results, read them yourself, see §8**):
>
> «سلام! پروژه‌ی Hopswitch رو از چت قبلی ادامه می‌دیم. zip پروژه و HANDOFF.md پیوست شده.
> اول `HANDOFF.md`، `AGENT.md`، `CHECKLIST.md` و `docs/adr/*` رو بخون. نتیجه‌ی CI رو خودت از شاخه‌ی
> `ci-report` ریپو (`mohammadraziei/hopswitch`) بخون و طبق بخش ۹ ادامه بده.»
>
> Reply in Persian (casual, friendly). Code, comments, commit messages and docs stay in English.

---

> **READ FIRST (07zn): the Cargo workspace was merged into ONE package `hopswitch`** (ADR-0013, the user's decision: only one name goes to crates.io).
> Older sections of this file say "crate"/"crates/..."; they were rewritten to the new paths where that was mechanical, but the **names** of the former
> crates remain in prose. Map: `hopswitch-core` = module `src/core`; `-storage` = `src/storage`; `-http` = `src/http.rs`; `-probe` = `src/probe`;
> `-platform-windows` = `src/platform_windows`; `-pacserver` = `src/pacserver.rs`; `-cli` = `src/cli` (+ `src/main.rs`, the composition root). Tests: `tests/<module>_<name>.rs`;
> examples: `examples/`. No `-p ...` flags any more: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, `cargo publish`.
> The layering is enforced by `tests/architecture.rs` (a table of who may use whom; core may not touch the OS). Add a module there when you add one.
> The React frontend must go to `ui/`, not `src/`.

## 0. Where we stopped (read this first)

* **Pushed vs not.** When this file was written the newest CI report on the `ci-report` branch was for master
  `e7ff0cc` (all 6 jobs green, PAC benchmark green). Steps **07y** (stale PAC detection: `status`/`off`) and **07z**
  (`--version`, bare-command decision, ADR-0012) were *not yet pushed*. Compare the report's `commit:` line with
  `git log` in the zip before trusting it.
* **Linux adapter question ANSWERED (2026-10-29):** the user wants shell proxy variables and delegated the
  rest. Decided in ADR-0012 (his own refinement): a Hopswitch-owned `proxy.env`, and a managed block that
  sources it written into the shell start file **once** and then left alone (`off` only empties the file).
  Current user first, new shells only, system-wide later. Start files, block position and SOCKS mapping
  still to research (step d). Do not ask him again.
* **Choose the next piece with the user (all are specified):**
  1. *Finish "apply a Rules profile"* (§9 step 5 rest): glue `RulesProfile` -> `PacPlan` -> `PacServer` ->
     `activate_state(Pac(url))`, with the **SOCKS refusal** (decided: fail closed) and a resolver for group targets, plus
     CLI `rules ...` commands. Needs a process that stays running (`serve`/daemon).
  2. *ADR-0012 step (c)*: the MCP server, read-only first (on top of `hopswitch-http` and the same token rules);
     then the Linux adapter (d), `service install` (e), UI pages (f), release builds (g). Step (b), `serve`, is
     **done in 07za-07ze** (see "07ze" below).
  3. *`#[non_exhaustive]`* on public enums before publishing the crates (small, open).
  Recommendation: (1) next, because `serve` now exists as the long-running process it needs; or (2)(c).
* **Working method that paid off:** TDD with the red step actually run; check tests catch deliberate breakage;
  run everything on Rust 1.75 (apt) **and** 1.99.1 (conda, §7) before handing over; read CI from the `ci-report`
  branch yourself; correct earlier claims in writing when a measurement contradicts them (twice this session).


## 0. The 60-second version

* **Hopswitch** = a cross-platform desktop **client** that switches the **system proxy** and **system DNS**
  between saved profiles (tray + UI planned). It never runs or ships a proxy server.
* **Built and tested (pure Rust, 545 tests):** domain model, services with exact snapshot/restore,
  JSON persistence, Windows proxy adapter, a real connectivity prober (DNS / HTTP / SOCKS), auto connect +
  proxy groups ("fastest of ..."), and a full **CLI** (`hopswitch`). **Not built:** the Tauri shell and UI,
  Windows DNS (needs admin), macOS and Linux adapters, per-domain rules.
* **Phase 2 is under way.** CI is green and the `PAC benchmark` passed the ADR-0011 gate on real Windows
  (details in §10). Done: `RuleList` (importer/normaliser) and `ProxyState::Pac(PacConfig)` (core, Windows
  adapter, CLI `status`). Rules profiles exist in the model (07q) and the PAC generator exists (07r). The SOCKS question is decided (fail closed, §10) and the PAC server exists (07w, crate `hopswitch-pacserver`). Since 07za the HTTP plumbing (loopback only, `Host` check, limits, panic-safe handlers) is crate `hopswitch-http`, shared by the PAC server and the coming `serve`. Next: what is left of §9 step 5 (below), then step 6.
* **Process is strict:** TDD (red seen before green), quality gate before every commit, conventional
  commits, zip the whole project after every step (`present_files`).

---

## 1. What the project is

* Windows / macOS / Linux client. Stack: **Tauri 2 (Rust)** + React/TypeScript/Tailwind (ADR-0001).
  Core logic is pure Rust with no OS or Tauri imports; OS access only behind traits (`SystemNetwork`,
  `CommandRunner`, `Prober`, stores).
* Main audience: Iranian users who need (a) local sites direct while everything else goes through a
  proxy, (b) sanction-bypass for developer services, (c) one-click proxy/DNS switching with a guaranteed
  restore of the original settings.
* The user's biggest fear is damaging existing system settings (e.g. wiping `/etc/resolv.conf` entries).
  That is why every change is preceded by a persisted snapshot and `off` restores *exactly*; foreign
  system files are only edited through `core::managed_block` (ADR-0003, AGENT.md rule 7).

## 2. How the user works (read this twice)

* Writes Persian, informal. Often delegates ("تصمیمش با خودته", "شما استادی برو جلو"), but expects the
  reasons for decisions to be stated, and he *does* push back when something is off (he corrected the
  colour theme and the FOD licensing stance; both corrections were right).
* Pastes **CI logs from GitHub** after pushing. He pushes himself; the assistant has no push access.
  His GitHub history is **not identical** to the zip's `.git` (his commit messages differ, e.g. "chore: add
  CI workflow..." / "feat: implement PAC benchmark workflow..."): treat the **zip's working tree as the
  source of truth** and expect him to copy files over his clone.
* Wants: step-by-step work, **strict TDD**, clean idiomatic code, `CHECKLIST.md` ticked, the project zipped
  and delivered after every step (name `hopswitch-step-NN.zip`, delete the previous zip, exclude
  `target/`), research done with real sources instead of guesses, honest statements of what is *not*
  verified.
* UI taste: modern, polished, dark-first, RTL/Persian first-class. He will give design feedback himself.
* His OS was never confirmed; CI covers Windows, macOS and Linux.

## 3. Repository map

```
hopswitch/
├── AGENT.md  CHECKLIST.md  README.md  HANDOFF.md      # rules, roadmap, public README, this file
├── Cargo.toml        # ONE package `hopswitch` (lib + bin, ADR-0013); rust-version = "1.75"; lints: unsafe_code=forbid, missing_docs=warn, clippy pedantic
├── rustfmt.toml      # max_width = 100
├── src/
│   ├── lib.rs  main.rs   # main.rs = composition root (gives the CLI WindowsNetwork and NetProber)
│   ├── core/             # pure Rust domain + services + ports (was hopswitch-core)
│   ├── storage/          # write_atomic, JsonFileRepository, JsonFileSessionStore, JsonFileAutoStore
│   ├── platform_windows/ # WindowsNetwork (proxy via registry + WinINet), ProcessRunner; `pub mod registry` = pure parse/plan/render
│   ├── probe/            # NetProber, std only: DNS A query over UDP, HTTP through HTTP/SOCKS4a/SOCKS5
│   ├── http.rs           # shared loopback HTTP server (std only); pacserver.rs is a handler on it
│   └── cli/              # command.rs (parser) + app.rs (behaviour) + serve.rs (web API); binary `hopswitch`
├── tests/                # tests/<module>_<name>.rs and architecture.rs (who may use whom)
├── examples/             # gen_pac.rs, proxy_smoke.rs
├── tools/pac-bench/      # spike for ADR-0011: make_pac.py, verify_local.py, bench.ps1, README.md
├── design/               # README, preview/*.dc.html (UI mockups, canvas.json), logo/ (SVG sources + build.py + export/)
├── docs/                 # ARCHITECTURE.md, adr/0001 … 0011
└── .github/workflows/    # ci.yml (lint, test x3 OS, msrv, windows-proxy-smoke), pac-bench.yml (manual)
```

Tests live in `tests/<module>_<name>.rs` (public API). The per-crate counts above were written at 354 tests; the total is now 444 (rule list +23, PAC state +21, rules profiles +35, PAC generator +13).

## 4. Domain model and the invariants that matter

* **Validated types, "parse, don't validate".** `ProxyConfig`, `DnsConfig`, `ProxyProfile`, `DnsProfile`,
  `ProxyGroup`, `Profiles`, `ConnectivitySettings` can only be built valid, including through `serde`
  (`try_from` DTOs). Never add a public constructor/field that bypasses this.
* **`ProxyState`** = what the system really had: `Disabled` (nothing switched on **and nothing stored**),
  `Manual(ProxyConfig)`, `Other { raw, enabled }` (anything else, restored value for value; `enabled`
  says whether it routes traffic; old sessions without the flag read as `true`).
  **`DnsState`** = `Automatic`, `Manual(DnsConfig)`, `Other { raw }`.
* **Services** (`NetworkService<K>`, i.e. `ProxyService` / `DnsService`): snapshot persisted *before* the
  first change; activate / switch A→B keeps the original; idempotent; `deactivate` restores exactly;
  failed apply rolls back; `resume` after restart; `restore_all`. Proxy and DNS are independent.
* **Profiles** (`profiles.json`, schema 2): `proxies`, `groups`, `dns`. Proxy and group ids share **one
  namespace**; a group's members are **plain proxies only** (no cycles possible); members must exist; a proxy
  used by a group cannot be removed (`ProfilesError::InUse`). Version-1 files still load; an older build
  refuses a version-2 file instead of dropping groups.
* **Auto settings** (`auto.json`, schema 2): `proxy = { connect: bool, selected: Option<id> }` (a plain flag +
  the selected profile, set by `proxy use`), `dns = { connect, switch, pool }`, `connectivity` (probe URL,
  DNS domain, timeout 500–10 000 ms, interval 5–3600 s; defaults `http://cp.cloudflare.com/generate_204`,
  `example.com`, 3000 ms, 30 s).
* **Decision engine** (`Autopilot`, pure): given the active id and probe results, returns
  `Keep | Activate(id) | Deactivate`. Defaults: give up on a profile after **3 failures in a row**; switch only
  for a gain of **≥ 50 ms and ≥ 20 %**; connect-only picks the first live profile in list order, with switch the
  fastest. A profile applied by hand and not in the pool is never touched.
* **`AutoRunner::tick`**: proxy follows the *selected* profile (plain proxy → connect while it answers,
  disconnect after repeated failures; group → fastest live member, switch with hysteresis, failover, disconnect
  when none answers). DNS follows its own options + pool. A failure in one kind never blocks the other. A kind
  that is off sends **no traffic**. `AutoRunner::decisive()` (used by `auto run --once`) gives up after one failure.
* **Manual disconnect wins over automation:** `proxy off`, `dns off`, `off` turn auto connect off for the kind
  that was disconnected and print a note (otherwise `auto run` would undo the user's click). The selection is kept.
* **Probes are real tests, never ICMP:** proxy = HTTP request *through* it; DNS = real query sent to the server.
  A DNS profile's latency = its fastest live server. `https://` targets only test that the tunnel opens (no TLS).
  A TLS proxy (`https://` scheme) is reported dead (unsupported).
* **Presets** (pure data in core): 6 DNS (Cloudflare, Google, Quad9, OpenDNS, AdGuard, Shecan), 7 proxy
  (v2rayN SOCKS 10808 / HTTP 10809, Clash 7890, Clash Verge 7897, Shadowsocks 1080, Tor 9050, Tor Browser 9150).
* **Windows adapter:** proxy only. Registry values `ProxyEnable/ProxyServer/ProxyOverride/AutoConfigURL/
  AutoDetect`, PowerShell through the `CommandRunner` port, WinINet refresh after each write. `Disabled`
  writes remove `ProxyServer`, `ProxyOverride`, `AutoConfigURL`. Stored-but-switched-off settings are
  `Other { enabled: false }`. DNS returns `Unsupported` (needs admin; Step 10).

## 5. The CLI today

Settings directory: `%APPDATA%\Hopswitch` (Windows), `~/Library/Application Support/Hopswitch` (macOS),
`$XDG_CONFIG_HOME/hopswitch` or `~/.config/hopswitch` (others); override with env `HOPSWITCH_DIR`.
Files: `profiles.json`, `auto.json`, `session-proxy.json`, `session-dns.json`. Damaged files are reported
and **never modified or deleted**; `off` works even if `profiles.json` / `auto.json` are damaged.
On non-Windows systems the system adapter is `UnsupportedNetwork` (profiles, `ping`, groups and settings
still work; applying fails with a clear message).

```
hopswitch status | presets | help
hopswitch proxy use <name|preset> | off | list | add <name> <scheme://host:port> [--bypass a,b]
hopswitch proxy add-group <name> <member>...      # "fastest of ..."; use <group> applies the fastest live member
hopswitch proxy remove <name>                     # a proxy or a group
hopswitch dns use|off|list|add <name> <ip[,ip...]>|remove
hopswitch ping [proxy|dns] [--url U] [--domain D] [--timeout MS]
hopswitch auto status | set [--url U] [--domain D] [--timeout MS] [--interval SECS]
hopswitch auto proxy connect on|off
hopswitch auto dns connect|switch on|off | pool add|remove <name>
hopswitch auto run [--once]
hopswitch off
```

`proxy use` also selects the profile for auto connect. A group with no live member changes nothing.
`auto run` reopens the services every round and prints a persistent problem only once.
(The help text is generated in `src/cli/app.rs`, `USAGE`.)

## 6. Decisions (ADRs) and unwritten rules

| ADR | Decision |
| --- | --- |
| 0001 | Tauri 2 over Flutter |
| 0002 | Name **Hopswitch** (crate prefix `hopswitch-*`) |
| 0003 | Foreign system files only via a marked block; prefer `resolvectl`/`nmcli`; never replace a symlinked resolv.conf; backup first; glibc reads only 3 nameservers |
| 0004 | UI: two independent panels (Proxy ∣ DNS) + 64 px icon rail (Home / Settings / About); tray mirrors panels; `ProxyProfile` and `DnsProfile` are separate |
| 0005 | Sessions and services (snapshot before change, exact restore) |
| 0006 | Presets and manual entry |
| 0007 | Persistence (atomic JSON, versioned, corrupt files quarantined only on request) |
| 0008 | OS adapters; **addendum**: `Disabled` = nothing stored; stored-but-off = `Other { enabled: false }` |
| 0009 | Real probes (not ICMP); auto connect/switch as a pure decision engine; hysteresis |
| 0010 | Profile kinds, SwitchyOmega-style: `Proxy`, `Group` (done), `Rules` (phase 2), `Apps` (undecided); auto connect = plain flag + selected profile; manual disconnect wins |
| 0011 | Domain rules and public rule lists; compact PAC; **proposed**, waits for the Windows measurement |

Also in force: no `unwrap/expect/panic` outside tests; `missing_docs` is a warning (and CI uses `-D warnings`),
so every public item needs docs; clippy pedantic is on; tests in `tests/<module>_<name>.rs`; one checklist step ≈ one
commit; the UI will use `design-system/MASTER.md` (Step 8, ui-ux-pro-max) and the checklist in AGENT.md.

## 7. CI and the toolchain

`.github/workflows/ci.yml` jobs: **`lint`** (ubuntu: `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings`
on newest stable), **`test`** (ubuntu / windows / macos, `cargo test --no-fail-fast`), **`msrv`** (Rust 1.75.0),
**`windows-proxy-smoke`** (real registry on a runner: apply `v2rayn-socks` through the CLI, read the registry,
`off`, assert the registry is back; **second round** with a stored-but-off address). `pac-bench.yml` is manual
(`workflow_dispatch`) or runs when `tools/pac-bench/**` changes.

**CI uses the newest stable Rust; clippy gains lints between releases and has failed CI twice** because the
sandbox had 1.75. To see what CI will see, install the same version. In the sandbox (no rustup) this worked:

1. From <https://conda.anaconda.org/conda-forge/> download `linux-64/rust-1.99.1-h5387ef0_0.conda` (≈200 MB)
   and `noarch/rust-std-x86_64-unknown-linux-gnu-1.99.1-h480d76f_0.conda` (≈41 MB). Find current files in
   `linux-64/current_repodata.json` and `noarch/current_repodata.json`.
2. Each `.conda` is a zip with a `pkg-*.tar.zst`: unpack both into one folder with Python (`zipfile`,
   `pip install zstandard`, `tarfile`).
3. `export PATH=<dir>/bin:$PATH LD_LIBRARY_PATH=<dir>/lib CARGO_TARGET_DIR=/tmp/target199` (separate target dir).
4. `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test`. Many lints are fixed
   by `cargo clippy --fix --allow-dirty --allow-no-vcs --all-targets`.
5. Also run the MSRV: `cargo +1.75.0 test` (or the apt Rust 1.75 that the sandbox has by default).

`rust-version = "1.75"` is declared in `Cargo.toml` (clippy respects it, so it will not suggest newer APIs).
Some dependencies are pinned for old toolchains (`thiserror = "1"`, `tempfile = "=3.10.1"`).

## 8. Status: verified, unverified, waiting on the user

**Verified by running:** all 354 tests on Rust 1.75 **and** 1.99.1; `fmt` and `clippy -D warnings` on both;
the CLI binary against a local fake proxy on Linux (`ping`, groups, `auto status`); the PAC variants in V8 (Node 22)
and QuickJS (0 wrong answers in 174 723 hosts). **Observed on a real Windows runner (CI run before Step 4b):**
`proxy use v2rayn-socks` wrote `ProxyEnable=1`, `ProxyServer=socks=127.0.0.1:10808`; `status` showed it active.

**Not verified (do not claim otherwise):**
* The newest CI run (lint, test ×3, msrv, windows-proxy-smoke including the stored-address round). The last run
  the user pasted (#4) had two failures, both fixed afterwards: clippy 1.99 `assert_is_empty` in tests, and the
  Windows `hopswitch-probe --test proxy` target. **The failing test's name was not in the truncated log**; the fix
  (looser timing: refused loopback connects take ~2 s on Windows, `thread::sleep` can wake early) is a best guess.
  If it fails again, ask for the lines `---- <test name> stdout ----` and the next 10 lines.
* `pwsh` is not available in the sandbox, so PowerShell is only ever verified by the user's CI runs.
* PAC clients other than WinHTTP (Chromium, Firefox, macOS, Linux). The exact PAC size cap in Chromium (its
  fetcher defines one; the value was not visible in what was read; a third-party note says 1 MB).
* Real Windows behaviour of the new `Pac` write (clear the manual proxy, then set `AutoConfigURL`): only the
  pure registry logic is tested. The next CI smoke run does not exercise it yet; add a PAC round to
  `windows-proxy-smoke` when the local PAC server exists.
* Preset values against official docs (Shecan was confirmed through third-party sources; Electro omitted).
* Tauri, the UI, macOS and Linux were never built or run. The sandbox cannot run a GUI.

**Reading CI results yourself (the user is tired of relaying them, so never ask him to paste any).** The repo is
public: `https://github.com/mohammadraziei/hopswitch`, default branch `master`. After every push the
`failure report` job of each workflow writes a short Markdown report to the **`ci-report` branch**; fetch it with
`curl` from the sandbox (`raw.githubusercontent.com` and `github.com` are allowed; the unauthenticated
`api.github.com` is usually rate-limited, so do not rely on it):

* `https://raw.githubusercontent.com/mohammadraziei/hopswitch/ci-report/ci.md` — the CI workflow: per-job results,
  and for each failed job the failed step plus the log lines that say why (failed test names and panics, compiler
  and clippy errors, `##[error]` lines).
* `https://raw.githubusercontent.com/mohammadraziei/hopswitch/ci-report/pac-bench.md` — the PAC benchmark: the
  WinHTTP table and the keyword table (runs by itself when a push touches the PAC generator, the list importer,
  `tools/pac-bench`, `tools/ci` or its own file).

Check that the report's `commit:` line equals the HEAD you expect (`git log -1` in the zip); `raw` is cached for a few
minutes, so retry after a minute if it is the previous commit's report. If the commit never appears the push did not
happen or CI did not start: say so, do not guess. The scripts are `tools/ci/report.py` (tested by
`tools/ci/test_report.py`, also run in the `lint` job) and `tools/ci/publish.sh`. **Not verified end to end:** they
were tested on fixtures, on real `cargo test` / `clippy` output and against a local bare repository; the first real
run on GitHub is the first time the whole chain runs. If `ci.md` does not appear after a push, the likely causes are
the `report` job's token permissions (`contents: write`; a repository setting "Workflow permissions: read only" may
override it) or a bug in these two scripts.

**Waiting on the user (ask for these first):**
1. (Done: the user pushes; nothing to paste. Ask only for things the reports cannot show.)
2. (Done 2026-10-26: CI green; PAC benchmark table received, see §10.)
3. Optional manual checks on his own Windows machine: `hopswitch auto run --once`, then `hopswitch auto run`.
4. Upload `design/logo/export/social-preview.png` in the repo settings (Social preview).

## 9. What to do next (ordered)

1. ~~Read the CI and PAC results~~ (done: CI green).
2. ~~Apply the ADR-0011 decision gate~~ **PASSED** (§10). Build phase 2 with TDD, in this order
   (items 1 and 3 are DONE in step 07o, the model part of item 2 in step 07q):
   1. **DONE** `ProxyState::Pac(PacConfig)` as a first-class state (today a PAC is read as `Other`): Windows `AutoConfigURL`
      with no manual proxy; keep restore exactness (extend `tests/restore.rs`); update CLI `status`.
   2. **DONE (model, step 07q)** `Target` (`Direct` is a built-in target, not a saved profile), `Rule`, `RulesProfile`
      (ordered rules + default, first match wins, domain-suffix matching, duplicate domain refused) in `Profiles`
      (shared id namespace, targets must be saved proxies/groups, used targets cannot be removed; `profiles.json`
      schema 3). **Still open:** list bindings (a list + target, always *after* the user's own rules; arrives with the
      updater, step 6), `ProxyEntry`/automation support (a rules profile is deliberately not selectable yet), CLI.
   3. **DONE** **Rule lists** (`RuleList`, `ListFormat`, `ImportReport`, `add_suffix`): importer for the two formats (plain `domains.txt`, dotted `.example.com`), normalise
      (lowercase, strip dot, dedupe, drop+count invalid, drop entries covered by a parent), one `.ir` suffix rule
      instead of 69 k entries.
   4. **DONE (07r)** **PAC generator in Rust** (`PacPlan::new(default).with_rule(..).with_list(..).generate()`,
      `PacPlan::from_profile(&profile, resolver)`; see §10 for what is verified) — original brief: (the Python spike in `tools/pac-bench/make_pac.py` is the reference): compact
      bucket format, ES3 only, plain host names `DIRECT`, lowercase, `hasOwnProperty` guard. Must pass the same
      exhaustive reference test (every listed domain, a subdomain of each, near-misses, look-alikes) run with Node in CI.
      Budget: hard 1 000 000 bytes, target 750 000.
   5. **Local PAC server** — **server DONE (07w)**: `src/pacserver.rs`, `PacServer::start(script)`, `.url()` (a
      `PacConfig`), `.set_script()`, `.stop()`/drop; loopback only, free port, std only; refuses a wrong `Host` header
      (DNS rebinding, 403), one thread per connection capped at 32, `431` over 8 KiB of headers, `no-store`, GET/HEAD
      only. 22 tests over real sockets (stressed 30x, three deliberate breakages: two caught; the drain-after-reply
      in `finish()` is **not** covered by any test on Linux, it protects against a reset that destroys the reply on
      Windows). **Still to build:** (a) the glue that turns a `RulesProfile` into a running server plus
      `ProxyState::Pac(server.url())` on the Windows adapter, with the **SOCKS check** (`PacPlan::uses_socks()` ->
      refuse, ADR-0011) and a resolver for group targets; (b) the rule that applying a `Rules` profile needs a running
      Hopswitch process (desktop app, or an explicit `hopswitch serve`); (c) watchdog/cleanup of the PAC setting on
      exit and after a crash. **What is already solved (07x):** the session snapshot is persisted *before* the system
      is changed, so after a crash `resume(saved session)` + `deactivate()` restores the original exactly
      (`NetworkService::activate_state`, test `a_crash_while_a_pac_is_applied_still_leaves_the_original_restorable`),
      and `PacConfig::hopswitch_port()` tells Hopswitch's own server address from any other PAC (only the exact
      `http://127.0.0.1:<port>/hopswitch.pac`). **What is NOT known:** what the system does while the PAC address is
      dead. I first wrote "every web request fails"; that was a guess and is probably wrong for browsers, which
      are generally reported to fall back to *direct* when a PAC cannot be loaded (a silent leak, not an outage);
      neither is verified. The `PAC benchmark` now prints what WinHTTP returns for a dead PAC address (row "a PAC
      address nothing listens on"): **measured 2026-10-28 on real WinHTTP: it returns error 0x2f87
      (ERROR_WINHTTP_BAD_AUTO_PROXY_SCRIPT), not "no proxy"**; what each application then does (go direct, or fail)
      is still unmeasured, so messages must not claim either. **Detection DONE (07y):** `Prober::probe_local_port`
      (a third method; `NetProber` does a TCP connect to 127.0.0.1:port, `FakeProber::with_local_port`),
      `stale_hopswitch_pac(state, prober, timeout)` in core (only Hopswitch's own exact address is judged, nothing is
      probed for anything else); CLI `status` says "nothing is listening; run `hopswitch off`" (also while a session
      is active, which is what a crash leaves) and "served by Hopswitch" when it is alive; `off` / `proxy off` with a
      saved original restore it as before, and with **no** saved original switch a dead own address off (the one
      always-safe state) instead of saying "nothing to restore"; an alive own address or a foreign PAC is left alone.
      **Still open:** no watchdog exists while Hopswitch is not running, so until the user runs any Hopswitch
      command the dead address stays (a login-time task or helper is a later decision); the dead address is only
      detected by commands, not on a timer.
   6. **Updater** (opt-in, release asset URL, check `.sha256` when published, cache, keep the old list on failure,
      download through the selected proxy, manual import) + bundled snapshot + `THIRD_PARTY_NOTICES`.
   7. CLI: `rules ...` commands; presets "Iran: local sites direct" and "Developer services" (both off by default).
   If the gate **fails**: shrink the list (only the `proxy` and hand-picked categories) or move per-domain routing to
   a local router (ADR-0010 §5, needs its own ADR because it changes what the product is).
3. **Step 7 — Tauri shell** (commands, capabilities allow-list, tray, single instance, autostart) and a background
   loop calling `AutoRunner::tick`; then **Step 8/9** design system and UI (profile editor for kinds, group picker,
   auto-connect checkbox next to the selected profile, latency badges, Settings > Connectivity).
   The sandbox cannot run a GUI: keep logic in `core`/adapters, write the shell carefully, and ask the user to run
   `npm run tauri dev` locally and paste errors.
4. **Step 10 — Windows DNS** through an elevated helper (never run the whole app elevated). Until then auto for DNS
   does not work on Windows.
5. **Steps 5/6 — macOS (`networksetup`) and Linux (`gsettings`, `resolvectl`/`nmcli`, managed block)** adapters.
6. Before launch: reserve GitHub org, domain, trademark search, npm/crates.io names; re-verify preset values.

## 10. Data sources for rules (ADR-0011)

* **`bootmortis/iran-hosted-domains`** — MIT, rebuilt by a GitHub Action every one to two weeks. Release assets include
  `domains.txt` (<https://github.com/bootmortis/iran-hosted-domains/releases/latest/download/domains.txt>), Clash,
  sing-box, v2ray `.dat`, Surge, `switchy_omega.sorl`, each with a `.sha256`. Categories: `all`/`tld-ir`/`other`
  (→ direct), `ads` (→ block), `proxy` (Iran-related domains that are filtered inside Iran → proxy).
  Measured (2026-10-24): 127 069 domains, no duplicates, none invalid; 69 512 end in `.ir`, 57 557 do not (48 910 `.com`).
* **`freedomofdevelopers/fod`, file `domains`** (<https://raw.githubusercontent.com/freedomofdevelopers/fod/master/domains>)
  — services that block Iran, `.example.com` per line; 620 lines, 605 distinct, no invalid. Contributed publicly
  through issues; the README mentions CC BY-SA 3.0. **Project stance (the user's call):** treat it as a public list
  of facts, credit it, ship a normalised snapshot (not legal advice). Do **not** add their proxy server
  (`fodev.org:8118`, plain HTTP, third party, no guarantee) as a preset.
* **PAC sizes measured:** object per domain 2 276 532 B; `.ir` as one rule 1 074 041 B; **compact buckets + `.ir`
  rule 649 448 B** (254 KB gzip); FOD 10 718 B. Load ≈ 10 ms, 4–7 µs per lookup in V8 and QuickJS.
  Third-party notes claim Chromium re-evaluates PAC often; unverified.
* **Windows PAC engine measured (2026-10-26, `PAC benchmark` run, 535 sample hosts per variant):**
  `bucket-ir` 649 466 B -> ok, 0 wrong, first lookup (download + compile) 432.9 ms, mean 250.4 us;
  `fod` 10 728 B -> ok, 0 wrong, 22.3 ms, 280.5 us; `object-full` 2 276 542 B and `object-ir` 1 074 053 B ->
  both FAILED in `Resolve` with 0x2f87 (`ERROR_WINHTTP_BAD_AUTO_PROXY_SCRIPT`, 12167). The compact bucket
  format is therefore mandatory. The generator in Rust must reproduce `make_pac.py`'s `bucket-ir` output.
* **PAC generator (step 07r) results:** `src/core/pac_script.rs`. Iran list with the `.ir` rule: 649 575 bytes
  (the spike's `bucket-ir`: 649 448), 30 ms to generate, 0 wrong on 327 790 lookups in Node; `fod` 9 778 bytes.
  Tests (`tests/core_pac_script.rs`) run the script in Node and compare with the product's own matching
  (`RuleList::matches`, rule order) on ~64 000 hosts, including look-alikes, upper case, trailing dots and
  host names such as `constructor.com`; breaking the generator on purpose is caught (two mutations tried; the
  `hasOwnProperty` guard has no observable effect and is purely defensive). **Node is required in CI**
  (`HOPSWITCH_REQUIRE_NODE=1` in `ci.yml`); locally the Node tests skip with a message when Node is missing.
  **Verified on real WinHTTP (2026-10-26 run):** `rust-bucket-ir` and `rust-fod` pass (0 wrong of 535; first
  lookup 40.7 ms / 27.1 ms). The workflow builds them with `cargo run --example gen_pac`.
  **FINDING that needs a product decision:** the keyword probe shows WinHTTP returns *no proxy* for `SOCKS`,
  `SOCKS4` and `SOCKS5` in a PAC (type 1, empty string), while `PROXY` and `HTTPS` work. An app that asks WinHTTP
  for its proxy would go **direct** (a leak, silently) when a rule says `SOCKS5`. Browsers (own PAC engine) are
  expected to honour it; not measured. Most users run SOCKS5 (v2rayN `v2rayn-socks`). `PacPlan::uses_socks()`
  detects it. **DECIDED (2026-10-26, user: "take the principled way"): fail closed.** Refuse to apply a Rules
  profile on Windows whose targets include a SOCKS proxy, with a message that names the reason and the alternative
  (the same tool's HTTP port, e.g. v2rayN 10809); no warn-and-allow. Reasons in ADR-0011. Implement the check in the
  code that builds the plan for the Windows adapter (step 5/6), with its tests. Known gaps by design: IPv4 literals are matched like host names (a rule
  `0.1` would match `10.0.0.1`); a group target is resolved by the caller (`from_profile`'s resolver).
* **Code review against the BeautifulRust skill (step 07u).** `morganlinton/BeautifulRust` (`skills/rust-best-practices`,
  MIT) was read in full except its `references/`. New code of this session was fixed where it fell short (`FromStr` for
  `PacConfig`, documented limits, a simpler insertion in `add_suffix`, `PartialEq` on `PacPlan`). **Older code, not
  changed in 07u, **done in 07v** (the user said to take the principled way): (1) `AutoSettings::new` took two
  `bool`s; it now takes `AutoMode::{Off, Connect, Switch, ConnectAndSwitch}` (all four combinations have a meaning, per
  the module docs; the config file still stores two flags, converted by the crate-private `AutoMode::from_flags`);
  (2) the six public types without `Debug` now have it (derived, or hand-written where a field is a trait object), and
  `missing_debug_implementations = "warn"` is on in `[workspace.lints.rust]` so clippy `-D warnings` keeps it that
  way; (4) the undocumented consts got doc comments. **Still open:** (3) public enums are not `#[non_exhaustive]`
  (fine while only this workspace matches on them; revisit before publishing the crates).
  Found clean: no `unwrap()` outside tests, no `get_` getters, no `&String`/`&Vec` parameters, no glob imports,
  no `deny(warnings)`, `unsafe` is forbidden.
* **Headless mode, web interface, bare command, MCP (ADR-0012, step 07z).** The user asked for: `--ip`/`--port`
  on a web interface served by the CLI; easy automatic install on machines without graphics; bare `hopswitch` opens
  the GUI (as admin if needed), any argument is the CLI (like MATLAB); an MCP server if useful. ADR-0012 records the
  decisions (one command two shapes; GUI = separate `hopswitch-gui` next to `hopswitch`; elevation per action, not
  for the whole GUI; `serve` default `127.0.0.1:47831` with bearer token, `Host`/`Origin` checks, non-loopback only
  with `--allow-remote`; static musl binaries + `service install`; `hopswitch mcp` over stdio, read-only unless
  `--allow-write`). **Done in 07z:** `hopswitch --version` / `-V`; `decide_launch` + `no_gui_hint` (pure, 10 tests) and
  `main.rs` wiring, checked on the real Linux binary (no display, display without GUI, display with a stand-in GUI
  script, and that any argument never starts the GUI). **Untested, by necessity:** starting a real GUI on
  Windows/macOS, `SSH_CONNECTION` handling on those two. The hint text must not mention `hopswitch serve` until
  the interface *pages* exist (07ze made `serve` an API only, with no page at `/`; the doc comment of `no_gui_hint`
  is the place to add it). **Not done:** MCP, Linux adapter, `service install`, release builds. **Linux adapter question: answered** (shell proxy
  variables; see ADR-0012). Do not build `apt`/`docker`/DNS on a guess.
  Order of work is in ADR-0012: (b) `serve`, (c) MCP read-only, (d) Linux adapter, (e) `service install`, (f) UI pages,
  (g) releases.
* **Decision in step 07o:** a PAC is `ProxyState::Pac` only when no manual proxy is in use or stored; `AutoDetect`
  is never read into the state nor written (same as for manual proxies), which keeps restore exact. A PAC with
  a stored server, proxy switched on, or an address Hopswitch cannot model stays `Other`.

## 11. Design, theme, logo

* **Theme: dark first, emerald accent** (from `design/preview` boards): bg `#0F1115`, surface `#171A21`, border
  `#262B36`, text `#E8EAF0`, muted `#9AA3B2`, accent `#34D399` (lighter `#6EE7B7`, text on accent `#04130C`);
  light theme accent `#059669`; Vazirmatn for Persian; RTL via logical CSS properties, IPs `dir="ltr"`.
  Editor, Empty and Rtl boards are outdated (old bundled-profile model, no rail). `Artboard-be99` is the user's: leave it.
* **Logo** (`design/logo`): a switch whose knob hops (green track = "on", arrow = the hop), dark squircle with a faint
  green rim. Three hand-drawn SVGs: `icon.svg` (≥ 48 px), `icon-small.svg` (16–32 px, arrow dropped because it reads as a
  padlock when small), `tray-template.svg` (black + alpha for the macOS menu bar). `build.py` generates light/dark
  lockups (Poppins Bold converted to paths), Tauri-named PNG/ICO exports, tray icons and a 1280×640 social preview.
  Needs `pip install novasvg pillow fonttools` and the Poppins font (`POPPINS_DIR`). `novasvg.svg2png` mis-handles
  `bytes` input: pass a `str` or a path.
* Skills evaluated earlier: ui-ux-pro-max (use for `design-system/MASTER.md`), Impeccable (selectively; known tells in
  our mockups: Inter as the only font, nested cards, rounded icon tiles beside headings; it has telemetry, off with
  `DO_NOT_TRACK=1`), taste-skill (not now; marketing pages).

## 12. Environment gotchas (sandbox)

* No rustup. Default toolchain from apt is **Rust 1.75** (see §7 for 1.99.1). `bash_tool` runs `sh`: use `bash` for
  `${PIPESTATUS}`. `cargo test` stops at the first failing test binary: use `--no-fail-fast`.
* **Never `pkill -f <pattern>`** when the pattern also appears in your own command line (it kills the shell). To stop a
  background server, loop over `/proc/*/cmdline` for an exact match. Start servers with
  `setsid nohup … < /dev/null &`, otherwise the tool call hangs.
* Text patches done with Python `str.replace` replace **every** occurrence (it once changed both `ProxyState::Other`
  and `DnsState::Other`), and `cargo fmt` reflows code so a later patch may silently not match: use anchored
  replaces and check the result. The tool's output display may abbreviate `<name>` to `<n>`; the file is fine.
* Network from the sandbox: github.com, raw.githubusercontent.com, crates.io, PyPI, npm and conda.anaconda.org work;
  `web_fetch` only opens URLs the user (or a previous result) supplied. Node 22 and `quickjs` (pip) are available;
  `pwsh` is not.
* Windows/macOS behaviour cannot run here: isolate command construction and parsing in pure functions, test those,
  and let CI's real runners prove the rest.

## 13. Mistakes made in this project (do not repeat)

1. Picked a blue/orange logo **without reading** `design/preview` and this file: the theme is dark + emerald.
   Read the design docs before any visual choice.
2. Stated that Chromium refuses `file://` PAC URLs; the source only says it refuses *redirects* to `file://`.
   Corrected in ADR-0011. Do not state browser behaviour that was not read.
3. Estimated the PAC size (≈ 3 MB) before measuring it (1.07 MB / 0.65 MB). Measure first.
4. Worked on clippy 1.75 while CI ran 1.99: two CI failures. Reproduce the CI toolchain (§7).
5. Wrote tests that assumed Linux timing (probe tests) and a CI assertion stricter than the adapter's promise
   (exposed the real Step 4b restore gap). Make timing assertions generous and test promises, not accidents.
6. Attributed a licensing opinion to FOD's owner; it was the project's own stance. Attribute carefully.

## 14. Open questions for the user

* Which OS does he use daily? (Never confirmed; decides which adapter and what he can test himself.)
* Phase 3 (per-app): accept a local router (changes what the product is) or only "launch a program with the proxy"?
* Confirm the UI direction in ADR-0004 and the profile editor for kinds; he promised to give design feedback.
* Combos (apply a proxy and a DNS together) and TUN/VPN-style capture are out of scope for now.

## 07za-07ze: `hopswitch serve` (ADR-0012 step b)

* **`src/http.rs` (`hopswitch-http`, std only, 19 tests):** the HTTP plumbing shared by every local server.
  `Server::start(addr, name, handler)` refuses anything but a loopback address (`InvalidInput`), checks `Host` (own
  address or `localhost:<port>`; 403 otherwise; a missing `Host` is served), answers 400 for malformed requests and
  431 above 8 KiB of headers, one thread per connection (cap 32; the slot is returned by a `Drop` guard, and a
  panicking handler gets a 500), drops header values with a line break, and ignores a handler's `Content-Length` /
  `Connection`. `hopswitch-pacserver` is now only a handler on it (its 22 tests were not changed).
* **`hopswitch serve [--ip <loopback>] [--port N] [--no-auto]`** (`src/cli/serve.rs`, 22 new tests over real
  sockets). Default `127.0.0.1:47831`; `--port 0` picks a free port. A non-loopback `--ip` is a usage error that
  points to `ssh -L`; there is **no** `--allow-remote` and **no** TLS (decided with the user, ADR-0012).
  Token: 256 random bits from the OS (`getrandom`), file `web-token` in the settings folder, mode 0600 on Unix
  (loosened files are tightened, damaged files replaced), constant-time comparison. Every `/api/` request needs
  `Authorization: Bearer <token>` (scheme case-insensitive); the token is checked **before** the path (no route
  probing); `Origin` must be the server's own or absent; no CORS headers; `Cache-Control: no-store`. The URL is printed
  with the token after `#` so a browser never sends it. Only `GET`/`HEAD` exist. **Only endpoint so far:**
  `GET /api/status` -> `{"version", "status"}` where `status` is the text of `hopswitch status`. `/` is a 404 (no pages).
  `run()` handles `Command::Serve` like `auto run`; `Context` is now `Clone`.
* **Checked:** the real binary run with an isolated HOME and probed with `curl` (401 without a token, JSON with it, 403
  for a foreign `Host` and for a foreign `Origin`, token file `600`, `--ip 0.0.0.0` exits 2); 7 deliberate breakages
  of the security rules each caught by a named test; fmt, clippy `-D warnings`, rustdoc `-D warnings` and all 545 tests
  green on Rust 1.75 and 1.99.1 (conda). Note again: clippy 1.99 flags `assert!(x.is_empty())` in tests (use
  `assert_eq!`); 1.75 does not, so check on both.
* **CI (read 2026-10-29 17:15 UTC):** commit `d07f8d5` on master, all 6 of 6 jobs green (fmt + clippy, Windows registry,
  test on ubuntu/macos/windows, Rust 1.75). The `pac-bench` report is still the one for `e7ff0cc`, i.e. the
  "PAC through WinHTTP" job has not been seen to run since `pacserver` was moved onto `hopswitch-http`; check it.
* **Not verified, by necessity:** the Windows token file
  keeps the folder's inherited rights (no explicit ACL); the auto-connect loop inside `serve` (a thread that calls
  `auto_run` and retries every 30 s when auto connect is off) is not covered by a test, and `serve` itself blocks
  forever, so it is only run by hand.
* **Next:** MCP read-only (c) reusing `hopswitch-http` and the token file; then "apply a Rules profile" (§9 step 5),
  which needs this process. The API has no write endpoints yet, by design.

## 07zg: release workflow (Step 11, first part), modelled on liburlparser

* **Pattern (copied from `MohammadRaziei/liburlparser`, `.github/workflows/orchestrator.yml`):** a push to `master` builds everything and puts it in
  the pre-release `__beta__`; a tag `v<version>` does the same into a real release; anything else (a manual run on another branch) only builds.
  Differences from liburlparser, on purpose: old `__beta__` files are deleted only **after** every file was built (a failed build leaves the previous
  beta intact), and the `__beta__` tag is moved to the new commit (there it stays on the first one); `gh` is used instead of `softprops/action-gh-release`.
* **Files:** `.github/workflows/release.yml` (orchestrator: `init` decides release name and checks that the tag equals `v<Cargo version>`; `package`
  calls the reusable workflow; `publish` writes `SHA256SUMS`, the release text from `.github/release_template.md`, moves the tag, creates/updates the
  release, replaces the files; only for a tag or `master`; `concurrency` queues runs instead of cancelling them), `.github/workflows/package.yml`
  (reusable: `workflow_call`, also `workflow_dispatch`, and a pull request that touches `Cargo.toml`, `Cargo.lock`, `src/cli/Cargo.toml`, `LICENSE` or
  itself), `.github/release_template.md` (placeholders `RELEASE_NAME`, `VERSION_STRING`, `BUILD_NOTE`). actionlint is clean on all three.
* **What is built** (`package.yml`): `x86_64-unknown-linux-musl` (ubuntu; static; also the `.deb` and `.rpm`), `aarch64-apple-darwin` and `x86_64-apple-darwin`
  (both on `macos-latest`), `x86_64-pc-windows-msvc` (`.zip`), and an `msi` job (windows-2022, `cargo wix init` + `cargo wix`, **`continue-on-error`**) so a WiX
  problem cannot stop the release. Each build: `cargo build --release --locked --target T`, smoke test (`--version`; on Linux also "not a
  dynamic executable"), archive with README + LICENSE. Tool versions are pinned (cargo-deb 3.8.0, cargo-generate-rpm 0.21.1, cargo-wix 0.3.9) and installed
  into a cached `~/.cargo-tools`. Both Linux tools read the program from `target/release/hopswitch`, so the workflow copies the musl build there instead of
  relying on how each tool maps `--target` onto asset paths.
* **Report of the release run:** job `report` at the end of `release.yml` (`if: always()`, `continue-on-error`) does what `ci.yml` does: `tools/ci/report.py`
  then `tools/ci/publish.sh report.md release.md`, so the report is on the **`ci-report` branch as `release.md`**
  (`https://raw.githubusercontent.com/mohammadraziei/hopswitch/ci-report/release.md`), not on the release (a failed build never reaches it). No new scripts.
  **Nothing of it has run on GitHub yet** (on 2026-10-29 `release.yml` and `package.yml` were not on master and there was no `__beta__` release);
  `api.github.com` was rate-limited from the sandbox, so only public pages and raw files can be read.
* **Also added:** `LICENSE` (MIT, "Copyright (c) 2026 Hopswitch contributors": the repo declared MIT but had no file; **the user should confirm the
  copyright holder**), `[profile.release]` (strip, lto, codegen-units 1; deliberately no `panic = "abort"` because `hopswitch-http` relies on `catch_unwind`),
  `repository` in `[workspace.package]`, `[package.metadata.deb]` and `[package.metadata.generate-rpm]` in `src/cli/Cargo.toml`.
* **Checked here (Rust 1.99.1, glibc build, not musl):** release binary (937 KB); `.deb` built with the exact workflow command, installed with `dpkg -i`,
  `hopswitch --version` run from `/usr/bin`, removed; `.rpm` built and its contents read with `rpm -qlp`; the archive and checksum steps; the real `run:`
  scripts of `init` and `publish` executed against a fake `gh` and `git` (tag matching / not matching, beta with a new and with an existing release, tag
  release: the recorded calls are the intended ones). 545 tests, fmt and clippy still green on 1.75. The glibc `.rpm` required GLIBC up to 2.34: this is why
  Linux uses musl in CI.
* **First real run (2026-10-29, run #2):** `package / build x86_64-unknown-linux-musl` built fine and `hopswitch --version` ran, but the "static" check failed: it
  looked for the text "not a dynamic executable" in `ldd`, and `ldd` on a static-pie file prints "statically linked". Replaced by "no INTERP header in
  `readelf -l`" (tested on a musl-gcc static-pie file and on the dynamic glibc build). The steps after it (archive, `.deb`, `.rpm`) had not run; the other jobs were not seen.
* **NOT verified (no musl std, no WiX, no macOS, no GitHub API here):** the musl build and its "static" check, the Windows `.zip` step (PowerShell
  `Compress-Archive`), the `.msi` job, the macOS targets, the cache of the tools, and the real `gh release ...` calls and the tag push (permissions,
  tag protection rules). Next step for the user: push to master, read the "Release" run, fix what fails. An empty `Depends:` line is written in the `.deb`
  control file (dpkg accepted it); if a tool complains, drop `depends = ""` and check what cargo-deb writes for a static binary.
* **Not done:** signing (Windows Authenticode, macOS notarization: an unsigned download from a browser shows a warning), Linux aarch64, an install script
  (the stable URLs `releases/download/__beta__/<file>` and `releases/latest/download/<file>` are meant for it), an updater, a README "Install" section.

## 07zl: crates.io preparation (the user decided to publish now: nobody installs it soon, so API churn is cheap)

* Done: the internal crates are declared once in the root `[workspace.dependencies]` with `path` **and** `version = "0.1.0"` (crates.io needs the version; the
  crates use `workspace = true`), and `homepage`, `readme`, `keywords`, `categories` are inherited from `[workspace.package]`. **When the version changes,
  change it in `[workspace.package]` and in the six internal lines of `[workspace.dependencies]`.**
* Checked: `cargo publish --workspace --dry-run` with Rust 1.99.1 packs, rebuilds from the packed copies and verifies all 7 crates (core, http, storage,
  platform-windows, probe, pacserver, cli; `hopswitch` is only the binary name) in dependency order; 545 tests, fmt, clippy still green on 1.75; `Cargo.lock` unchanged.
  All 7 names were free on crates.io on 2026-10-29. **Nothing was uploaded** (needs the user's crates.io token).
* **Publishing is done by CI** (user's decision): job `crates` in `release.yml`, only for a tag `v*`, after `package` and `publish` succeeded, runs
  `cargo publish --workspace --locked` (stable cargo is >= 1.90) with the repository secret **`CARGO_REGISTRY_TOKEN`**; without the secret it fails with a clear
  message. The `report` job lists it. **The user must add the secret** (GitHub: Settings -> Secrets and variables -> Actions; token from crates.io/settings/tokens,
  account email verified), bump nothing (0.1.0 is the first version) and push the tag `v0.1.0`. Master pushes only build `__beta__`; they never touch crates.io.
  A published version cannot be deleted, only yanked. Users then run `cargo install hopswitch-cli` (the program is `hopswitch`).
* **Not verified:** the real upload (token, crates.io rules), and what `cargo publish --workspace` does when some crates are already published (a re-run of a
  half-failed publish): if it errors, bump to 0.1.1 instead. No dry-run job was added to CI on purpose: after the first publish, a dry run of the same version
  would fail on every master push until the version is bumped.
* Deliberately **not** done: `#[non_exhaustive]` on public enums (versions are 0.x, a breaking change is a 0.2.0).

## 07zn: one package (ADR-0013)

* **Why:** crates.io needs every dependency published; seven crates meant seven names. The user wants only `hopswitch` (and said nobody installs it soon, so it can be published now).
* **What changed:** every crate became a module (`git mv`, history kept); inside modules `crate::x` became `crate::<module>::x`, other crates `hopswitch_x::` became `crate::x::` (library) or
  `hopswitch::x::` (tests, examples, `main.rs`); tests were renamed `tests/<module>_<name>.rs` (two names collided: `proxy.rs`, `server.rs`); one root `Cargo.toml` (deps: serde, serde_json,
  thiserror, base64, getrandom; dev: tempfile pinned `=3.10.1`; `[lints]`; `[profile.release]`; deb/rpm metadata; `include` = src, tests, examples, README, LICENSE, manifests). No external dependency
  changed version in `Cargo.lock`. Workflows: no `-p`/`--workspace`; `cargo deb` / `cargo generate-rpm` / `cargo wix` run at the root; `pac-bench` paths are `src/core/pac_script.rs`, `src/core/rule_list.rs`,
  `examples/gen_pac.rs`; the `crates` job runs `cargo publish --locked`.
* **Checked:** 549 tests (545 + 4 architecture tests), fmt, clippy `-D warnings`, rustdoc `-D warnings` on Rust 1.75 **and** 1.99.1; the architecture tests were checked with seven deliberate violations
  (core uses storage / std::fs, http uses core, brace-form `use crate::{..}`, cli uses platform_windows, an unlisted new module, `libc` added) plus the alias case; `.deb` and `.rpm` built from the new manifest
  (same contents as before), the `.deb` installed with dpkg, run and removed; `cargo publish --dry-run --locked` packs 95 files, 133 KiB compressed, and verifies.
* **Found on the way:** the `cli` module never used `platform_windows` or `probe` (only `main.rs` wires them): the table is stricter than the old Cargo graph. `core` uses `std::net` types and `std::thread` (no file system,
  process, environment or sockets), so only those are forbidden.
* **Not verified:** the merged layout has not been seen on GitHub Actions (Windows, macOS, the musl build, `package.yml`, the `.msi` job with `cargo wix init` at the root); the real publish.
* **Names on crates.io:** only `hopswitch` is needed now. Its availability was not checked after this change (the seven `hopswitch-*` names are no longer used).

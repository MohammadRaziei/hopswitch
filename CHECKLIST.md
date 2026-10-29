# CHECKLIST

Legend: ✅ done · 🔄 in progress · ⬜ todo. A step is done only when its **Definition of Done** holds.

## Definition of Done (every step)

- [ ] Tests written first and seen failing (red), then passing (green)
- [ ] `cargo fmt --all --check` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo test` green (+ frontend gate once it exists)
- [ ] Docs / this checklist updated
- [ ] Conventional commit made, project zipped and delivered

## Roadmap

### ✅ Step 1 — Foundation & domain model (project named Hopswitch, ADR-0002)
- [x] Cargo workspace, lints (`unsafe_code = forbid`, clippy pedantic), rustfmt, `.gitignore`
- [x] `AGENT.md`, `CHECKLIST.md`, `README.md`, architecture doc, ADR-0001
- [x] `ProxyConfig` (scheme, host, port, bypass list; URL parse/display; hostname + IPv6 validation)
- [x] `DnsConfig` (1–4 servers, ordered, no duplicates/unspecified; serialises as array)
- [x] `Profile` (id, name ≤ 64 chars counted by character, proxy and/or DNS)
- [x] Deserialisation re-validates (no invalid state can be loaded from disk)
- [x] `managed_block`: safe, reversible edits of foreign files (marked block at top, byte-exact restore, refuses damaged markers) — ADR-0003

### ✅ Step 2 — System abstraction & services (ADR-0004, ADR-0005)
- [x] `Profile` split into independent `ProxyProfile` and `DnsProfile` (shared id/name validation, serde re-validation)
- [x] `SystemNetwork` port: `read_*` / `write_*` for proxy and DNS (apply and restore share one primitive); `SystemError`
- [x] `ProxyState` / `DnsState` model what was really there (`Disabled`/`Automatic`, `Manual`, opaque `Other { raw }`), serialisable
- [x] `Session` (original snapshot + active id) persisted through the `SessionStore` port **before** the first change
- [x] `ProxyService` / `DnsService` (one generic `NetworkService`): activate, switch A→B keeping the original, idempotent activate, deactivate (exact restore), `resume` after restart, `restore_all` for "restore on quit"
- [x] Failure handling per service: apply failure rolls back to the pre-call state; rollback failure and restore failure are reported distinctly and keep the session for a retry. No cross-kind rollback: proxy and DNS are independent by design
- [x] `testing::{InMemoryNetwork, InMemoryStore}` with failure injection and call counters
- [ ] (moved to Step 6) full copy of a file we will edit: lives in the Linux adapter's `Other { raw }` payload + `managed_block`

### ✅ Step 3b — Preset catalog (ADR-0006)
- [x] `presets` module in core: 6 DNS presets (Cloudflare, Google, Quad9, OpenDNS, AdGuard, Shecan) and 7 proxy presets (v2rayN SOCKS 10808 / HTTP 10809, Clash 7890, Clash Verge 7897, Shadowsocks 1080, Tor 9050, Tor Browser 9150); pure data, `to_profile(id)`, `is_saved_in(&Profiles)`
- [x] Tests: every preset validates, ids are unique slugs, documented values are pinned, proxy presets are loopback-only
- [x] Values checked against public sources (v2rayN ports, Clash/Mihomo/Clash Verge ports, Shecan free-plan IPs via third-party docs)
- [ ] Before release: re-verify against each provider's *official* docs (Shecan was only confirmed through third-party scripts); **Electro** was left out because no reliable source for its addresses was found

### ✅ Step 3 — Persistence (ADR-0007)
- [x] `Profiles` collection in core: unique ids per kind, insertion order, add / replace / remove, validating serde
- [x] `ProfileRepository` port + `JsonFileRepository` (new crate `hopswitch-storage`): versioned file, missing file = empty, corrupt / newer-version detection, explicit `quarantine_corrupt`
- [x] `write_atomic` (temp + fsync + rename + dir fsync), creates parent directories
- [x] Durable `JsonFileSessionStore` (+ `load` for resume); end-to-end "crash then restore original" test
- [x] Tests with temp directories

### 🟨 Step 4 — Windows adapter (ADR-0008)
- [x] `CommandRunner` port + `FakeRunner` in core; `ProcessRunner` (no console window on Windows)
- [x] Proxy: registry read/write, `WinINet` refresh, foreign states restored value for value, unsupported cases refused
- [x] 31 adapter tests with the fake runner (run on any OS) + `proxy_smoke` example for real-machine verification
- [ ] **Manual check on a real Windows machine** (`cargo run -p hopswitch-platform-windows --example proxy_smoke`): read, apply, restore, crash-restore
- [ ] DNS (needs admin) → implemented together with Step 10 (elevated helper)
### ⬜ Step 5 — macOS adapter (`networksetup`, per-service handling)
### ⬜ Step 6 — Linux adapter (GNOME `gsettings`, KDE, `resolvectl` / `nmcli`)
- [ ] Detect who owns DNS: `systemd-resolved` / NetworkManager → use their CLI; plain static file → managed block
- [ ] Never replace a symlinked `/etc/resolv.conf`; never run with a damaged-marker file
- [ ] Backup before first write; atomic write preserving mode/owner (fallback to in-place write when rename is impossible, e.g. container bind mounts)
- [ ] Warn/truncate when more than 3 DNS servers (glibc limit)
- [ ] Integration tests against a temp dir that mimics real `resolv.conf` shapes
### ⬜ Step 7 — Tauri shell
- [ ] Commands (`list_profiles`, `save_profile`, `delete_profile`, `activate`, `deactivate`, `status`) + error mapping
- [ ] Capabilities / permissions allow-list
- [ ] Tray icon + quick-switch menu, single-instance, autostart

### ✅ Step 7b — CLI (`crates/cli`)
- [x] `hopswitch proxy|dns use <name|preset>`, `off`, `list`, `add`, `remove`; `hopswitch off`, `status`, `presets`, `help` — same services and adapters as the app (no logic of its own)
- [x] Profiles load lazily (`off` works even if `profiles.json` is corrupt); damaged files are reported and never modified
- [x] 38 tests (parser, settings folder, end-to-end behaviour against an in-memory system + temp dir)
- [ ] Manual check on a real Windows machine: `hopswitch proxy use v2rayn-socks`, `hopswitch off`

### 🟨 Step 7d — Profile kinds, SwitchyOmega-style (ADR-0010)
- [x] Phase 1: `ProxyGroup` ("fastest of ...", members = plain proxies only, so no cycles), integrity in `Profiles` (members exist, one id namespace, a proxy in use by a group cannot be removed), schema 2 for `profiles.json` (version 1 still loads)
- [x] Auto connect is a plain flag + the *selected* profile (`proxy use` selects); a group follows its fastest member with hysteresis and failover; proxy `switch` / pool removed, DNS keeps its own; `auto.json` schema 2
- [x] A manual disconnect (`proxy off`, `dns off`, `off`) turns auto connect off for that kind and says so
- [x] CLI: `proxy add-group`, `proxy use <group>`, groups in `proxy list` / `ping` / `status` (`via <group>`)
- [x] Phase 2 spike (ADR-0011, `tools/pac-bench`): real lists measured; compact PAC = 649 KB for 127 k domains, 0 wrong answers in 174 723 hosts (V8), ~10 ms load
- [ ] Read the `PAC benchmark` workflow result (real WinHTTP engine) and apply the decision gate in ADR-0011
- [x] Phase 2, rule lists (pure core, independent of the PAC gate): `RuleList` imports plain and dotted lists, lower-cases, drops and counts invalid / duplicate / parent-covered entries, sorts, matches suffixes on label boundaries, and `add_suffix` replaces covered entries (one `ir` rule instead of 69 511). Real lists: 127 069 domains in 76 ms; FOD 620 lines -> 596
- [x] Phase 2 decision gate (ADR-0011) PASSED on Windows (WinHTTP): `bucket-ir` 649 466 B, 0 wrong of 535, first lookup 433 ms; object-per-domain variants fail with 0x2f87. ADR accepted
- [x] Phase 2, `ProxyState::Pac(PacConfig)` as a first-class state: `PacConfig` (http/https/file, no whitespace, <= 2048), Windows adapter reads a PAC as `Pac` only when no manual proxy is in use or stored (anything mixed stays `Other`), writes it with the manual proxy cleared first and the address last, never touches `AutoDetect`; restore exactness tests extended (every original x apply PAC x restore); CLI `status` shows `PAC <url>`
- [x] Phase 2, rules profiles in `Profiles` (core + storage): `Target` (`Direct` built in, or a saved proxy/group), `Rule`, `RulesProfile` (ordered rules, first match wins, default target, duplicate domain refused); one id namespace with proxies and groups; targets must exist; a proxy/group used by a rules profile cannot be removed; `profiles.json` schema 3 (v1 and v2 still load). Not yet: list bindings, CLI commands, applying a rules profile
- [x] Phase 2, PAC generator in Rust (`PacPlan`, `PacRoute`, `PacError`): user rules (ordered), then domain lists (compact bucket tables, a whole-TLD entry is `1`), then a default; plain host names DIRECT; ES3 only; hard limit 1 000 000 bytes (`TooLarge`), target 750 000; deterministic output. Tested in Node against the product's own matching code on 64 000 hosts (mutation-checked); real lists: Iran 649 575 bytes (Python spike: 649 448), 30 ms to generate, 327 790 lookups 0 wrong. CI now requires Node (`HOPSWITCH_REQUIRE_NODE`). Not yet verified on WinHTTP: the `PAC benchmark` workflow now also builds `rust-bucket-ir` and `rust-fod` with `gen_pac` and prints what WinHTTP returns for the SOCKS5 keywords
- [x] Phase 2, Rust PAC output verified on real WinHTTP (2026-10-26): `rust-bucket-ir` 649 574 B and `rust-fod` 9 778 B ok, 0 wrong of 535. FINDING: WinHTTP answers "no proxy" for `SOCKS`/`SOCKS4`/`SOCKS5` in a PAC (HTTP and HTTPS proxies work); `PacPlan::uses_socks()` added. DECISION NEEDED before wiring PAC to the Windows adapter (see HANDOFF section 10)
- [x] CI: every branch, concurrency, timeouts, `cargo doc` with warnings denied, and a `failure report` job in `ci.yml` and `pac-bench.yml` that writes `ci.md` / `pac-bench.md` to the `ci-report` branch (failed jobs with the log lines that explain them; the WinHTTP tables). Tested on fixtures and real cargo output; not yet seen running on GitHub
- [x] Code review against the BeautifulRust skill: `AutoMode` replaces the two-`bool` `AutoSettings::new`; `Debug` on every public type with `missing_debug_implementations` enforced; docs on all consts; `FromStr` for `PacConfig`. Open: `#[non_exhaustive]` before publishing
- [x] Decision: a Rules profile that needs a SOCKS proxy is refused on Windows (fail closed, ADR-0011); enforce it where the plan is built
- [x] Phase 2, shared HTTP server (`crates/http`, std only, 07za): loopback-only bind (anything else `InvalidInput`), `Host` check, 400/403/431, thread per connection (cap 32, slot returned even when a handler panics, 500 for the panic), header-injection filter, handler cannot set `Content-Length`/`Connection`; 19 socket tests, four deliberate breakages each caught. `pacserver` is now a thin handler on it (its 22 tests unchanged).
- [ ] Phase 2, `hopswitch serve` (ADR-0012 step b): token (>=128 bits, owner-only file), `Authorization: Bearer`, `Origin` check, status endpoint, `--ip` loopback only, `--port`, `--no-auto`; CLI wiring.
- [x] Phase 2, local PAC server (`crates/pacserver`, std only): loopback + free port, `Host` check against DNS rebinding, thread per connection (cap 32), 431/404/405/400/403, no-store, live `set_script`; 22 socket tests. Not yet: wiring to a Rules profile + SOCKS refusal, the running-process rule, crash cleanup
- [x] Crash safety groundwork: `NetworkService::activate_state` (apply a ready-made state with snapshot-first, rollback and original kept), crash/resume test with a PAC applied, `PacConfig::loopback` / `hopswitch_port()` to recognise Hopswitch's own PAC address. CORRECTION: the failure mode of a dead PAC address (outage vs silent direct) is unmeasured; the PAC benchmark now probes it
- [x] Stale PAC detection and cleanup: `Prober::probe_local_port`, `stale_hopswitch_pac`, CLI `status` / `off` / `proxy off` (restore the saved original; with none, switch a dead own address off; never touch a served or foreign PAC). MEASURED: WinHTTP returns 0x2f87 for a PAC address nothing listens on
- [x] ADR-0012 (headless mode, web interface, bare command, MCP) written; step (a) done: `--version`/`-V`, `decide_launch` + `main.rs` (bare command opens `hopswitch-gui` when there is a display and it is installed, else help + reason). Next: (b) `serve --ip --port` with token, (c) MCP, (d) Linux adapter (needs the user's answer), (e) `service install`
- [ ] Phase 2: per-domain rules via a generated PAC served from a local HTTP endpoint; needs a first-class PAC state in `ProxyState` (the Windows adapter reports PAC as "other" today), a `Direct` profile, rule lists + importer (plain and dotted formats), updater with snapshot, `THIRD_PARTY_NOTICES`
- [ ] Phase 3: per-app. Not decided; needs a local router (changes what the product is) or the cheap "launch a program with the proxy" alternative. Own ADR first.
- [ ] UI: profile editor for the kinds, the group picker, the auto connect checkbox next to the selected profile

### ✅ Step 4b — Windows restore fidelity (found by the CI smoke job)
- [x] `Disabled` = nothing switched on and nothing stored (writing it also removes `ProxyServer`, `ProxyOverride`, `AutoConfigURL`); a stored-but-switched-off address is `Other { enabled: false }` and restored exactly (ADR-0008 addendum)
- [x] Test: apply + restore returns 14 kinds of original registry to the same meaning; `status` says "off" for stored settings that are not in use
- [ ] Confirm on the real runner: `windows-proxy-smoke` now checks both a clean registry and a stored address (not run yet)

### ✅ Logo (`design/logo`)
- [x] App icon, small-size variant (16-32 px), monochrome tray mark, light/dark lockups, Tauri-named PNG/ICO exports, GitHub social preview; reproducible with `design/logo/build.py`
- [ ] Upload `export/social-preview.png` in the GitHub repo settings; copy the icons into `src-tauri/icons` when the Tauri shell exists

### ✅ CI
- [x] `.github/workflows/ci.yml`: fmt + clippy + tests on Linux / Windows / macOS, and `windows-proxy-smoke` (applies a preset through the CLI on a real Windows runner, reads the registry, restores)
- [x] First run on GitHub: the Windows smoke job proved the adapter on a real runner (apply wrote `ProxyEnable=1`, `ProxyServer=socks=127.0.0.1:10808`; `off` switched it off). Clippy failed on a lint newer than the local toolchain (`assigning_clones`): fixed; lint now runs in its own job so it cannot hide test results
- [x] Second run found two things: clippy 1.99 added `assert_is_empty` (fixed in all tests), and the Windows `hopswitch-probe` proxy tests assumed Linux timing (a refused loopback connection takes ~2 s on Windows; `thread::sleep` can wake early): loosened. The CI toolchain is now reproducible locally (Rust 1.99.1 from conda-forge; see HANDOFF)
- [x] `rust-version = "1.75"` declared and tested by the `msrv` CI job
- [ ] Third run: confirm `lint`, `test` (3 OS), `msrv` and `windows-proxy-smoke` are all green

### 🟨 Step 7c — Connectivity test & auto modes (ADR-0009)
- [x] `ConnectivitySettings` (probe URL / DNS domain / timeout / interval): validated, serde re-validates
- [x] `Autopilot` decision engine in core: auto connect, auto switch, both; failure streak + minimum-gain hysteresis; 21 tests
- [x] `Prober` port + `FakeProber` + parallel `probe_proxies` / `probe_dns_profiles` in core (5 tests)
- [x] `crates/probe` (`NetProber`, std only): DNS `A` query over UDP; HTTP through HTTP / SOCKS4a / SOCKS5 proxies; `https://` targets test the tunnel; 22 tests against loopback fake servers
- [x] CLI: `hopswitch ping [proxy|dns] [--url U] [--domain D] [--timeout MS]` (read-only, marks the active profile)
- [x] `AutoConfig` (per-kind connect/switch + ordered pool + test settings; **the proxy part became `connect` + `selected` in Step 7d, DNS kept switch + pool**), `AutoConfigStore` port, `JsonFileAutoStore` -> `auto.json` (versioned, atomic, damaged file reported and never touched)
- [x] `AutoRunner` in core: probe pool -> `Autopilot` -> apply through the normal services; proxy and DNS independent, a failure in one never blocks the other; off kinds send no traffic
- [x] CLI: `hopswitch auto status | set | proxy|dns connect|switch on|off | proxy|dns pool add|remove | run [--once]` (**proxy `switch` and `pool` were removed in Step 7d**); `ping` uses the saved test settings; removing a profile prunes it from its pool; `run` reopens services every round and reports a persistent problem once
- [ ] Background loop in the Tauri shell (call `AutoRunner::tick`, same as `auto run`); UI: latency badges per profile, Settings > Connectivity, Auto connect / Auto switch toggles per panel
- [ ] Manual check on a real Windows machine: `hopswitch auto run --once`, then `hopswitch auto run`

### ⬜ Step 8 — Design system & frontend scaffold
- [ ] Generate `design-system/MASTER.md` with ui-ux-pro-max (dark-first, desktop utility app)
- [ ] Run Impeccable critique/audit/harden on the real UI; add `npx impeccable detect` to CI
- [ ] Align outdated design boards (Editor, Empty, Rtl) with ADR-0004
- [ ] Vite + React + TS + Tailwind, Vitest + `mockIPC`, ESLint, i18n (fa / en), RTL

### ⬜ Step 9 — UI screens
- [ ] Two independent panels (Proxy, DNS), each with switch, active card, saved list
- [ ] Icon rail navigation; Settings and About pages (Safety: restore on quit, backup before first change)
- [ ] Profile editor (proxy URL field with inline validation, DNS chips, bypass list)
- [ ] Empty / loading / error states, toasts, keyboard shortcuts

### ⬜ Step 10 — Privilege helper for DNS changes
### ⬜ Step 11 — Packaging & CI (GitHub Actions matrix, signed installers, updater)

## Pre-delivery UI checklist (applies from Step 8)

- [ ] No emoji icons; SVG only
- [ ] Contrast ≥ 4.5:1; focus ring visible; full keyboard path
- [ ] `prefers-reduced-motion` respected
- [ ] 375 / 768 / 1024 / 1440 px verified; RTL verified
- [ ] Long profile names and hostnames wrap without clipping

## Before public launch

- [ ] Reserve GitHub org/user `hopswitch`
- [ ] Register domain (`hopswitch.app` / `.io` / `.com`)
- [ ] Trademark search (WIPO Global Brand Database, EUIPO)
- [ ] Reserve `hopswitch` on npm and crates.io

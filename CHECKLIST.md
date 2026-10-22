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

### ⬜ Step 3b — Preset catalog (ADR-0006)
- [ ] `presets` module in `core`: DNS and proxy presets as data; tests that every preset validates and ids are unique
- [ ] "already saved" detection (compare preset config with saved profiles)
- [ ] verify preset values against provider docs before release

### ⬜ Step 3 — Persistence
- [ ] `ProfileRepository` trait + JSON file implementation
- [ ] JSON-file `SessionStore` (the port already exists; must be durable: write + fsync + atomic rename)
- [ ] Atomic writes (temp file + rename), schema `version` field, corrupt-file recovery
- [ ] Tests with temp directories

### ⬜ Step 4 — Windows adapter (registry proxy + `InternetSetOption`, DNS via PowerShell/netsh)
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

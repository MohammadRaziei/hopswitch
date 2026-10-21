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

### ⬜ Step 2 — System abstraction & profile service
- [ ] `SystemNetwork` trait: read / apply / restore proxy and DNS
- [ ] `SystemSnapshot` captured before the first apply
- [ ] `ProfileService`: activate, deactivate (restore snapshot), switch A→B without losing the original snapshot
- [ ] Partial-failure handling (proxy applied, DNS failed → rollback)
- [ ] In-memory fake adapter used by the tests

### ⬜ Step 3 — Persistence
- [ ] `ProfileRepository` trait + JSON file implementation
- [ ] Atomic writes (temp file + rename), schema `version` field, corrupt-file recovery
- [ ] Tests with temp directories

### ⬜ Step 4 — Windows adapter (registry proxy + `InternetSetOption`, DNS via PowerShell/netsh)
### ⬜ Step 5 — macOS adapter (`networksetup`, per-service handling)
### ⬜ Step 6 — Linux adapter (GNOME `gsettings`, KDE, `resolvectl` / `nmcli`)
### ⬜ Step 7 — Tauri shell
- [ ] Commands (`list_profiles`, `save_profile`, `delete_profile`, `activate`, `deactivate`, `status`) + error mapping
- [ ] Capabilities / permissions allow-list
- [ ] Tray icon + quick-switch menu, single-instance, autostart

### ⬜ Step 8 — Design system & frontend scaffold
- [ ] Generate `design-system/MASTER.md` with ui-ux-pro-max (dark-first, desktop utility app)
- [ ] Vite + React + TS + Tailwind, Vitest + `mockIPC`, ESLint, i18n (fa / en), RTL

### ⬜ Step 9 — UI screens
- [ ] Profile list with big on/off switch and live status
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

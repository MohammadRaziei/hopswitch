# HANDOFF — Hopswitch (state at 2026-10-24, Steps 7b + 7c done except the Tauri loop + UI; 304 tests green)

> **Paste-in starter for the new chat (the user writes Persian, informal):**
> «سلام! پروژه‌ی Hopswitch رو از چت قبلی ادامه می‌دیم. zip پروژه و HANDOFF.md پیوست شده.
> اول `HANDOFF.md`, `AGENT.md`, `CHECKLIST.md` و `docs/adr/*` رو بخون، بعد `crates/cli/src/app.rs` رو با TDD تموم کنیم (بخش ۵).»
> Reply in Persian (casual, friendly); code, comments and docs stay in English.

## 1. What this project is

**Hopswitch**: cross-platform (Windows / macOS / Linux) desktop **client** that switches the **system proxy**
and **system DNS** between saved entries, from a modern UI and a tray menu. It runs/ships no proxy server;
it only points the OS at one the user already has (think `all_proxy`, but system-wide, with on/off + restore).
Stack: **Tauri 2 (Rust)** + React/TypeScript/Tailwind frontend. Chosen over Flutter (ADR-0001).

## 2. How the user wants to work (important)

- Step by step, together, **strict TDD**: write failing tests first, *run them and see red*, then implement, then green, then refactor.
- Clean, professional, idiomatic code; clean folder structure; `AGENT.md` and `CHECKLIST.md` live in the repo.
- **After every step or commit: zip the whole project and send it** (`present_files`). Exclude `target/`. Name like `hopswitch-step-NN.zip`; delete the previous zip.
- Conventional Commits. One checklist step ≈ one commit.
- Wants a *modern, polished UI with good UX*; will give design feedback later himself. Delegates small design choices ("whatever is better") but expects a stated reason.
- **His main fear:** on Linux, touching `/etc/resolv.conf` could wipe the user's existing entries. Already addressed by `managed_block` + ADR-0003. Keep honouring it (see AGENT.md rule 7).

## 3. Repository state

Location in the old sandbox: `/home/claude/hopswitch` (sandbox resets; **the zip is the source of truth**, `.git` is inside it).

```
hopswitch/
├── AGENT.md  CHECKLIST.md  README.md  HANDOFF.md
├── Cargo.toml            # workspace; lints: unsafe_code=forbid, missing_docs=warn, clippy pedantic
├── rustfmt.toml          # max_width = 100
├── crates/core/          # package `hopswitch-core`, pure Rust, no Tauri/OS imports
│   ├── src/{lib,error,proxy,dns,profile,profiles,network,session,service,repository,testing,managed_block}.rs
│   └── tests/*.rs        # public-API tests
├── crates/storage/       # package `hopswitch-storage`: write_atomic, JsonFileRepository, JsonFileSessionStore
│   └── tests/{atomic,repository,session_store}.rs   # temp-dir tests
├── crates/platform-windows/  # package `hopswitch-platform-windows`: WindowsNetwork (proxy), ProcessRunner
│   ├── src/{lib,registry,runner,network}.rs   # registry = pure parse/plan/render
│   ├── tests/proxy.rs                         # 31 tests via FakeRunner
│   └── examples/proxy_smoke.rs                # real-machine check (read / apply / restore)
├── crates/cli/           # package `hopswitch-cli`, binary `hopswitch` — WORK IN PROGRESS, NOT A WORKSPACE MEMBER (see §5)
├── design/               # README + static UI mockups (design/preview/*.dc.html, canvas.json)
└── docs/{ARCHITECTURE.md, adr/0001..0004}
```

Git history: domain model → rename → `managed_block` → UI direction → handoff → Steps 2, 3, 3b, 4. HEAD = `7a75f7d feat(platform-windows): ...`. **Uncommitted:** `crates/cli/` (new, unfinished) and the `Cargo.toml`/`Cargo.lock` changes.

**Tests: 165 passing at the last green gate** (core 104, storage 30, platform-windows 31), run *before* the CLI crate was added. The CLI crate has never been compiled. Quality gate (must pass before every commit):

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### Public API of `hopswitch-core` today
- `ProxyScheme {Http,Https,Socks4,Socks5}` (FromStr case-insensitive, Display).
- `ProxyConfig::new(scheme, host, port)`, `.with_bypass(Vec<String>)`, getters, `FromStr`/`Display` for `scheme://host:port` (IPv6 as `[::1]`), hostname lower-casing/validation. `serde` deserialisation re-validates (`try_from` DTO).
- `DnsConfig::new(Vec<IpAddr>)` / `from_strs(...)` / `servers()`; 1–`MAX_DNS_SERVERS`(=4) servers, ordered, no duplicates, no unspecified addr; serialises as a plain JSON array.
- `ProxyProfile` / `DnsProfile` (Step 2; the bundled `Profile` is gone). Name ≤ `MAX_NAME_CHARS`(=64) counted in *chars* (Persian OK).
- `ValidationError` (thiserror enum) for all of the above.
- `managed_block::{apply_block, remove_block, contains_block, BlockError, BEGIN_MARKER, END_MARKER}`: pure string transforms. Block goes at the **top** of the file; everything else byte-for-byte; `remove(apply(x)) == x`; refuses unterminated/stray/duplicate markers and lines that could break out.

## 4. Decisions already made (read the ADRs)

| ADR | Decision |
|---|---|
| 0001 | Tauri 2 over Flutter |
| 0002 | Name **Hopswitch** (crate prefix `hopswitch-*`). Names checked free on npm/crates.io/PyPI. Rejected: Waypoint, Switchyard, Lane, Routa, Pol, Rah |
| 0003 | Foreign system files are edited only via a marked block; prefer `resolvectl`/`nmcli` when systemd-resolved/NetworkManager owns DNS; never replace a symlinked resolv.conf; backup before first write; atomic write; glibc reads only the first **3** nameservers (warn/truncate in the Linux adapter) |
| 0004 | UI: **two independent panels (Proxy | DNS) side by side**, slim 64px icon rail (Home / Settings / About), tray mirrors the panels. Consequence: `Profile` splits into independent `ProxyProfile` and `DnsProfile` |

Architecture rules (AGENT.md): dependencies point inward (`src-tauri → platform-* → core`); OS access only behind a trait in `core`; parse-don't-validate; **always snapshot before applying** and restore exactly on off; no `unwrap/expect/panic` outside tests; least privilege (no running the whole app elevated).

## 5. NEXT: CLI is finished (Step 7b)

`crates/cli` is a workspace member again; `app.rs` is written and all 203 tests pass (core 104, storage 30,
platform-windows 31, cli 38 -> now 264 in total incl. probe 22; see Step 7c); fmt + clippy (`-D warnings`) clean.
Behaviour notes: DNS `Automatic` shows as `automatic (from the network)` in `status`; `off` with nothing active is a no-op without writes.

**In this order of value:**
1. **Ask the user to run the Windows smoke test** (`cargo run -p hopswitch-platform-windows --example proxy_smoke`, then `-- apply v2rayn-socks`) and report the output: the Windows adapter has only ever been tested with fakes. Once the CLI works, `hopswitch proxy use v2rayn-socks` / `hopswitch off` on his machine is the best real test.
2. Step 5 (macOS) / Step 6 (Linux) adapters, or Step 7 (Tauri shell), depending on what the user wants. **The OS was assumed to be Windows; the user never confirmed.**
3. Windows DNS needs administrator rights: implemented together with Step 10 (elevated helper). Until then `WindowsNetwork` DNS calls return `Unsupported`.
4. Before release: re-verify preset values against official docs (Shecan was confirmed only through third-party scripts; **Electro** was left out for lack of a reliable source). Revisit the `tempfile = "=3.10.1"` pin when the minimum Rust version is decided.

TDD as always: red → green → refactor → gate → checklist → commit → zip.

## 6. UI design preview

Open `design/preview/index.html` (standalone page, all current boards; see `design/README.md`). The older canvas files are mirrored in `design/preview/*.dc.html`. Boards: **Main** (two panels + rail), **Tray**, **Settings** (General + Safety: restore on quit, backup before first change, update check, no telemetry of his proxies/DNS), **About**. **Editor, Empty, Rtl are outdated** (old bundled-profile model, no rail); align them with ADR-0004 when asked. `Artboard-be99` is an empty board the *user* added: leave it alone.
Look: dark first, bg `#0F1115`, surface `#171A21`, border `#262B36`, muted `#9AA3B2`, accent `#34D399`; light/RTL palette in `Rtl` board; Vazirmatn for Persian. RTL + Persian must be first-class (logical CSS properties; keep IPs LTR via `dir="ltr"`).
To change the canvas use the Artifact tool with that URL (re-read before overwriting; the user edits it too).

## 7. Skills / references evaluated

- **ui-ux-pro-max** (nextlevelbuilder/ui-ux-pro-max-skill): plan to generate `design-system/MASTER.md` in Step 8 and use its pre-delivery checklist (already copied into AGENT.md). Re-read its README for the exact CLI.
- **Impeccable** (pbakaus/impeccable): *yes, selectively*: `critique/audit/polish/harden/onboard`, and `npx impeccable detect` as a CI gate on the frontend. Known tells in our mockups to fix in the real UI: Inter as sole font, nested cards, rounded icon tiles beside headings. It has anonymous telemetry (off via `DO_NOT_TRACK=1`) and installs hooks that download a binary; the user should review before installing locally.
- **taste-skill** (leonxlnx): *not now*; built for marketing/landing pages with real photos. Use later for a Hopswitch download website.
- Tauri 2 skills/docs: `skills.sh` entries (gihwan-dev/claude-code-gui `tauri-v2`, openaec-foundation `tauri-2-claude-skill-package`) for IPC/permissions review; Tauri docs "Mock Tauri APIs" (`mockIPC`) for Vitest.

## 8. Environment notes & gotchas (old sandbox; may differ)

- No rustup. Installed via apt as root: `apt-get install -y cargo rustc rustfmt rust-clippy` → **Rust 1.75**. Hence `thiserror = "1"` (2.x needs ≥ 1.77). **Tauri 2 needs Rust ≥ 1.77 + webkit/GTK libs**, and a GUI can't run in the sandbox: Tauri/UI code can't be executed there, so write it carefully and ask the user to run `npm run tauri dev` locally and paste errors. Keep logic in `core`/`platform-*` where tests can actually run.
- `bash_tool` runs `sh`: use `bash script.sh` for `${PIPESTATUS}`/`${var:0:2}`. `npm` prints a ".npmrc prefix" error in the sandbox. `cargo test` stops at the first failing test binary (use `--no-fail-fast` for the full picture).
- Network allow-list includes npm, crates.io, PyPI, github.com; `api.github.com/users/...` returned 403.
- Clippy pedantic bit us on: `needless_borrows_for_generic_args`, `single_match_else`, `doc_markdown` (wrap `Hopswitch`-style names in backticks).
- Windows/macOS adapters can't be tested in the sandbox either: isolate command construction/parsing into pure functions and unit-test those.

## 9. Open items for the user (not blocking)

- Reserve before public launch: GitHub org `hopswitch`, domain (`hopswitch.app/.io/.com`), trademark search (WIPO/EUIPO), npm + crates.io names.
- He has not explicitly confirmed the two-panel + rail + Settings/About direction beyond "whatever is better"; he said he will give design feedback himself later.
- Possible later feature: one-click "combos" that apply a proxy and a DNS together; TUN/VPN-style capture for apps that ignore the system proxy (explicitly out of scope for now).

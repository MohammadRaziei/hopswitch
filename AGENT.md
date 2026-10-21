# AGENT.md — working agreement for humans and AI agents

Hopswitch is a cross-platform **client** (Windows / macOS / Linux) that switches the
**system proxy** and **system DNS** between saved profiles, from a modern UI and a
system-tray menu. It does not run or ship any proxy server.

## Stack

| Layer            | Choice                                              |
|------------------|-----------------------------------------------------|
| Shell            | Tauri 2 (Rust)                                      |
| Core logic       | `crates/core` — pure Rust, **no Tauri / OS imports** |
| OS adapters      | `crates/platform-*` (added in later steps)          |
| Frontend         | React + TypeScript + Tailwind + Vitest              |
| Design source    | `design-system/MASTER.md` (generated with ui-ux-pro-max) |

## Architecture rules

1. **Dependencies point inward.** `src-tauri` → `platform-*` → `core`. `core` depends on nothing OS-specific.
2. **OS access only behind a trait** defined in `core` (`SystemNetwork`, introduced in Step 2). Services are tested with in-memory fakes; real adapters get thin integration tests.
3. **Parse, don't validate.** Domain types (`ProxyConfig`, `DnsConfig`, `Profile`) can only be constructed valid, including via `serde` (`try_from`). Never add a public constructor or `pub` field that bypasses validation.
4. **Always snapshot before applying.** Turning a profile off must restore the exact previous system state.
5. **No `unwrap`/`expect`/`panic!` in non-test code.** Return typed errors (`thiserror`).
6. **Least privilege.** Tauri capabilities are allow-listed per command; DNS changes that need admin rights go through a dedicated helper, never by running the whole app elevated.

## TDD workflow (mandatory)

For every unit of behaviour:

1. 🔴 **Red** — write a failing test through the public API; run it and confirm it fails *for the right reason*.
2. 🟢 **Green** — write the smallest implementation that passes.
3. 🔵 **Refactor** — clean up with tests green.
4. Run the full gate (below), tick the item in `CHECKLIST.md`, then commit.

Test names describe behaviour (`rejects_port_zero`), not implementation.
Prefer tests in `crates/*/tests/` (public API). Frontend tests use Vitest + `mockIPC` from `@tauri-apps/api/mocks`.

## Quality gate (must pass before every commit)

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
# from Step 8 onward, also:
npm run lint && npm run typecheck && npm test
```

## Commits

[Conventional Commits](https://www.conventionalcommits.org): `feat(core): …`, `fix(platform-windows): …`, `test: …`, `docs: …`, `chore: …`.
One step of `CHECKLIST.md` = one commit (or a small series). After each step, the whole project is zipped and delivered.

## Folder layout

```
hopswitch/
├── AGENT.md  CHECKLIST.md  README.md
├── Cargo.toml                 # workspace
├── crates/
│   ├── core/                  # domain + services (pure Rust)
│   └── platform-*/            # OS adapters            (later)
├── src-tauri/                 # Tauri shell, commands, tray (later)
├── src/                       # React frontend              (later)
├── design-system/             # MASTER.md + page overrides  (later)
└── docs/
    ├── ARCHITECTURE.md
    └── adr/                   # architecture decision records
```

## UI / UX rules (from the ui-ux-pro-max checklist)

- One source of truth: `design-system/MASTER.md`; page files only override deviations.
- SVG icons (Lucide), never emojis as icons. `cursor-pointer` on clickable elements.
- Text contrast ≥ 4.5:1, visible focus states, full keyboard navigation.
- Respect `prefers-reduced-motion`; micro-interactions 150–300 ms.
- Colour is never the only carrier of meaning (status = colour **and** icon/text).
- Layout verified at 375 / 768 / 1024 / 1440 px; long labels wrap instead of clipping.
- Support RTL (Persian) and LTR from day one — use logical CSS properties (`ms-*`, `ps-*`).

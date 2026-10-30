# ADR-0013: One package, several modules (replaces the crate split of ADR-0007 and ADR-0008)

* **Status:** accepted 2026-10-30, at the user's request
* **Context:** crates.io only accepts a crate whose dependencies are published too. With seven
  workspace crates, publishing the program meant publishing seven names (`hopswitch-core`,
  `hopswitch-storage`, ...) that nobody should depend on, and keeping seven versions in step. The
  user wants one name: `hopswitch`.
* **Decisions:**
  1. **One package `hopswitch`** with a library and the binary of the same name. The former crates are
     modules of the library: `core`, `storage`, `http`, `probe`, `platform_windows`, `pacserver`,
     `cli` (`src/<module>/mod.rs`, or `src/<module>.rs` for the two that are one file). `src/main.rs`
     is the composition root: it gives the CLI the Windows adapter and the prober.
  2. **The layering stays, enforced by `tests/architecture.rs` instead of by Cargo.** A table lists
     what each module may use: `core` nothing; `storage`, `probe`, `platform_windows` only `core`;
     `http` nothing; `pacserver` `core` and `http`; `cli` `core`, `storage` and `http`. (That is the
     old dependency graph, and slightly stricter: `cli` never used `platform_windows` or `probe`,
     only `main.rs` does, through ports.) `core` may not use `std::fs`, `std::process`, `std::env`,
     `std::os`, `std::io`, real sockets or an operating-system crate, and the package may not gain an
     operating-system dependency without this test being changed on purpose. A new module must be
     added to the table, which is the moment to decide where it belongs.
  3. **What ADR-0007 and ADR-0008 decided still holds, with these words changed:** "own crate"
     means "own module" (`storage`), and `crates/platform-*` means `src/platform_*` (a module per
     operating system, compiled only where it is needed once an adapter needs OS-only code).
     Tests of module `m` live in `tests/<m>_<name>.rs`.
  4. **The frontend will not live in `src/`.** That is where the Rust code is now. The React app
     goes to `ui/`; a Tauri shell, when it comes, is `src-tauri/`, a workspace member with
     `publish = false` that depends on `hopswitch` by path and version.
  5. **Publishing:** one `cargo publish`. The package `include`s only `src`, `tests`, `examples`,
     the README, the licence and the manifests.
* **Alternatives rejected:** publishing all seven crates (what many projects do; it works, but
  it puts six names on crates.io that are not meant for anyone else); keeping the workspace and
  building one flat package only at publish time (what is published would not be what was
  tested); a placeholder crate that only reserves the name (`cargo install` would not work).
* **Consequences:** the compiler no longer stops a module from importing another; the architecture
  tests do, and they were checked by seven deliberate violations. Everything is compiled as one unit,
  so a program that wants only the domain still builds the whole library; its dependencies are few
  (`serde`, `serde_json`, `thiserror`, `base64`, `getrandom`). If that ever matters, a module is
  split into a crate again by moving its folder: the table in `tests/architecture.rs` is the list of
  boundaries.

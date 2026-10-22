# ADR-0007: Persistence lives in its own crate; core only defines ports

* **Status:** accepted
* **Decisions:**
  1. **`core` stays free of file I/O.** It defines `Profiles` (validated collection: unique ids per
     kind, insertion order) and the `ProfileRepository` / `SessionStore` ports. File code lives in
     `crates/storage` (`hopswitch-storage`).
  2. **Atomic, durable writes.** `write_atomic` = write sibling temp file → `fsync` → rename over the
     target → `fsync` the directory (Unix). A crash leaves the whole old or whole new file. One writer
     per path is assumed (the app will be single-instance).
  3. **Versioned files.** `{ "version": 1, ... }`. A *newer* version is refused
     (`UnsupportedVersion`), never treated as corrupt and never overwritten.
  4. **Corruption is reported, not silently fixed.** `load` never modifies a bad file.
     `JsonFileRepository::quarantine_corrupt` moves it to `<name>.corrupt[-N]` (nothing is deleted); the
     app decides when to call it. A corrupt *session* file is an error and is never removed: it may
     hold the only copy of the user's original settings.
  5. **Missing file = first run** (empty profiles / no session).
* **Consequences:** two session files are used (one per kind: proxy, DNS). `tempfile` is pinned to
  3.10.1 for old toolchains; revisit when the minimum Rust version is decided.

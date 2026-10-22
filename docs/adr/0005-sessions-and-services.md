# ADR-0005: Sessions, services and the `SystemNetwork` port

* **Status:** accepted
* **Decisions:**
  1. **One write primitive.** `SystemNetwork` has `read_*` and `write_*` per kind. "Apply" and
     "restore" are both `write_*`, so a restore exercises exactly the code that applying does.
  2. **States describe reality.** `ProxyState`/`DnsState` include `Other { raw }` for settings we
     cannot model (PAC scripts, extra servers, ...). `raw` is opaque to core and written back by the
     same adapter, so a snapshot is always restorable exactly.
  3. **Persist before applying.** The first activation saves a `Session { original, active_id }` via
     the `SessionStore` port and only then changes the system. If saving fails, nothing is changed.
  4. **Rollback target = state read just before the call**, not "the original". A failed switch A→B
     therefore leaves A active instead of silently turning everything off.
  5. **Failures are explicit.** Read, apply (rolled back), rollback-also-failed and restore errors are
     distinct `ServiceError` variants. A failed restore keeps the session so it can be retried.
  6. **Proxy and DNS never share state or rollback.** One generic `NetworkService<Kind>` implements the
     rules once; `ProxyService` and `DnsService` are aliases.
* **Consequences:** test doubles (`testing::InMemoryNetwork`, `InMemoryStore`) ship in core so
  adapters and the Tauri shell can reuse them. Step 3 only has to implement a durable `SessionStore`.

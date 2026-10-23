# Architecture

```
┌──────────────────────────────────────────────────────────┐
│ Frontend (React/TS)  ── invoke() / events ──┐            │
└─────────────────────────────────────────────┼────────────┘
                                              ▼
┌──────────────────────────────────────────────────────────┐
│ src-tauri  — commands, tray, state, error mapping        │
└───────────────┬──────────────────────────────────────────┘
                ▼
┌──────────────────────────┐     ┌─────────────────────────┐
│ crates/core              │◀────│ crates/platform-*       │
│  Profile / ProxyConfig / │     │  implement SystemNetwork│
│  DnsConfig, services,    │     │  for Windows/macOS/Linux│
│  traits (ports)          │     └─────────────────────────┘
└──────────────────────────┘
```

* **core** is a hexagonal "domain + ports" crate. It never touches the OS.
* **storage** implements the persistence ports (profiles, sessions) with atomic, versioned JSON files (ADR-0007).
* **platform-\*** crates are adapters; each is small and integration-tested on its own OS in CI.
  The Windows adapter (`platform-windows`) is split into pure registry logic and a thin process runner (ADR-0008).
* **src-tauri** is a thin composition root: wires adapters into services and exposes commands.

## Key flows

Proxy and DNS each have their own service, snapshot and active entry (ADR-0004, ADR-0005).

**Activate** → read current state → if nothing of ours is active: persist a `Session` holding that
state → write the profile's state. On failure, write the pre-call state back and return a typed error
(a failed *first* activation also drops the session).

**Switch A→B** → same call; the original snapshot is kept, only the active id changes.

**Deactivate** → write the original snapshot back → clear the session. If the write fails the session
is kept so the user can retry.

**Quit** → `restore_all` deactivates both services, attempting both even if one fails.

## Out of scope (for now)

Transparent/TUN capture of traffic from apps that ignore system proxy settings.
System proxy settings only affect applications that honour them.

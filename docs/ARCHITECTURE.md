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
* **platform-\*** crates are adapters; each is small and integration-tested on its own OS in CI.
* **src-tauri** is a thin composition root: wires adapters into services and exposes commands.

## Key flows (target design)

**Activate profile** → capture `SystemSnapshot` (only if nothing active) → apply proxy → apply DNS →
on any failure, roll back to the snapshot and surface a typed error.

**Deactivate** → restore the snapshot exactly → clear "active profile".

## Out of scope (for now)

Transparent/TUN capture of traffic from apps that ignore system proxy settings.
System proxy settings only affect applications that honour them.

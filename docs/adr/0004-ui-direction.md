# ADR-0004: UI direction — two independent panels, slim icon rail

* **Status:** accepted (design); domain change lands in Step 2
* **Decision:**
  1. The main screen has **two side-by-side panels, Proxy and DNS**. Each has its own on/off switch,
     its own saved entries, and exactly one active entry. Side-by-side (not tabs) so the state of both
     is visible at once.
  2. Navigation is a **slim icon rail** (64 px): Home, Settings, About. No wide sidebar.
  3. The tray menu mirrors the panels: a PROXY section and a DNS section, each with an "Off" entry.
  4. Settings includes a **Safety** section: restore original settings on quit (default on), keep a
     backup before the first change, update check (no telemetry about the user's proxies or DNS).
* **Consequence for the domain model:** `Profile` (proxy + DNS bundled) is replaced by two independent
  aggregates, `ProxyProfile` and `DnsProfile`, activated, snapshotted and rolled back **separately**
  (turning DNS off must never touch the proxy). `ProxyConfig` and `DnsConfig` stay as they are.
  One-click "combos" (apply a proxy and a DNS together) can be layered on later.
* **Tooling notes:** apply Impeccable (`critique`, `audit`, `harden`, `onboard`) to the final UI and run
  `npx impeccable detect` in CI; known tells to fix in the real UI: Inter as the only font, nested cards,
  rounded icon tiles beside headings. Taste-skill is reserved for the future marketing/download site.

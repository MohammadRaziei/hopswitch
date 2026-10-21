# ADR-0002: Project name — Hopswitch

* **Status:** accepted
* **Context:** The name must fit a tool that hops system traffic between proxy/DNS profiles, and
  must not collide with existing network software or package names.
* **Decision:** **Hopswitch** (crate prefix `hopswitch-*`, binary `hopswitch`).
* **Verified (2026-10-21):** no product with this name found in web search; the names
  `hopswitch` are unregistered on npm, crates.io and PyPI.
* **Rejected:** Waypoint (HashiCorp Waypoint, Istio waypoint proxy), Switchyard (Python network
  framework, NVIDIA Rust proxy), Routa (existing Tauri project), Lane / Pol / Rah (too generic or taken).
* **Still to verify before public launch:** GitHub org/user name, domain (`hopswitch.app` / `.io` / `.com`),
  trademark databases (WIPO, EUIPO), Microsoft Store / Mac App Store.

# ADR-0006: Presets and manual entry when adding a proxy or DNS

* **Status:** accepted (design); implementation in CHECKLIST Step 3b
* **Decision:** "Add DNS" and "Add proxy" open a dialog with two tabs: **Presets** and **Manual**.
  * Presets are a built-in, read-only catalog (name, addresses/host+port, short tag). Selecting one
    and pressing Add saves a normal `DnsProfile`/`ProxyProfile`; "Customize first" opens the Manual tab
    pre-filled. Presets already saved are shown disabled as "Saved".
  * Presets are *templates*, not live links: once added, the profile is the user's own and editable.
  * Proxy presets are the default local ports of common clients (v2rayN SOCKS 10808 / HTTP 10809,
    Clash 7890, Shadowsocks 1080, Tor 9050 / Tor Browser 9150). The dialog warns the app must be running.
* **Consequences:** the catalog lives in `core` as pure data with tests (every preset must build a valid
  `ProxyConfig`/`DnsConfig`; ids unique). Catalog values must be re-verified against each provider's
  documentation before release.

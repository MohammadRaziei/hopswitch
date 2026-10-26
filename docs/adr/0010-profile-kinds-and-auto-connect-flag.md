# ADR-0010: Proxy profile kinds (SwitchyOmega-style) and auto connect as a plain flag

* **Status:** accepted (phase 1 implemented; phases 2-3 planned)
* **Context:** the proxy section should work like SwitchyOmega: a *profile* is not always one proxy.
  Wanted kinds: a plain proxy; per-domain rules; per-app rules; and a smart profile that picks the
  fastest of several others. Auto connect was an open question (a profile with options, or a flag).

## Decisions

1. **A proxy profile is one of:** `Proxy` (exists today), `Group` (smart switch, phase 1),
   `Rules` (per-domain, phase 2), `Apps` (per-process, phase 3, see 5). UI names: "Proxy",
   "Fastest of..." (a group with policy `fastest`), "Domain rules", "Apps". `Group` carries a *policy*
   (only `fastest` for now) so `failover` can be added without changing the shape.
2. **Phase 1 keeps `ProxyProfile` untouched** and adds `ProxyGroup` as its own aggregate in `Profiles`
   (like ADR-0004: independent aggregates). Proxies and groups share one id namespace, so
   `proxy use <key>` is unambiguous.
3. **A group's members are plain proxies only**, no groups inside groups: cycles cannot be represented.
   Members must exist; a proxy used by a group cannot be removed (the error names the groups);
   loading a file that breaks these rules is refused like any other corrupt file.
4. **Group profiles are not applied as such.** The system only ever has one real proxy, so applying
   a group means "apply its fastest live member now". The group stays the *selected* profile.
5. **Per-app is not decided here.** The system proxy (and PAC) cannot see processes; only a local
   router that Hopswitch itself runs can map a connection to its process, and that changes what the
   product is (README: "does not provide or run" a proxy) and adds a failure mode (a dead local port
   cuts the internet). It needs its own ADR. Cheap alternative for later: "launch this program with the
   proxy" (environment / `--proxy-server`) without any router.
6. **Per-domain rules (phase 2)** are done with a generated PAC script served from a tiny local HTTP
   endpoint (static text, no traffic goes through it). `ProxyState` then needs a first-class PAC state
   (today the Windows adapter reports PAC as "other"). A `Direct` profile arrives with rules.
7. **Auto connect is a plain flag, not a profile.** A profile says *what* to connect to; the flag says
   *whether to keep the system connected*. The settings keep `selected` (the profile to connect, set by
   `proxy use`) next to `connect`. Reconnecting after an automatic disconnect needs to know the target,
   and `selected` survives disconnects, unlike the applied profile.
8. **Automation follows the selected profile.** Selected = plain proxy: connect when it answers,
   disconnect (restore the original settings) after repeated failures. Selected = group: connect to the
   fastest live member, move to a clearly faster one, fail over, disconnect when none answers. This is
   exactly the old "auto connect + auto switch" pair, so the proxy `switch` option and the pool are
   gone; DNS keeps its own options and pool for now.
9. **A manual disconnect wins over automation.** `hopswitch proxy off` / `off` while auto connect is on
   turns auto connect off for that kind and says so; otherwise a running `auto run` would undo the
   user's click within seconds. `auto proxy connect on` turns it back on.
11. **Rules profiles (implemented, step 07q).** `Direct` is a built-in *target*, not a saved profile
    (`proxy off` stays the global "no proxy"). A `RulesProfile` shares the id namespace with proxies and
    groups, but it is not a `ProxyEntry`: it cannot be a target or a group member, and the automation
    does not select it yet. Targets (rule targets and the default) must be saved proxies or groups; a
    proxy or group they use cannot be removed. `profiles.json` is schema 3 (adds `rules`).
10. **Persistence.** `profiles.json` goes to schema 2 (adds `groups`); version 1 files still load.
    An older Hopswitch refuses a version-2 file instead of silently dropping the groups on save.
    `auto.json` goes to schema 2 (`proxy` = `{ connect, selected }`); a version-1 `proxy.pool` /
    `proxy.switch` is ignored on load (the project is unreleased: no migration is written).

## Consequences

* Smart switching, auto connect and failover for the proxy are one mechanism (`Autopilot`), now
  driven by the selected profile instead of a separate pool.
* The CLI gains `proxy add-group`; `proxy use` accepts a group; `auto proxy switch|pool` are removed.
* Phases 2 and 3 are not promised by this ADR; they need the `ProxyState` PAC change and a decision
  on the local router.

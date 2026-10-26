# ADR-0011: Domain rules, public rule lists and how they reach the system

* **Status:** accepted (2026-10-26). The decision gate (section 10) passed on a real Windows runner:
  the `PAC benchmark` workflow ran the variants through WinHTTP (`WinHttpGetProxyForUrl`, 535 sample hosts
  per variant). `bucket-ir` (649 466 bytes) loaded, gave 0 wrong answers, first lookup (download + compile)
  432.9 ms, mean lookup 250.4 us. `fod` (10 728 bytes): 22.3 ms, 0 wrong. The two object-per-domain variants
  (2 276 542 and 1 074 053 bytes) both **failed** with `ERROR_WINHTTP_BAD_AUTO_PROXY_SCRIPT` (0x2f87 = 12167),
  so the compact bucket format is required, not just a size optimisation. Not measured: other PAC clients
  (Chromium, Firefox, macOS, Linux) and sample sizes beyond 535 hosts.
* **Context:** ADR-0010 phase 2 = a `Rules` profile ("domain rules"). For Iranian users the main use is
  "local sites go direct, everything else through my proxy" (a foreign exit IP breaks banks and local
  services), and the reverse for developer services that refuse Iranian IPs. Two public lists fit:
  * `bootmortis/iran-hosted-domains` (MIT): domains hosted in Iran, rebuilt by CI every one to two
    weeks, release assets in several formats with `.sha256` files, plus categories (`all`, `ads`,
    `proxy`, ...).
  * `freedomofdevelopers/fod` `domains`: services that block Iran, one `.example.com` per line.
    Entries were contributed publicly through issues; the README points to CC BY-SA 3.0.

## Measured (tools/pac-bench, real lists downloaded 2026-10-24)

| | |
| --- | --- |
| Iran list | 127 069 domains, no duplicates, none invalid. 69 512 (55 %) end in `.ir`, 57 557 do not (48 910 are `.com`) |
| FOD list | 620 lines, 605 distinct (15 duplicates), none invalid |
| PAC, one object key per domain, whole list | 2 276 532 bytes |
| same, but `.ir` as one rule | 1 074 041 bytes |
| **compact buckets** + `.ir` rule | **649 448 bytes** (254 KB gzipped) |
| FOD as PAC | 10 718 bytes |
| Correctness | 0 wrong answers in 535 sample hosts on all variants in V8 and QuickJS, and 0 wrong in 174 723 hosts (every listed domain, a sub-domain of each, 50 000 near-misses, listed names used as a prefix of another domain) for the two `.ir` variants in V8 |
| Speed, compact variant | load about 10 ms, 4 to 7 microseconds per lookup (V8 and QuickJS) |

Not verified: Windows' PAC engine (WinHTTP; measured by the CI job), browsers that fetch the script
themselves, and the exact size cap in Chromium. Its source defines a maximum PAC size
(`pac_file_fetcher_impl.cc`; the value was not visible in what was read) and third-party notes quote
1 MB. So the budget is a hard 1 000 000 bytes and a target of 750 000.

## Implementation notes (step 07r)

The generator is `PacPlan` in `crates/core/src/pac_script.rs`. Layers answer in this order: the user's rules
(a linear scan, in order), each list (a bucket table; a single-label entry such as `ir` is stored as `1` for
the whole TLD), then the default. Plain host names are `DIRECT`; hosts are lower-cased and one trailing dot is
removed. A script over 1 000 000 bytes is an error (`PacError::TooLarge`), never a truncated script. On the
real lists the Rust output is 649 575 bytes (Iran, `.ir` as one rule) and 9 778 bytes (FOD). Correctness is
checked by running the script in Node against `RuleList::matches` and the rule order. Windows' engine is
asked again by the `PAC benchmark` workflow (`rust-bucket-ir`, `rust-fod`), and the same run reports how
WinHTTP treats the `SOCKS5` keyword, which is not yet known.

## Decisions

1. **A `Rules` profile is an ordered list of rules plus a default target.** A rule matches a domain
   suffix (an entry matches itself and every sub-domain) and points at a target: a proxy, a group,
   `Direct` (new, built in), later `Block`. First match wins. The user's own rules always come before
   any list, so a list can never override what the user typed.
2. **Rule lists are named sources**, each with a format, an action and a refresh policy. Two formats:
   plain (`domains.txt`, one domain per line) and dotted (FOD, `.example.com`). Import normalises
   (lower case, no leading dot, de-duplicated), drops and counts invalid lines, and removes entries
   covered by a parent entry. A TLD that dominates a list (`.ir`) becomes a single rule instead of data.
3. **A list only chooses between the user's own targets.** It can never add a proxy server, so a bad or
   hijacked list can send a site direct or through a proxy the user already trusts, nothing else.
   Ads blocking (`Block`) stays opt-in and out of phase 2.
4. **Two presets, off by default:** "Iran: local sites direct" (Iran list -> Direct, default = the
   selected proxy or group) and "Developer services" (FOD -> the selected proxy, default Direct).
5. **The system gets the rules as a PAC script**, generated in the compact bucket format: domains grouped
   by TLD and first two letters, searched with `indexOf`. It is ES3 (no `let`, arrows or trailing
   commas) because Windows' engine is old; plain host names answer `DIRECT`; host names are lower-cased;
   the TLD table is guarded with `hasOwnProperty` (hosts such as `constructor.com`). The real generator
   will be written in Rust and held to the same exhaustive check the Python spike passes, run in CI with
   Node. Size budget as above; if a list does not fit, the import says so instead of producing a broken PAC.
6. **Delivery is a static PAC served from `127.0.0.1`.** A `file://` address is not dependable across
   engines (Chromium's fetcher, for one, refuses redirects to `file://` and can be built without file
   support; not verified further), so HTTP on the loopback interface it is. Consequence: while a `Rules` profile is applied, a Hopswitch process must be running. The CLI
   alone cannot hold it, so applying a `Rules` profile belongs to the desktop app (or an explicit
   `hopswitch serve`). What apps do when the PAC address is dead differs and is not verified, so the app
   must remove the PAC setting on exit and a watchdog must do it after a crash (extends Step 4b).
7. **Lists are updated on request or on a schedule the user turned on**, from the release asset URL,
   checking the published `.sha256` when there is one (that detects a damaged download, not a hostile
   origin). The previous list stays if an update fails to download or to validate. GitHub is sometimes
   unreachable from Iran, so a download can go through the selected proxy and a file can be imported by
   hand. A snapshot ships with the app so rules work offline.
8. **Attribution.** The Iran list is MIT: its notice ships in `THIRD_PARTY_NOTICES`. The FOD list is a
   public list of facts contributed through issues; it is credited and shipped as a normalised snapshot.
   (Not legal advice. The CC BY-SA 3.0 mention in its README was noted; the project's decision is to
   treat the list as public facts and to credit it.)
9. **Prerequisites:** a first-class PAC state in `ProxyState` (today the Windows adapter reports PAC as
   "other"), the `Direct` profile, and the restore fidelity fix of Step 4b.
10. **Decision gate.** The compact variant goes ahead if WinHTTP loads it, answers every sample host
    correctly and the first lookup (download + compile) takes under 2 seconds. If not, shrink the list
    (for example only the `proxy` and hand-picked categories) or move this feature behind the local
    router of ADR-0010 section 5, where a trie in Rust has no such limits.

## Consequences

* The two most useful lists for the target users become first-class and stay under the PAC size cap.
* Per-domain rules need an always-running Hopswitch; the CLI-only story is limited to plain proxies and
  groups.
* The benchmark tools stay in `tools/pac-bench` so the numbers can be reproduced when the lists grow.

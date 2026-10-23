# ADR-0008: OS adapters talk to the system through a `CommandRunner` port

* **Status:** accepted
* **Decisions:**
  1. **Adapters are testable anywhere.** `core` defines `CommandRunner` (+ `testing::FakeRunner`).
     Each `platform-*` crate keeps its logic pure (parse / plan / render) and only the tiny
     `ProcessRunner` starts real processes. Adapter tests run on every OS in CI.
  2. **Windows proxy = registry + `WinINet` refresh**, via `powershell -EncodedCommand`
     (UTF-16 base64) so no value can break command-line quoting. The refresh
     (`INTERNET_OPTION_SETTINGS_CHANGED` / `REFRESH`) is best effort.
  3. **Mapping.** `host:port` = HTTP, `socks=host:port` = SOCKS5; the server is written before
     `ProxyEnable`; a manual proxy removes any PAC script. HTTPS and SOCKS4 proxies, and bypass
     entries containing `;` or control characters, are *refused* (`Unsupported`) rather than stored wrongly.
  4. **Foreign states are kept value for value.** PAC scripts, per-protocol proxies, an enabled proxy
     with no server, etc. become `ProxyState::Other` whose payload is all five registry values as JSON;
     writing it back restores each value or removes it. `AutoDetect` alone is not a proxy and is left untouched.
  5. **"Disabled" restores the effective setting, not stale bytes.** It sets `ProxyEnable=0` and leaves any
     leftover `ProxyServer` alone.
  6. **Windows DNS is deferred to Step 10.** It needs administrator rights, which is exactly what the
     elevated helper provides; until then DNS calls return `Unsupported` with an explanation.
* **Consequences:** the adapter is verified by fakes only; running it on a real Windows machine
  (`cargo run -p hopswitch-platform-windows --example proxy_smoke`) is a required manual check
  before relying on it.

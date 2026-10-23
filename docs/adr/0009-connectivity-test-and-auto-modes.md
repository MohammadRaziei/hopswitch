# ADR-0009: Real connectivity probes, and auto connect / auto switch as a pure decision engine

* **Status:** accepted
* **Decisions:**
  1. **"Ping" means a real test, never ICMP.** A proxy can ignore ICMP and still work. Proxy probe = a
     small HTTP request *through that proxy* to a target URL; DNS probe = a real query for a domain sent
     *directly to that server*. Both are independent of the current system settings, so every saved
     profile can be tested even when nothing is active. Latency = time to the first complete answer.
  2. **User-editable test targets** (`ConnectivitySettings`): proxy URL, DNS domain, timeout
     (500-10 000 ms), interval (5-3600 s). Defaults: `http://cp.cloudflare.com/generate_204` (light,
     cookie-free, plain HTTP so no TLS stack is needed) and `example.com`. No telemetry; probes only
     go to the configured targets.
  3. **Two options per kind, independent for proxy and DNS** (`AutoSettings`):
     * *Auto connect*: connect when something answers; when nothing does, disconnect, restoring the
       original settings through the normal snapshot/restore path.
     * *Auto switch*: while connected, move to the fastest profile; never connects or disconnects alone.
     * *Both*: connect to the fastest, fail over to a live one, disconnect only if all are dead.
     Connect-only picks the first live profile in the user's list order (order = priority).
  4. **Anti-flapping.** A profile must fail N probes *in a row* (default 3) before it is given up on,
     and another must be faster by at least 50 ms *and* 20 % (both configurable) to win a switch.
  5. **Pool = profiles the user ticked**, not every saved profile. A profile applied by hand that is not
     in the pool is never touched by the autopilot.
  6. **Where the code lives.** The decision logic (`Autopilot::observe`) is pure and lives in `core`;
     it takes probe results and returns `Keep | Activate(id) | Deactivate`. Probing sits behind a port
     (next step) so it is tested with fakes; the real prober and the background loop (Tauri shell,
     next to the tray) apply the decisions through the existing `ProxyService` / `DnsService`.
* **Consequences:** auto DNS on Windows waits for the elevated helper (Step 10); auto proxy works as
  soon as the real prober exists. The probe interval costs a little traffic, so auto modes are off by default.

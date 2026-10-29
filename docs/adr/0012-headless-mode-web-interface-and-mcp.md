# ADR-0012: One `hopswitch` command: GUI when there is one, CLI otherwise, a web interface and an MCP server for machines without a screen

* **Status:** accepted for the decisions below; the order of work is at the end. Nothing in section
  "Not decided yet" may be built before the user answers it.
* **Context:** The user wants Hopswitch to install and work on machines **without graphics** (servers) as
  easily as on a desktop, to be reachable from a browser on such a machine (`--ip`, `--port`), and to
  behave like MATLAB: the bare command opens the GUI, any argument makes it a command-line tool. He also
  asked for an MCP server if it is useful. The core is already independent of any UI (HANDOFF section 1),
  the CLI already works without a screen, and a loopback HTTP server (`hopswitch-pacserver`) exists.
* **Decisions:**
  1. **One command, two shapes.** `hopswitch` with **no arguments** opens the GUI when there is a graphical
     session *and* the GUI is installed; otherwise it prints the help text plus one line saying why no GUI
     was opened and how to get a web interface (`hopswitch serve`), and exits with code 0. **Any argument**
     (`--version`, `status`, `proxy use ...`) runs the CLI and never opens a window. The GUI is a separate
     program, `hopswitch-gui`, installed next to `hopswitch`: the CLI package has no window-system
     dependencies, so it installs on a server; the GUI package is optional.
  2. **A graphical session** is decided from the environment, not by trying to open a window: on Linux
     `DISPLAY` or `WAYLAND_DISPLAY` set and not empty; on macOS and Windows assumed present unless the
     process runs inside an SSH session (`SSH_CONNECTION` set). The decision is a pure function with tests;
     starting the GUI is the only untested line.
  3. **Elevation is per action, never for the whole program.** The bare command does not elevate. When an
     action needs administrator rights (Windows DNS, writing system files on Linux), that action asks for
     them (UAC prompt, `sudo`, or the helper of ADR-0008), so the GUI itself never runs as administrator.
     This is the principle of least privilege; "run the GUI as admin if needed" is satisfied per action.
  4. **`hopswitch serve [--ip <addr>] [--port <n>]`** runs the web interface and its JSON API, and also runs
     the auto-connect loop (`--no-auto` turns that off), so one process is the whole headless product.
     Defaults: `--ip 127.0.0.1`, `--port 47831` (a fixed, uncommon port so an SSH tunnel is easy to write).
     Port `0` means "pick a free one" and is printed.
  5. **Security of the web interface** (it changes the network settings of the machine, so it is treated as
     a privileged local service):
     * Every `/api/` request needs `Authorization: Bearer <token>`. The token is random (at least 128 bits
       from the operating system), compared in constant time, stored in the settings folder with owner-only
       permissions, and printed at start inside a URL fragment (`http://127.0.0.1:47831/#token=...`), which
       a browser never sends to a server.
     * The `Host` header must be the server's own address (as in `hopswitch-pacserver`; stops DNS
       rebinding) and a request with a foreign `Origin` is refused. No CORS headers are ever sent.
     * **Loopback only (changed 2026-10-29, at the user's request).** A non-loopback `--ip` (`0.0.0.0`, a
       LAN address) is always refused, and there is no `--allow-remote`; `--ip` accepts only loopback
       addresses (`127.0.0.0/8`, `::1`). The way to reach a server is
       `ssh -L 47831:127.0.0.1:47831 user@server`, which also encrypts. TLS is not part of Hopswitch:
       with loopback only there is nothing to protect on the wire and no certificate to manage. If a
       reverse proxy is ever wanted, that is a new ADR.
     * The loopback, `Host`, size and timeout protections live once in crate `hopswitch-http`
       (std only); the PAC server and the web interface are handlers on top of it (07za).
     * The server never serves files from disk; the interface is embedded in the binary.
  6. **One interface, two shells.** The web interface is built once (React, ADR-0001/0004). The GUI shows the
     same pages; the API is the contract. (Whether the GUI talks to the same HTTP API or to Tauri commands
     is decided when the GUI is built; the API must not depend on that choice.)
  7. **Install without a screen.** The CLI is released as a **static binary** per platform (Linux:
     `x86_64` and `aarch64`, `musl`, no shared libraries, no GUI libraries) with a `.sha256` next to each
     file. `hopswitch service install` / `uninstall` / `status` writes, enables and removes a service
     definition that runs `hopswitch serve` (Linux: a `systemd` unit; Windows: a service; macOS:
     `launchd`) and refuses to run without administrator rights. An install script may only download a
     release file, verify its checksum, and put it in place; it must not do anything else.
  8. **MCP server: yes, small and read-only by default.** `hopswitch mcp` speaks the Model Context
     Protocol over standard input and output (JSON-RPC 2.0: `initialize`, `tools/list`, `tools/call`). Its
     tools are the CLI commands, run through the same code, so the behaviour is identical: read-only
     tools (`status`, list profiles, `ping`, presets) are always on; tools that change the system
     (`proxy use`, `off`, `auto ...`) exist only when started with `--allow-write`. It has no network
     listener. An assistant that can change a machine's proxy can redirect that machine's traffic, so
     writing is opt-in by the person who configured the server, not by the assistant.
* **Linux adapter, first target (decided 2026-10-29; the user named shell proxy variables, left the rest to
  us, then proposed the "source once" shape below, which replaces the first draft):** a "proxy" on a server
  is a set of environment variables, and a running process cannot change the environment of shells that are
  already open. So the adapter owns one file, `<settings dir>/proxy.env` (`export` lines for `http_proxy`,
  `https_proxy`, `all_proxy`, `no_proxy` and the upper-case twins; `no_proxy` from the profile's bypass
  list), and writes **once** a `core::managed_block` into the user's shell start file that only sources it
  if it exists (ADR-0003, AGENT.md rule 7: backup first, damaged markers are refused). After that, `use`,
  `off` and `auto` change only `proxy.env`; the start file is not touched again. `off` empties or removes
  `proxy.env` and leaves the block (a no-op without the file); the block is removed only by an explicit
  command. It affects **new shells only**; `status` and `use` must say so. **Scope: current user first**, no
  administrator rights. System-wide (`/etc/profile.d/hopswitch.sh`) is a later, separate step with
  per-action elevation (decision 3). `apt`, `docker` and DNS stay undecided and are not built.
  **Open before building (researched in step (d), not guessed):** which start files (`.profile` is read by
  login shells only, `.bashrc` mostly by interactive ones, non-interactive `ssh host cmd` and cron read
  neither); where the block goes (a later `export http_proxy` by the user would override ours); how a SOCKS
  profile maps to these variables (fail closed like the Windows PAC rule unless the tools' behaviour has
  been read).
* **Order of work:** (a) `--version` and the bare-command decision (pure, tested); (b) `serve` with token,
  loopback and the `Host`/`Origin` checks, and a status endpoint; (c) the MCP server (read-only first);
  (d) the Linux adapter for the targets the user names; (e) `service install`; (f) the interface pages;
  (g) release builds and the install script. Each step is independently useful and tested; none is
  announced as working until it has been run, and anything that can only run on a machine the sandbox
  does not have (Windows services, `launchd`, real GUI start) is marked untested in HANDOFF.
* **Progress:** (a) done in 07z. (b) done in 07za-07ze: `hopswitch serve` with the token, loopback and
  `Host`/`Origin` checks and `GET /api/status`; the shared server is crate `hopswitch-http`. Not yet: write
  endpoints, interface pages, MCP, `service install`.
* **Consequences:** The CLI crate stays free of window-system code. `hopswitch-gui` is a second binary
  in the same release. A new HTTP server (not the PAC server) is needed; it should share the request
  parsing and the `Host` check with `hopswitch-pacserver` instead of copying them.

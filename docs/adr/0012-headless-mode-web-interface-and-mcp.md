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
     * A non-loopback `--ip` (`0.0.0.0`, a LAN address) is **refused** unless `--allow-remote` is also
       given, and then a warning is printed that the traffic is plain HTTP. The recommended way to reach a
       server is `ssh -L 47831:127.0.0.1:47831 user@server`. TLS is not part of this decision: it needs a
       library or a certificate story and gets its own ADR if wanted.
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
* **Not decided yet (asked of the user):** what the Linux adapter changes on a server. Servers have no
  system proxy; programs read `http_proxy` / `no_proxy` variables, `apt` configuration, a Docker daemon
  file or a `systemd` service. Each needs its own file and administrator rights. Until the user names
  the targets, only `resolvectl`/`nmcli` DNS and a shell-environment managed block (ADR-0003) are
  candidates and nothing is built on a guess.
* **Order of work:** (a) `--version` and the bare-command decision (pure, tested); (b) `serve` with token,
  loopback and the `Host`/`Origin` checks, and a status endpoint; (c) the MCP server (read-only first);
  (d) the Linux adapter for the targets the user names; (e) `service install`; (f) the interface pages;
  (g) release builds and the install script. Each step is independently useful and tested; none is
  announced as working until it has been run, and anything that can only run on a machine the sandbox
  does not have (Windows services, `launchd`, real GUI start) is marked untested in HANDOFF.
* **Consequences:** The CLI crate stays free of window-system code. `hopswitch-gui` is a second binary
  in the same release. A new HTTP server (not the PAC server) is needed; it should share the request
  parsing and the `Host` check with `hopswitch-pacserver` instead of copying them.

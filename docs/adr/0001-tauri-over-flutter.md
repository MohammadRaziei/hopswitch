# ADR-0001: Tauri 2 instead of Flutter

* **Status:** accepted
* **Context:** The app is a desktop-only utility whose real work is talking to OS network
  settings, living in the tray, and starting at login. It needs a polished, animated UI.
* **Decision:** Tauri 2 (Rust backend + web UI).
* **Why:** native OS access and tray/autostart/updater are first-class in Rust/Tauri; installers
  are a few MB with low idle RAM; the web ecosystem gives the richest UI tooling (and RTL support).
* **Consequences:** two languages (Rust + TypeScript); UI tests need a mocked IPC layer
  (`mockIPC`). Flutter would only win if mobile became a goal.

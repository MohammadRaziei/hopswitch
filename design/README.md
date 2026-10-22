# Design preview

Static mockups (design-canvas artboards) of the Hopswitch UI. They are a **visual reference**, not
production code; the real frontend is built in Steps 8–9 of `CHECKLIST.md`.

| Board | Shows |
|-------|-------|
| `Main` | Two independent panels (Proxy, DNS), icon rail, global "Turn everything off" |
| `Tray` | Quick switch with separate PROXY and DNS sections |
| `Settings` | General (autostart, language, theme) and Safety (restore on quit, backups) |
| `About` | Version, update check, logs, licence, links |
| `Editor`, `Empty`, `Rtl` | **Outdated** (still the old combined-profile model); to be aligned with ADR-0004 |

Live canvas: https://claude.ai/artifact/Jn8Tzdn3Kgb4i88dujgo1p

## Viewing the preview locally

`design/preview/index.html` is a self-contained page with all current boards. Open it directly in a
browser, or serve the folder (no build step, no dependencies):

    python3 -m http.server 8000 --directory design/preview   # then open http://localhost:8000

(The individual `*.dc.html` boards need a `support.js` wrapper that is not part of this repo.)

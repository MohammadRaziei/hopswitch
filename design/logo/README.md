# Hopswitch logo

**Idea:** a switch whose knob *hops* from one side to the other: the app in one picture
(switch between proxy/DNS profiles). It follows the UI theme (`design/preview`): dark first, emerald
accent. The track is the green "on" state of the toggle, the arrow is the hop.

| File | Use |
| --- | --- |
| `icon.svg` | App icon, 48 px and up (1024 master, transparent corners) |
| `icon-small.svg` | The same idea reduced to a bare switch, for 16-32 px. The arrow turns into mush below ~48 px and starts to look like a padlock |
| `tray-template.svg` | Black + transparency only, for the macOS menu bar (template image) |
| `logo-light.svg`, `logo-dark.svg` | Icon + wordmark for light / dark backgrounds. Text is converted to paths, so no font is needed |
| `export/` | Generated PNG / ICO files, named like the output of `tauri icon`, plus tray icons and the GitHub social preview |

## Sizes (why these)

* **Tauri** takes a square PNG or SVG with transparency, at least 512 px, and writes `32x32.png`,
  `128x128.png`, `128x128@2x.png` and `icon.png`. Its docs recommend a simpler design for small sizes
  if you build the icons yourself, which is why `32x32.png` and the 16-32 px layers of `icon.ico`
  come from `icon-small.svg`.
* **Windows tray:** 16x16, and 32x32 for 200 % scaling -> `tray-16.png`, `tray-32.png`.
* **macOS menu bar:** 22x22 and 44x44 @2x, black + alpha, file name ending in `Template` ->
  `trayTemplate.png`, `trayTemplate@2x.png`.
* **GitHub social preview:** 1280x640 -> `export/social-preview.png` (upload it in the repo's Settings).

## Using it in the Tauri app

```
cp design/logo/export/{32x32.png,128x128.png,128x128@2x.png,icon.png,icon.ico} src-tauri/icons/
cargo tauri icon design/logo/icon-1024.png   # only if you also need icon.icns / mobile sizes;
                                             # then copy 32x32.png and icon.ico back over the generated ones
```

## Rebuilding

Edit the three hand-drawn SVGs, then `python design/logo/build.py`
(needs `pip install novasvg pillow fonttools` and the Poppins font; set `POPPINS_DIR` if it is elsewhere).

## Colours

Taken from the UI mockups (`design/preview`), so the logo and the app look like one product.

| | |
| --- | --- |
| Icon background (gradient) | `#1E222B` -> `#0B0D11`, rim `#2C5A4C` |
| Green track / accent | `#34D399` (on dark), `#059669` (on light) |
| Arrow | `#6EE7B7` |
| Knob | `#FFFFFF` -> `#C9D0DC` |
| Wordmark on dark | `#E8EAF0` + `#34D399` |
| Wordmark on light | `#0F1115` + `#059669` |
| Social preview | background `#0F1115`, tagline `#9AA3B2` |

Wordmark: Poppins Bold (SIL Open Font License).

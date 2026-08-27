# Plan 048 — Finalize the Voicetypr icon system

Baseline: `fix/047-silent-failures` working tree on 2026-08-27. Trigger: user
rejected the current multi-element app icon and approved the minimal ivory `V`
direction with one restrained speech notch.

## Scope A — approval master (this pass)

1. Create one deterministic SVG master for the approved mark: graphite plate,
   ivory `V`, one small speech notch, optically centered and legible at 16 px.
2. Create derived light-shell and dark-shell transparent SVG variants for
   Windows. Preserve the same geometry; change contrast treatment only.
3. Render a review sheet at 16, 32, 64, and 128 px on representative light and
   dark backgrounds.
4. Validate SVG parsing, bounds, palette, and raster dimensions.

## Scope B — shipping integration (explicitly deferred)

- Do not replace `src-tauri/icons/*`, `public/AppIcon.png`, the React
  `Brandmark`, tray assets, Store/MSIX assets, or release screenshots in this
  pass.
- After visual approval, generate `.icns`, multi-resolution `.ico`, Tauri PNGs,
  MSIX square/store assets, and Windows theme-qualified target-size assets.
- Shipping integration requires macOS Dock/Finder/app-switcher smoke and real
  Windows light/dark Taskbar/Start/Alt+Tab/Explorer smoke.

## Verification

- Parse every SVG as XML.
- Rasterize the master and both Windows variants at required proof sizes.
- Confirm output dimensions and visually inspect the review sheet.
- `git diff --check`.


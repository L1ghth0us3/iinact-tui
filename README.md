# iinact-tui

A fast, dependency‑light terminal DPS meter for FFXIV that connects to an IINACT WebSocket server and renders a kagerou‑style table using ratatui.

- Transport: plain WebSocket (OverlayPlugin‑compatible)
- Default endpoint: `ws://127.0.0.1:10501/ws`
- UI: compact TUI, respects your terminal background (blur/transparency)

## Features
- Auto‑connects to IINACT and subscribes to `CombatData` and `LogLine`.
- Live table sorted by ENCDPS; party-only rows (known job codes).
- Damage share column (Share%) with higher priority than ENCDPS/Job on narrow layouts.
- Right-aligned numeric headers and values (ENCDPS, Crit%, DH%, Deaths).
- Responsive columns at small widths (minimal and name-only modes).
- Decorations (cycle with `d`):
  - `Decor: underline` — thin role-colored bar directly under each entry (two-line rows).
  - `Decor: background` — role-colored background meter behind each entry (one-line rows).
  - `Decor: none` — no extra decoration (compact one-line rows).
- Encounter/Zone header on top, Dur | ENCDPS | Damage below it; dim gray header separator.
- Idle detection with a status indicator that flips to “Connected (idle)” after a configurable timeout.
- Settings pane (`s`) with persisted configuration stored under `~/.config/iinact-tui/iinact-tui.config` (override via `IINACT_TUI_CONFIG_DIR`).
- Configurable defaults for decoration style and opening mode, adjustable from the settings pane.
- Encounter history is recorded end-to-end: every CombatData frame is captured with its raw payload, and the TUI lazily loads summaries, encounters, and full detail as you drill in.

## Prerequisites
- Rust 1.74+ (stable) recommended
- IINACT running locally (or reachable over your network)
  - Default WebSocket endpoint: `ws://127.0.0.1:10501/ws`
  - IINACT implements the OverlayPlugin API (`getLanguage`, `subscribe` with `CombatData`/`LogLine`).

## Build & Run
```bash
# From the repo root
cargo run
# Write logs to the default config directory (~/.config/iinact-tui/debug.log)
cargo run -- --debug
# Or choose a custom log file path
cargo run -- --debug ./logs/iinact-debug.log
```
The app will connect automatically to `ws://127.0.0.1:10501/ws` and begin rendering as soon as events arrive.

### Debug logging
- Pass `--debug` to enable file logging at startup. Without it, the TUI stays silent (no stdout/stderr noise).
- Supplying `--debug` with no value writes all tracing output (info/debug/warn/error) to `~/.config/iinact-tui/debug.log` on Unix-like systems or the equivalent config directory on Windows.
- Provide a path after `--debug` (e.g., `--debug ./logs/iinact.log`) to log elsewhere; parent directories are created automatically if needed.

## Controls
- `q` or `Esc` — quit
- `d` — cycle decorations (underline → background → none)
- `m` — toggle table mode (DPS ↔ HEAL)
- `s` — toggle the settings pane
- `h` — open/close the encounter history panel
- `i` — when idle mode is active, toggle the idle overlay on/off to peek at the last encounter
- `↑/↓` — move the selection inside the settings pane
- `←/→` — adjust the selected setting (idle timeout, default decoration, default mode)

## Notes & Behavior
- Party‑only: rows are filtered to common job codes (PLD/WAR/DRK/GNB, WHM/SCH/AST/SGE, MNK/DRG/NIN/SAM/RPR/VPR, BRD/MCH/DNC, BLM/SMN/RDM/PCT, BLU).
- Normalization: numeric fields arrive as strings; commas/percent signs are stripped before parsing for sorting/ratios. Damage share is computed from per‑combatant damage over encounter total.
- Case‑insensitive: keys like `encdps`/`ENCDPS` are handled consistently.
- Encounter naming: while a fight is active some servers report generic names (e.g., "Encounter"); the header falls back to Zone until a final name is available.
- Background: widgets avoid setting a background color so your terminal theme (blur/transparency) stays visible. The header separator uses a subtle gray; background meters intentionally set a background for the meter fill only.
- Persisted config: settings are written to `~/.config/iinact-tui/iinact-tui.config` on Linux/macOS (or `%APPDATA%\iinact-tui\iinact-tui.config` on Windows). Set `IINACT_TUI_CONFIG_DIR` to override.
- History panel: press `h` to switch into the history view; use `↑/↓` or mouse scroll to pick a date, hit `Enter`/click to drill into the encounters list, press `Enter` again for per-encounter details, and `←`/`Backspace` to step back. Date and encounter lists load from lightweight indexes first, with overlay indicators while data hydrates; encounter detail fetches the full frame-by-frame record on demand.
- Idle overlay: when the app is idle you’ll see the idle window by default—press `i` to hide/show it without leaving idle mode so you can review the most recent encounter quickly.

## Troubleshooting
- No data? Confirm IINACT is running and the endpoint is reachable. The default is `ws://127.0.0.1:10501/ws`.
- Table is empty? Only party jobs are shown; pets/limit break lines are filtered out.
- Rendering glitches on low‑color terminals? Consider using a non‑truecolor theme; role colors fall back to xterm‑256 indices (75/41/124) for meter fills.

## Roadmap (short)
- In‑TUI URL editor to switch WS endpoints.
- Theme presets (purple/cyberpunk, monochrome, gray meters).
- Persist meter mode and layout preference.

## License
This project does not currently declare a license. Ask before redistributing.

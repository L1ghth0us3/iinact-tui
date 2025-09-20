# Changelog

All notable changes to this project will be documented in this file.

The format is inspired by Keep a Changelog and uses calendar dates (YYYY‑MM‑DD).

## [Unreleased]
- Add optional color presets and theme tweaks for background meters.
- In‑TUI URL editor to switch WS endpoints.
- Persist user preferences between runs.

## [0.1.0] - 2025-09-20
Initial MVP of the IINACT terminal DPS meter (ratatui).

Highlights
- Auto‑connects to IINACT at `ws://127.0.0.1:10501/ws` and subscribes to `CombatData` + `LogLine`.
- Party‑only rows (filters to known FFXIV jobs); case‑insensitive keys and numeric normalization.
- Live table with kagerou‑inspired columns: Name, Job, ENCDPS, Crit%, DH%, Deaths.
- Right‑aligned numeric headers and values; responsive column set based on terminal width.
- Two‑line per‑entry bars (meter:off): thin role‑colored bar directly under each entry (tank=75, healer=41, dps=124).
- Background meter mode (meter:on): compact one‑line rows with a role‑colored background fill proportional to ENCDPS.
- Header: Encounter/Zone on the first line, Dur | ENCDPS | Damage on the second; dim gray separator under the table header.
- Preserves terminal background (no forced background colors in normal widgets).

Keys
- `q`/`Esc`: quit
- `u`: toggle meter mode (off=underline bars, on=background meters)

Bug fixes and polish
- Ensured header separator always renders (all widths).
- Encounter title stays reactive during active fights (falls back to Zone if “Encounter”/empty).
- Removed experimental gradient bars; simplified to solid role colors for clarity.


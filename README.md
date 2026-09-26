# Solar Hub

A personal macOS desktop app for monitoring a solar inverter and battery, and
automating the output source (Solar / SBG / Utility) overnight and in the
morning using Gemini.

Built with Tauri (Rust) + React + TypeScript.

## Features

- Live inverter snapshot (PV, load, grid, battery, output/charger source) and
  a Bluetooth (BLE) BMS connection for real battery telemetry.
- macOS notifications for grid/battery alerts and automation events.
- An automation engine that:
  - decides whether to switch to SBG overnight so the battery covers the
    remaining hours until sunrise, then verifies the decision against real
    discharge data,
  - runs a morning check that switches based on the battery's charge current,
  - defaults to dry run (decides and notifies, never writes) until proven out.

## Requirements

- macOS
- [pnpm](https://pnpm.io)
- Rust (stable) + the Tauri prerequisites for macOS
- Node.js (for the automation agent under `agent/`)
- A [Gemini API key](https://aistudio.google.com/) for automation

## Getting started

```bash
pnpm install
pnpm tauri dev
```

Build a release bundle:

```bash
pnpm tauri build
```

## Project structure

```
src/            React frontend (dashboard, battery, inverter, settings)
src-tauri/      Rust backend (inverter client, BLE battery, automation engine)
agent/          Node scripts that call Gemini for automation decisions
```

The Rust backend spawns the Node scripts in `agent/` on demand (`decide.js`,
`verify.js`, `morning.js`, `sun.js`) and passes JSON over stdin/stdout. See
`.claude/plans` or ask for the automation design notes if you need the full
architecture.

## Configuration

Set up credentials and automation in the app itself, under **Settings**:

- Solar cloud account (user ID, password, station/device ID, time zone) —
  the password is stored in the macOS keychain, never on disk.
- Location (latitude/longitude) — used for sunrise/sunset and sun angle.
- Gemini API key — also stored in the keychain.
- Automation settings (enabled, dry run, battery floor, morning threshold,
  etc.) on the same page.

## Notes

### Battery current source

Battery current (charge and discharge amps) is only ever read from the
Bluetooth BMS. There is no current sensor between the battery and the
inverter, so the inverter has no real current reading to report — if the BMS
isn't connected, current is treated as unknown (`None`/`0`) rather than
falling back to a guessed inverter field. Battery SOC and capacity can still
fall back to the inverter when there's no BMS, since those are separate,
independently-reported values.

### Dry run

Automation ships with `dry_run` on by default. In dry run the engine still
decides and sends notifications ("Would switch to SBG because…") but never
writes to the inverter. This also protects against an old saved config that
may already have `enabled: true`. Turn it off only after a few dry-run nights
look right.

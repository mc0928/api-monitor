# API Monitor

[![CI](https://github.com/mc0928/api-monitor/actions/workflows/ci.yml/badge.svg)](https://github.com/mc0928/api-monitor/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/mc0928/api-monitor)](https://github.com/mc0928/api-monitor/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

[简体中文](./README.md) | [English](./README.en.md)

A desktop dashboard for balances, channel health, model multipliers, success rates, and trends across multiple new2api and sub2api sites.

## Features

- Shows real model-group multipliers such as `0.16x`; unknown values appear as `--`.
- Displays only configured models across GPT, Claude, Grok, Kimi, Gemini, Qwen, and Seedream.
- Classifies `gpt-image-*` as GPT, Embedding/Reranker models as Qwen, and Seedream image models as Seedream.
- Colors success-rate chart segments green (≥95%), amber (≥80%), and red (<80%).
- Refreshes every minute by default and updates each site as soon as it finishes.
- Includes notifications, tray mode, dark mode, bilingual UI, ordering, and per-site proxy routing.

## Download

Download from the [v0.1.0 Release](https://github.com/mc0928/api-monitor/releases/tag/v0.1.0):

| Platform | Installer |
| --- | --- |
| Windows x64 | `.msi` or `-setup.exe` |
| macOS Apple Silicon | `.dmg` containing `aarch64` |
| macOS Intel | `.dmg` containing `x86_64` |

The macOS build is currently unsigned. If Gatekeeper blocks it, right-click the app in Finder and choose **Open**, or run:

```bash
xattr -cr "/Applications/api-monitor.app"
```

## Usage

1. Launch the app and click **Add site**.
2. Select `new2api` or `sub2api`, then enter the URL and credentials.
3. Configure models, proxy settings, and the refresh interval.
4. Click **Refresh all**.

Credentials stay on your computer. sub2api login tokens are kept in memory only.

## Data freshness

Requests bypass caches and run at the configured minute interval. Active monitors commonly provide minute-level records, while some new2api and sub2api V2 endpoints only expose hourly data. API Monitor preserves real upstream history and appends each refresh as a local minute sample without inventing upstream detail.

## Configuration paths

| Environment | Path |
| --- | --- |
| Windows | `%APPDATA%\api-monitor\sites.json` |
| macOS | `~/Library/Application Support/api-monitor/sites.json` |
| Source checkout | `config/sites.json` |

Using the UI is recommended. See [`config/sites.example.json`](./config/sites.example.json) for the available fields.

## Run from source

Install Node.js 18+, Rust stable, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
npm ci
npm run tauri dev
```

```bash
npm run typecheck
npm test
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

Installers are written to `src-tauri/target/release/bundle/`.

## Privacy and license

The app has no telemetry and only contacts configured sites and GitHub's release API. Never publish tokens, email addresses, or passwords.

[MIT License](./LICENSE)

<div align="center">

# API Monitor

### One desktop app to monitor balances, channel health, model multipliers, and success-rate trends across all your new2api / sub2api sites

[![Version](https://img.shields.io/github/v/release/mc0928/api-monitor?color=blue&label=version)](https://github.com/mc0928/api-monitor/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey.svg)](https://github.com/mc0928/api-monitor/releases)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-orange.svg)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

English | [简体中文](./README.md)

</div>

## Why API Monitor?

Running multiple new2api / sub2api relay sites means logging into each web panel just to check a balance, digging through pages for channel status and model multipliers, and discovering success-rate problems only after requests start failing.

**API Monitor** puts all of your sites into one local desktop dashboard: add a site and it polls automatically at your chosen interval, showing balances, channel health, model-group multipliers, and success-rate trends on a single screen. Failures trigger instant system notifications, the app lives in the tray, and credentials never leave your machine.

- **All sites, one screen** — balances, channel health, multipliers, and trends for every new2api / sub2api site in one place
- **Real multipliers, no guessing** — shows upstream model-group multipliers such as `0.16x`; unknown values appear as `--`
- **Minute-level success trends** — every refresh is appended as a local sample; chart segments colored green / amber / red by threshold
- **Instant failure alerts** — system notifications when a site request fails or data looks wrong, no screen-watching required
- **Lives in the tray** — single-instance app with a hover summary across sites
- **Credentials stay local** — no telemetry; sub2api tokens can be captured via browser login and kept in memory only
- **In-app updates** — check, download, and install in one step
- **Cross-platform** — native Windows / macOS app built with Tauri 2

## Features

### Sites & Channels

- **Two site types** — `new2api` (token + user ID) and `sub2api` (username/password, or automatic token capture via browser login)
- **Channel health at a glance** — per-site channel list with model-group multipliers
- **Site ordering** — drag to reorder manually in Settings, or sort automatically
- **Proxy support** — global proxy with a connectivity test; each site can use its own proxy route

### Models & Multipliers

- **Grouped by vendor** — shows only the models you configure, across GPT, Claude, Grok, Kimi, Gemini, Qwen, and DeepSeek
- **Smart classification** — `gpt-image-*` counts as GPT, Embedding/Reranker models as Qwen, and DeepSeek models as DeepSeek
- **Real group multipliers** — such as `0.16x`; shown as `--` when upstream doesn't provide one

### Success Rate & Data Freshness

- **Success-rate trend line** — covers all sites, colored green (≥95%), amber (≥80%), and red (<80%)
- **Automatic refresh** — every minute by default, adjustable or off; each site updates as soon as it finishes instead of waiting for the slowest one
- **No fabricated data** — requests bypass caches; when some new2api / sub2api V2 endpoints only expose hourly history, the app preserves real upstream records and appends each refresh as a local minute sample. Incompatible sub2api modes fall back to V2 passive monitoring automatically

### UI & System Integration

- **System notifications** — instant alerts on failed requests or abnormal data
- **Tray** — background resident, hover summary across sites, single-instance protection
- **Dark mode · English / 中文 interface**
- **In-app updates** — check under Settings → About, download with progress, restart to install

## Quick Start

1. **Add a site**: launch the app and click **Add site**
2. **Pick a type**: `new2api` takes a token and user ID; `sub2api` takes a username and password, or click **Web login** to sign in via browser and capture the token automatically
3. **Choose models**: fill in the models you want to monitor, grouped by vendor, in Settings
4. **Start monitoring**: click **Refresh all**; refreshes then run automatically at the configured interval

> **Note**: credentials stay on your computer, and sub2api login tokens are kept in memory only.

## Download & Installation

### System Requirements

- **Windows**: Windows 10 and above
- **macOS**: macOS 12 and above (Apple Silicon / Intel)

### Windows Users

Download the `.msi` or `-setup.exe` installer from [Releases](https://github.com/mc0928/api-monitor/releases/latest).

### macOS Users

Download the `.dmg` from [Releases](https://github.com/mc0928/api-monitor/releases/latest): pick the build containing `aarch64` for Apple Silicon, or `x86_64` for Intel.

> **Note**: the macOS build is currently unsigned. If Gatekeeper blocks it, right-click the app in Finder and choose **Open**, or run:

```bash
xattr -cr "/Applications/api-monitor.app"
```

## FAQ

<details>
<summary><strong>Which site types are supported, and what credentials do they need?</strong></summary>

`new2api` and `sub2api`. new2api needs a token and user ID; sub2api takes a username and password, or you can click **Web login**, sign in inside the opened browser, and let the app capture the token automatically.

</details>

<details>
<summary><strong>Why does one of my sites only have hourly data?</strong></summary>

Some new2api / sub2api V2 endpoints only expose history at hourly granularity. The app keeps that real upstream data as-is and appends each refresh as a local minute sample instead of inventing detail. When a sub2api mode is incompatible, it falls back to V2 passive monitoring automatically.

</details>

<details>
<summary><strong>What does a multiplier of <code>--</code> mean?</strong></summary>

The upstream didn't return a multiplier for that model group. The app only shows real data and never guesses.

</details>

<details>
<summary><strong>Are my credentials safe?</strong></summary>

Credentials are stored only in your local `sites.json`, and sub2api login tokens are kept in memory only. The app has no telemetry and contacts only your configured sites and GitHub's release API. Never publish tokens, email addresses, or passwords.

</details>

<details>
<summary><strong>How do I update the app?</strong></summary>

Settings → About → Check for updates to download and install in-app, or grab the latest build from [Releases](https://github.com/mc0928/api-monitor/releases/latest). The app also notifies you when a new version is available.

</details>

<details>
<summary><strong>Where is my configuration stored?</strong></summary>

| Environment | Path |
| --- | --- |
| Windows | `%APPDATA%\api-monitor\sites.json` |
| macOS | `~/Library/Application Support/api-monitor/sites.json` |
| Source checkout | `config/sites.json` |

Configuring through the UI is recommended. See [`config/sites.example.json`](./config/sites.example.json) for the available fields.

</details>

## Architecture Overview

<details>
<summary><strong>Architecture, core modules, and tech stack</strong></summary>

### Architecture

```
┌────────────────────────────────────────────────────┐
│                Frontend (React + TS)               │
│   SiteCard · ChannelList · SettingsDialog          │
│   ProviderFilter · models lib · dark mode · i18n   │
└─────────────────────────┬──────────────────────────┘
                          │ Tauri IPC
┌─────────────────────────▼──────────────────────────┐
│               Backend (Tauri 2 + Rust)             │
│  ┌───────────┐  ┌───────────┐  ┌────────────────┐  │
│  │  new2api  │  │  sub2api  │  │  http (proxy)  │  │
│  └─────┬─────┘  └─────┬─────┘  └────────────────┘  │
│        └───────┬──────┘                            │
│        ┌───────▼────────┐   ┌────────────────┐     │
│        │ state (scheduler)│─►│ persist (store)│     │
│        └────────────────┘   └────────────────┘     │
│   notification · tray · single-instance · updater  │
└────────────────────────────────────────────────────┘
```

### Core Modules

- **new2api / sub2api** — data sources for the two site types; incompatible sub2api modes fall back to V2 passive monitoring
- **state** — monitoring scheduler: timed refresh, per-site push as soon as each finishes
- **persist** — local persistence of results, backing the success-rate trend line
- **http** — proxy routing and requests (cache bypassed)
- **config** — reads and writes `sites.json`

### Tech Stack

**Frontend**: React 18 · TypeScript · Vite · TailwindCSS 3.4 · @dnd-kit

**Backend**: Tauri 2 · Rust · serde · reqwest · tauri-plugin-notification / single-instance / updater / process / opener

**Testing**: Vitest (frontend) · cargo test (backend)

</details>

## Development Guide

<details>
<summary><strong>Build from source</strong></summary>

### Prerequisites

- Node.js 18+
- Rust stable
- [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)

### Development & Build

```bash
# Install dependencies
npm ci

# Dev mode (hot reload)
npm run tauri dev

# Build installers (output in src-tauri/target/release/bundle/)
npm run tauri build
```

### Tests & Checks

```bash
# Frontend type check
npm run typecheck

# Frontend unit tests
npm test

# Backend tests
cargo test --manifest-path src-tauri/Cargo.toml
```

</details>

<details>
<summary><strong>Project Structure</strong></summary>

```
├── src/                        # Frontend (React + TypeScript)
│   ├── components/             # SiteCard / ChannelList / SettingsDialog / ProviderFilter
│   ├── lib/                    # Model grouping and helpers (covered by Vitest)
│   └── types.ts
├── src-tauri/                  # Backend (Rust + Tauri 2)
│   └── src/
│       ├── new2api.rs          # new2api data source
│       ├── sub2api.rs          # sub2api data source (with V2 passive fallback)
│       ├── state.rs            # Monitoring scheduler and state
│       ├── persist.rs          # Result persistence
│       ├── http.rs             # Proxy and requests
│       └── config.rs           # sites.json I/O
├── config/sites.example.json   # Example configuration fields
└── .github/workflows/          # CI and release automation
```

</details>

## Contributing

Issues and PRs are welcome! Before submitting, please make sure:

- Type check passes: `npm run typecheck`
- Frontend tests pass: `npm test`
- Backend tests pass: `cargo test --manifest-path src-tauri/Cargo.toml`

## License

[MIT](./LICENSE) © mc0928

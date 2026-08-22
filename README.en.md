# API Monitor

[简体中文](./README.md) | [English](./README.en.md)

A small desktop app for monitoring API relay sites: check balances, channel status, and success rates in one place.

Frontend: Tauri 2 + React 18 + TypeScript + Tailwind CSS. Backend: Rust / reqwest.

## Features

- **new2api sites**: query account balance and cumulative request count with a personal access token; pull model-plaza group performance (success rate / latency)
- **sub2api sites**: log in with email & password and pull the channel monitor list (status, 7-day success rate, usage quota, balance)
- **Unified dashboard**: summarizes operational / failed / unchecked counts per site, ranked by success rate by default
- **Filter by model family**: one-click filtering of channels by GPT / Claude / Grok / Kimi
- **Proxy support**: a global Clash mixed-proxy address (with a one-click connectivity test); each site can individually opt in to routing through the proxy
- **Custom monitored models**: new2api groups preferentially show performance data for models matching your configuration

## Getting Started

### Prerequisites

- Node.js ≥ 18
- Rust stable ([rustup](https://rustup.rs/))
- [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/) (Windows needs WebView2, preinstalled on Win10/11)

### Development

```bash
npm install
npm run tauri dev
```

### Build

```bash
npm run tauri build
```

The bundle is output to `src-tauri/target/release/bundle/`.

### Tests

```bash
npm run typecheck     # frontend type checking
cargo test            # backend parsing unit tests (run inside src-tauri)
```

## Configuration

The config file is `config/sites.json`, created automatically the first time you save settings (already in `.gitignore` — never commit real credentials):

```json
{
  "proxy": { "url": "http://127.0.0.1:7897" },
  "monitor": {
    "models": {
      "gpt": ["gpt-5.6-sol", "gpt-5.6-terra"],
      "claude": ["claude-sonnet-5", "claude-opus-5"],
      "grok": ["grok-4.6"],
      "kimi": ["kimi-k3"]
    }
  },
  "sites": [
    {
      "id": "site-xxx",
      "name": "Example new2api",
      "type": "new2api",
      "base_url": "https://example.com",
      "vpn": false,
      "token": "sk-..."
    },
    {
      "id": "site-yyy",
      "name": "Example sub2api",
      "type": "sub2api",
      "base_url": "https://example.com",
      "vpn": true,
      "username": "you@example.com",
      "password": "..."
    }
  ]
}
```

- `proxy.url`: Clash mixed-proxy address; leave empty to disable the proxy
- `sites[].type`: `new2api` (token-based balance lookup) or `sub2api` (login-based channel monitors)
- `sites[].vpn`: when `true`, requests to that site go through the proxy

Config lookup order: current working directory → walk up from the exe directory, using the first existing `config/sites.json`. This way the same config is found whether you launch from the project root or by double-clicking the built exe.

## Security

- **Credentials stay local**: tokens and passwords are stored in plaintext in `config/sites.json` (git-ignored; the repository contains no real credentials); sub2api login tokens are cached in memory only and never written to disk
- **Data flows out only**: the app only talks to the sites you configure — no telemetry, no reporting
- **CSP**: the WebView enforces a Content Security Policy that only allows loading the app's own resources by default
- Never commit or share a `sites.json` containing real credentials

## Icon Copyright

The vendor icons in the UI (OpenAI, Anthropic Claude, xAI Grok, Moonshot Kimi) are used for identification purposes only; their trademarks and copyrights belong to the respective vendors. Icon paths are taken from [Simple Icons](https://simple-icons.org/) (CC0) and [lobe-icons](https://github.com/lobehub/lobe-icons) (MIT).

## Project Layout

```
├── src/                  # Frontend (React + Tailwind)
│   ├── components/       # Site card, channel list, filter chips, settings dialog
│   └── lib/              # Tauri invoke wrappers, model-name utilities
├── src-tauri/
│   └── src/              # Rust backend
│       ├── new2api.rs    # new2api collection (balance + group performance)
│       ├── sub2api.rs    # sub2api collection (login + channel monitors)
│       ├── config.rs     # Config load/save and path resolution
│       ├── models.rs     # Model-name normalization / family detection
│       └── state.rs      # Token cache and result cache
└── config/sites.json     # Runtime config (created locally, not committed)
```

## License

[MIT](./LICENSE)

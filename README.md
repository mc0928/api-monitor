<div align="center">

# API Monitor

### 一个桌面应用，集中监控多个 new2api / sub2api 站点的余额、渠道状态、模型倍率与成功率趋势

[![Version](https://img.shields.io/github/v/release/mc0928/api-monitor?color=blue&label=version)](https://github.com/mc0928/api-monitor/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey.svg)](https://github.com/mc0928/api-monitor/releases)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-orange.svg)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

简体中文 | [English](./README.en.md)

</div>

## 为什么选择 API Monitor？

同时使用多个 new2api / sub2api 中转站时，查余额要逐个登录网页后台，渠道是否可用、模型倍率是多少全靠翻页面，成功率出问题往往要等到请求失败才发现。

**API Monitor** 把这些站点装进一个本地桌面面板：添加站点后按设定间隔自动轮询，余额、渠道状态、模型分组倍率、成功率趋势一屏尽览；异常即时系统通知，托盘常驻后台，凭据只保存在本机。

- **一站看全部站点** — new2api / sub2api 站点的余额、渠道健康、倍率与趋势集中在一屏
- **真实倍率，不猜不编** — 展示上游模型分组倍率（如 `0.16x`），查不到就显示 `--`
- **分钟级成功率趋势** — 每次刷新追加为本地采样，折线按绿 / 黄 / 红阈值着色
- **异常即时通知** — 站点请求失败或数据异常时系统通知，无需盯屏
- **托盘常驻后台** — 单实例运行，托盘悬停即见各站点摘要
- **凭据只留本机** — 无遥测；sub2api 支持浏览器登录自动捕获令牌，仅存内存
- **应用内自动更新** — 检查、下载、安装一步完成
- **跨平台** — Windows / macOS 原生应用，基于 Tauri 2

## 功能

### 站点与渠道

- **两种站点类型** — `new2api`（令牌 + 用户 ID）与 `sub2api`（账号密码，或网页登录自动捕获令牌）
- **渠道健康一览** — 每个站点的渠道列表与模型分组倍率
- **站点排序** — 设置中拖动手动排序，或按规则自动排序
- **代理** — 全局代理带连通性测试；每个站点可单独走代理

### 模型与倍率

- **按厂商分组** — 只展示已配置的监控模型，覆盖 GPT、Claude、Grok、Kimi、Gemini、Qwen、DeepSeek
- **智能归类** — `gpt-image-*` 归入 GPT，Embedding / Reranker 归入 Qwen，DeepSeek 模型归入 DeepSeek
- **真实分组倍率** — 如 `0.16x`；上游未提供时显示 `--`

### 成功率与数据新鲜度

- **成功率趋势线** — 覆盖全部站点，绿（≥95%）、黄（≥80%）、红（<80%）
- **自动刷新** — 默认每分钟，间隔可调或关闭；各站点完成后立即更新，不等待最慢站点
- **不伪造数据** — 请求绕过缓存；部分 new2api / sub2api V2 接口只提供小时数据时，原样保留上游记录并把每次刷新追加为本地分钟采样；sub2api 模式不兼容时自动回退 V2 被动监控

### 界面与系统集成

- **系统通知** — 请求失败或数据异常即时提醒
- **托盘** — 后台常驻，悬停显示各站点摘要，单实例保护
- **深色模式 · 中英文界面**
- **应用内更新** — 设置 → 关于中检查更新，下载带进度提示，安装后自动重启

## 快速开始

1. **添加渠道**：启动应用，点击"添加渠道"
2. **选择类型**：`new2api` 填令牌和用户 ID；`sub2api` 填账号密码，或点"网页登录"在浏览器中登录后自动捕获
3. **选择监控模型**：在设置中按厂商填写要监控的模型
4. **开始监控**：点击"全部刷新"，此后按设定间隔自动刷新

> **提示**：凭据仅保存在本机，sub2api 登录令牌仅保存在内存中。

## 下载与安装

### 系统要求

- **Windows**：Windows 10 及以上
- **macOS**：macOS 12 及以上（Apple Silicon / Intel）

### Windows 用户

从 [Releases](https://github.com/mc0928/api-monitor/releases/latest) 下载 `.msi` 或 `-setup.exe` 安装包。

### macOS 用户

从 [Releases](https://github.com/mc0928/api-monitor/releases/latest) 下载 `.dmg`：Apple Silicon 选名称包含 `aarch64` 的版本，Intel 选包含 `x86_64` 的版本。

> **注**：macOS 版本暂未签名。若被 Gatekeeper 拦截，请在访达中右键应用并选择"打开"，或执行：

```bash
xattr -cr "/Applications/api-monitor.app"
```

## FAQ

<details>
<summary><strong>支持哪些站点类型？需要什么凭据？</strong></summary>

`new2api` 和 `sub2api`。new2api 填令牌和用户 ID；sub2api 填账号密码，也可以点击"网页登录"，在打开的浏览器中登录后自动捕获令牌。

</details>

<details>
<summary><strong>为什么有的站点只有小时级数据？</strong></summary>

部分 new2api / sub2api V2 接口只提供小时粒度的历史数据。应用会原样保留这些真实数据，并把每次刷新结果追加为本地分钟采样，不会伪造上游明细。sub2api 模式不兼容时会自动回退 V2 被动监控。

</details>

<details>
<summary><strong>倍率显示 <code>--</code> 是什么意思？</strong></summary>

上游没有返回该模型分组的倍率信息。应用只展示真实数据，不做猜测。

</details>

<details>
<summary><strong>我的凭据安全吗？</strong></summary>

凭据仅保存在本机 `sites.json`，sub2api 登录令牌仅保存在内存中。应用没有任何遥测，只访问已配置的站点和 GitHub Release 更新接口。请勿公开令牌、邮箱或密码。

</details>

<details>
<summary><strong>如何更新应用？</strong></summary>

设置 → 关于 → 检查更新，应用内下载并安装；也可以直接到 [Releases](https://github.com/mc0928/api-monitor/releases/latest) 页面下载。有新版本时应用会主动提示。

</details>

<details>
<summary><strong>配置文件在哪里？</strong></summary>

| 环境 | 路径 |
| --- | --- |
| Windows | `%APPDATA%\api-monitor\sites.json` |
| macOS | `~/Library/Application Support/api-monitor/sites.json` |
| 源码开发 | `config/sites.json` |

推荐在软件界面中配置，字段示例见 [`config/sites.example.json`](./config/sites.example.json)。

</details>

## 架构概览

<details>
<summary><strong>查看架构、核心模块与技术栈</strong></summary>

### 架构

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

### 核心模块

- **new2api / sub2api** — 两类站点的数据源实现；sub2api 不兼容时自动回退 V2 被动监控
- **state** — 监控调度：定时刷新，每站点完成即推送，不等最慢站点
- **persist** — 监控结果本地持久化，支撑成功率趋势线
- **http** — 代理路由与请求（绕过缓存）
- **config** — `sites.json` 读写

### 技术栈

**前端**：React 18 · TypeScript · Vite · TailwindCSS 3.4 · @dnd-kit

**后端**：Tauri 2 · Rust · serde · reqwest · tauri-plugin-notification / single-instance / updater / process / opener

**测试**：Vitest（前端）· cargo test（后端）

</details>

## 开发指南

<details>
<summary><strong>从源码构建</strong></summary>

### 环境要求

- Node.js 18+
- Rust stable
- [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)

### 开发与构建

```bash
# 安装依赖
npm ci

# 开发模式（热重载）
npm run tauri dev

# 构建安装包（输出在 src-tauri/target/release/bundle/）
npm run tauri build
```

### 测试与检查

```bash
# 前端类型检查
npm run typecheck

# 前端单元测试
npm test

# 后端测试
cargo test --manifest-path src-tauri/Cargo.toml
```

</details>

<details>
<summary><strong>项目结构</strong></summary>

```
├── src/                        # 前端（React + TypeScript）
│   ├── components/             # SiteCard / ChannelList / SettingsDialog / ProviderFilter
│   ├── lib/                    # 模型分组等逻辑（Vitest 覆盖）
│   └── types.ts
├── src-tauri/                  # 后端（Rust + Tauri 2）
│   └── src/
│       ├── new2api.rs          # new2api 数据源
│       ├── sub2api.rs          # sub2api 数据源（含 V2 被动监控回退）
│       ├── state.rs            # 监控调度与状态
│       ├── persist.rs          # 结果持久化
│       ├── http.rs             # 代理与请求
│       └── config.rs           # sites.json 读写
├── config/sites.example.json   # 配置字段示例
└── .github/workflows/          # CI 与自动发布
```

</details>

## 贡献

欢迎提 Issue 和 PR！提交前请确保：

- 类型检查通过：`npm run typecheck`
- 前端测试通过：`npm test`
- 后端测试通过：`cargo test --manifest-path src-tauri/Cargo.toml`

## 许可证

[MIT](./LICENSE) © mc0928

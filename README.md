# API Monitor

[![CI](https://github.com/mc0928/api-monitor/actions/workflows/ci.yml/badge.svg)](https://github.com/mc0928/api-monitor/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/mc0928/api-monitor)](https://github.com/mc0928/api-monitor/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

[简体中文](./README.md) | [English](./README.en.md)

一个开箱即用的桌面小工具：把你散落在各个 API 中转站的**余额、渠道状态、成功率走势**收进一块面板，挂后台持续盯着，出问题第一时间弹通知。

如果你手里有多个 new2api / sub2api 架构的中转站账号，经常遇到这些情况，这个工具就是为你做的：

- 余额快用完了才发现，请求已经在报错
- 某个渠道悄悄挂了，只能等用户投诉或自己翻网页
- 想比较几个站的分组成功率，得挨个登录、挨个开监控页

## 它能做什么

**监控**

- **new2api 站点**：配置个人访问令牌即可查看账户余额与累计请求数，并拉取模型广场各分组的成功率、延迟；不填令牌也能看公开的分组性能
- **sub2api 站点**：账号密码自动登录，拉取渠道监控列表——在线状态、7 天成功率、用量配额、渠道余额；兼容主动监控（channel-monitors）与被动监控（V2 matrix）两种部署形式
- **成功率趋势线**：所有站点展示近 24 小时逐时走势，三种数据源（new2api `series` / sub2api `timeline` / V2 `buckets`）自动适配
- **站点健康汇总**：正常 / 异常 / 未检查数量一目了然，卡片默认按成功率排名

**省心**

- **后台常驻**：关闭窗口即最小化到系统托盘继续监控，托盘悬停即可看到「N 正常 · M 异常 · 余额合计」
- **异常通知**：站点刷新失败、恢复上线、渠道新故障时弹系统通知，不用盯着窗口
- **自动刷新**：关闭 / 5 / 10 / 30 分钟可选，默认 5 分钟
- **启动即恢复**：监控快照落盘，重开应用立刻显示上次结果，不用干等首轮刷新
- **版本更新提示**：有新版发布时顶部横幅提醒，一键跳转下载

**顺手**

- 按模型族一键筛选渠道：GPT / Claude / Grok / Kimi / Gemini
- 卡片拖拽排序（或保持按成功率自动排名）
- 暗色模式（可跟随系统）、界面中英文切换
- 全局代理（Clash 混合端口）+ 按站点「走代理」开关，一键测试连通性

**放心**

- 凭据只存在你本地的 `config/sites.json`，仓库不含任何真实凭据；sub2api 登录令牌仅缓存在内存，不落盘
- 数据只出不进：仅向你自己配置的站点发请求，无遥测、无上报

## 下载安装

到 [Releases](https://github.com/mc0928/api-monitor/releases/latest) 下载对应平台安装包：

| 平台 | 文件 |
| --- | --- |
| Windows | `.msi`（推荐）或 `.exe` 安装包 |
| macOS (Apple Silicon) | `aarch64.dmg` |
| macOS (Intel) | `x86_64.dmg` |

> **macOS 首次打开**：应用未做开发者签名，双击若被 Gatekeeper 拦截，请在「访达」中对应用**右键 → 打开**确认一次；或执行 `xattr -cr /Applications/api-monitor.app` 后再打开。

安装后首次运行建议：

1. 在「设置」中添加你的站点（或直接编辑 `config/sites.json`，见下方配置说明）
2. 允许系统通知授权，否则收不到异常提醒
3. 点关闭按钮会退到托盘继续监控；彻底退出请用托盘右键菜单的「退出」

## 从源码运行

适合想自己改或参与贡献的同学：

```bash
# 环境要求：Node.js ≥ 18、Rust stable、Tauri 2 系统依赖
# https://v2.tauri.app/start/prerequisites/
npm install
npm run tauri dev     # 开发调试
npm run tauri build   # 构建安装包（产物在 src-tauri/target/release/bundle/）
```

测试：`npm run typecheck`、`npm test`（前端 vitest）、`cargo test`（Rust 解析逻辑，在 `src-tauri` 下执行）。

## 站点配置

配置文件为 `config/sites.json`（首次保存设置时自动创建，已加入 `.gitignore`，**请勿提交真实凭据**）。完整字段可参考 [`config/sites.example.json`](./config/sites.example.json)：

```json
{
  "proxy": { "url": "http://127.0.0.1:7897" },
  "monitor": {
    "models": {
      "gpt": ["gpt-5.6-sol"],
      "claude": ["claude-sonnet-5"],
      "grok": ["grok-4.6"],
      "kimi": ["kimi-k3"],
      "gemini": ["gemini-2.5-pro"]
    }
  },
  "refresh": { "interval_minutes": 5 },
  "debug": false,
  "sites": [
    {
      "id": "site-xxx",
      "name": "示例 new2api",
      "type": "new2api",
      "base_url": "https://example.com",
      "vpn": false,
      "token": "sk-...",
      "user_id": "1"
    },
    {
      "id": "site-yyy",
      "name": "示例 sub2api",
      "type": "sub2api",
      "base_url": "https://example.com",
      "vpn": true,
      "username": "you@example.com",
      "password": "..."
    }
  ]
}
```

字段速查：

- `type`：`new2api`（令牌查余额 + 广场性能）或 `sub2api`（账号密码登录拉渠道）
- `vpn`：`true` 时该站点请求经 `proxy.url` 发出（部分站点需要代理才能访问）
- `token` / `user_id`：new2api 个人访问令牌（个人设置 → 系统访问令牌）；令牌**可选**，留空仅看公开性能；新版 new-api 需要额外的 `New-Api-User` 头时填 `user_id`
- `refresh.interval_minutes`：自动刷新间隔（0 = 关闭）
- `sort_by`：`auto` 按成功率排名 / `manual` 按你的手动排序（设置中可拖动）
- `monitor.models`：分组展示优先匹配这些模型的成绩

配置文件查找顺序：当前工作目录 → exe 所在目录逐级向上，取第一个存在的 `config/sites.json`，因此双击 exe 和开发模式定位到同一份配置。

## 技术栈与工程

- 前端：React 18 + TypeScript + Tailwind CSS；后端：Rust（Tauri 2、reqwest）
- 推送 `main` / 提 PR 自动跑类型检查 + 前后端单测（[CI](./.github/workflows/ci.yml)）
- 推送 `v*` 标签自动构建 Windows + macOS 安装包并创建 Draft Release（[Release](./.github/workflows/release.yml)）

<details>
<summary>目录结构</summary>

```
├── src/                  # 前端（React + Tailwind）
│   ├── components/       # 站点卡片、渠道列表、筛选器、设置弹窗
│   └── lib/              # Tauri invoke 封装、模型名工具、i18n、错误文案
├── src-tauri/src/        # Rust 后端
│   ├── new2api.rs        # new2api 采集（余额 + 分组性能）
│   ├── sub2api.rs        # sub2api 采集（登录 + 渠道监控 + V2 回退）
│   ├── config.rs         # 配置读写与路径定位
│   ├── persist.rs        # 监控结果落盘与恢复
│   ├── models.rs         # 模型名归一化 / 归属识别
│   ├── http.rs           # 客户端构建与统一鉴权 GET
│   └── state.rs          # 令牌缓存与结果缓存
├── .github/workflows/    # CI 与发布流水线
└── config/sites.json     # 运行时配置（本地生成，不入库）
```

</details>

## 图标版权

界面中的模型厂商图标（OpenAI、Anthropic Claude、xAI Grok、Moonshot Kimi、Google Gemini）仅作指代用途，商标与版权归各自厂商所有；图标取自 [Simple Icons](https://simple-icons.org/)（CC0）与 [lobe-icons](https://github.com/lobehub/lobe-icons)（MIT）。

## License

[MIT](./LICENSE)

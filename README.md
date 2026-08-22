# API Monitor

[简体中文](./README.md) | [English](./README.en.md)

渠道状态监控桌面小程序：集中查看多个中转站点的余额、渠道状态与成功率。

前端：Tauri 2 + React 18 + TypeScript + Tailwind CSS；后端：Rust / reqwest。

## 功能

- **new2api 站点**：个人访问令牌查询账户余额、累计请求数，拉取模型广场分组性能（成功率 / 延迟）
- **sub2api 站点**：账号密码登录，拉取渠道监控列表（状态、7 天成功率、用量配额、余额）
- **统一面板**：汇总各站点正常 / 异常 / 未检查数量，默认按成功率排名
- **按模型族筛选**：GPT / Claude / Grok / Kimi / Gemini 一键过滤渠道
- **代理支持**：全局 Clash 混合代理地址（可一键测试连通性）；单个站点可勾选「走代理」
- **监控模型自定义**：new2api 分组优先展示匹配配置模型的性能数据
- **自动刷新**：可选 关闭 / 5 / 10 / 30 分钟间隔轮询（默认 5 分钟）
- **异常通知**：站点刷新失败、恢复、渠道新故障时弹系统通知（配合托盘后台运行，不用盯着窗口）
- **关闭到托盘**：点关闭窗口最小化到系统托盘继续监控，托盘菜单可显示主窗口或退出
- **快照持久化**：监控结果落盘，重启应用立即恢复上次状态，无需等待首次刷新
- **暗色模式**：一键切换或跟随系统；界面中英文切换
- **new2api 令牌可选**：不填令牌也能看模型广场性能数据（余额不可用）；支持新版 new-api 的 `New-Api-User` 用户 ID 头
- **调试模式**：可选在结果中保留原始响应片段，便于排查字段解析

## 快速开始

### 环境要求

- Node.js ≥ 18
- Rust stable（[rustup](https://rustup.rs/)）
- [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)（Windows 需要 WebView2，Win10/11 已自带）

### 开发运行

```bash
npm install
npm run tauri dev
```

### 构建发布包

```bash
npm run tauri build
```

产物位于 `src-tauri/target/release/bundle/`。

### 测试

```bash
npm run typecheck     # 前端类型检查
npm test              # 前端单元测试（vitest）
cargo test            # 后端解析逻辑单元测试（src-tauri 目录下）
```

## 使用提示

- 点击窗口关闭按钮会**最小化到系统托盘**，监控与通知继续工作；退出请用托盘图标右键菜单的「退出」
- 首次运行弹出通知授权时请允许，否则收不到异常提醒
- 自动刷新间隔在「设置」中调整：关闭 / 5 / 10 / 30 分钟

## 配置

配置文件为 `config/sites.json`，应用首次保存设置时自动创建（已加入 `.gitignore`，请勿提交真实凭据）。仓库内附占位模板 [`config/sites.example.json`](./config/sites.example.json) 可参考字段：

```json
{
  "proxy": { "url": "http://127.0.0.1:7897" },
  "monitor": {
    "models": {
      "gpt": ["gpt-5.6-sol", "gpt-5.6-terra"],
      "claude": ["claude-sonnet-5", "claude-opus-5"],
      "grok": ["grok-4.6"],
      "kimi": ["kimi-k3"],
      "gemini": ["gemini-2.5-pro", "gemini-2.5-flash"]
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

- `proxy.url`：Clash 混合代理地址，留空则不使用代理
- `sites[].type`：`new2api`（令牌查余额）或 `sub2api`（登录拉渠道）
- `sites[].vpn`：为 `true` 时该站点的请求经代理发出
- `sites[].token`：new2api 个人访问令牌（个人设置 → 系统访问令牌）；**可选**，留空仅拉取公开的模型广场性能数据
- `sites[].user_id`：new2api 可选用户 ID，新版 new-api 鉴权需要 `New-Api-User` 请求头
- `refresh.interval_minutes`：自动刷新间隔（0 = 关闭，可选 0 / 5 / 10 / 30）
- `debug`：调试模式，结果中保留原始响应片段

配置文件查找顺序：当前工作目录 → exe 所在目录逐级向上，取第一个已存在的 `config/sites.json`，这样无论从项目根目录还是直接双击 exe 启动，都能定位到同一份配置。

## 构建与发布

- 推送到 `main` 或提 PR 时，GitHub Actions 自动运行类型检查、前端单测与后端单测（见 [CI](./.github/workflows/ci.yml)）
- 推送 `v*` 标签时自动构建 Windows 安装包并创建 Draft Release（见 [Release](./.github/workflows/release.yml)），确认无误后手动发布

## 安全性

- **凭据仅存本地**：令牌、账号密码明文保存在 `config/sites.json`（已加入 `.gitignore`，仓库不包含任何真实凭据）；sub2api 登录令牌只缓存在内存中，不落盘
- **数据只出不进**：应用仅向你配置的站点发请求，无遥测、无上报
- **CSP**：WebView 开启了内容安全策略，默认仅允许加载应用自身资源
- 请勿将包含真实凭据的 `sites.json` 提交到任何仓库或分享给他人

## 图标版权

界面中的模型厂商图标（OpenAI、Anthropic Claude、xAI Grok、Moonshot Kimi）仅作指代用途，其商标与版权归各自厂商所有；图标路径取自 [Simple Icons](https://simple-icons.org/)（CC0）与 [lobe-icons](https://github.com/lobehub/lobe-icons)（MIT）。

## 目录结构

```
├── src/                  # 前端（React + Tailwind）
│   ├── components/       # 站点卡片、渠道列表、筛选器、设置弹窗
│   └── lib/              # Tauri invoke 封装、模型名工具、i18n、错误文案
├── src-tauri/
│   └── src/              # Rust 后端
│       ├── new2api.rs    # new2api 采集（余额 + 分组性能）
│       ├── sub2api.rs    # sub2api 采集（登录 + 渠道监控）
│       ├── config.rs     # 配置读写与路径定位
│       ├── persist.rs    # 监控结果落盘与恢复
│       ├── models.rs     # 模型名归一化 / 归属识别
│       ├── http.rs       # 客户端构建与统一鉴权 GET
│       └── state.rs      # 令牌缓存与结果缓存
├── .github/workflows/    # CI 与发布流水线
└── config/sites.json     # 运行时配置（本地生成，不入库）
```

## License

[MIT](./LICENSE)

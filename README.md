# API Monitor

渠道状态监控桌面小程序：集中查看多个中转站点的余额、渠道状态与成功率。

前端：Tauri 2 + React 18 + TypeScript + Tailwind CSS；后端：Rust / reqwest。

## 功能

- **new2api 站点**：个人访问令牌查询账户余额、累计请求数，拉取模型广场分组性能（成功率 / 延迟）
- **sub2api 站点**：账号密码登录，拉取渠道监控列表（状态、7 天成功率、用量配额、余额）
- **统一面板**：汇总各站点正常 / 异常 / 未检查数量，默认按成功率排名
- **按模型族筛选**：GPT / Claude / Grok / Kimi 一键过滤渠道
- **代理支持**：全局 Clash 混合代理地址（可一键测试连通性）；单个站点可勾选「走代理」
- **监控模型自定义**：new2api 分组优先展示匹配配置模型的性能数据

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
cargo test            # 后端解析逻辑单元测试（src-tauri 目录下）
```

## 配置

配置文件为 `config/sites.json`，应用首次保存设置时自动创建（已加入 `.gitignore`，请勿提交真实凭据）：

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
      "name": "示例 new2api",
      "type": "new2api",
      "base_url": "https://example.com",
      "vpn": false,
      "token": "sk-..."
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

配置文件查找顺序：当前工作目录 → exe 所在目录逐级向上，取第一个已存在的 `config/sites.json`，这样无论从项目根目录还是直接双击 exe 启动，都能定位到同一份配置。

## 目录结构

```
├── src/                  # 前端（React + Tailwind）
│   ├── components/       # 站点卡片、渠道列表、筛选器、设置弹窗
│   └── lib/              # Tauri invoke 封装、模型名工具
├── src-tauri/
│   └── src/              # Rust 后端
│       ├── new2api.rs    # new2api 采集（余额 + 分组性能）
│       ├── sub2api.rs    # sub2api 采集（登录 + 渠道监控）
│       ├── config.rs     # 配置读写与路径定位
│       ├── models.rs     # 模型名归一化 / 归属识别
│       └── state.rs      # 令牌缓存与结果缓存
└── config/sites.json     # 运行时配置（本地生成，不入库）
```

## License

[MIT](./LICENSE)

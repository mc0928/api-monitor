# API Monitor

[![CI](https://github.com/mc0928/api-monitor/actions/workflows/ci.yml/badge.svg)](https://github.com/mc0928/api-monitor/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/mc0928/api-monitor)](https://github.com/mc0928/api-monitor/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

[简体中文](./README.md) | [English](./README.en.md)

用于集中查看多个 new2api / sub2api 站点的余额、渠道状态、模型倍率、成功率和趋势。

## 功能

- 展示真实模型分组倍率，例如 `0.16x`；未知倍率显示 `--`。
- 只展示已配置的监控模型，支持 GPT、Claude、Grok、Kimi、Gemini、Qwen 和 Seedream。
- `gpt-image-*` 归入 GPT；Embedding / Reranker 可归入 Qwen；Seedream 图片模型归入 Seedream。
- 成功率折线按绿（≥95%）、黄（≥80%）、红（<80%）显示。
- 默认每分钟刷新；各站点完成后立即更新，不等待最慢站点。
- 支持通知、托盘、深色模式、中英文、站点排序和按站点代理。

## 下载

从 [v0.1.0 Release](https://github.com/mc0928/api-monitor/releases/tag/v0.1.0) 下载：

| 平台 | 安装包 |
| --- | --- |
| Windows x64 | `.msi` 或 `-setup.exe` |
| macOS Apple Silicon | 名称包含 `aarch64` 的 `.dmg` |
| macOS Intel | 名称包含 `x86_64` 的 `.dmg` |

macOS 版本暂未签名。若被 Gatekeeper 拦截，请在访达中右键应用并选择“打开”，或执行：

```bash
xattr -cr "/Applications/api-monitor.app"
```

## 使用

1. 启动软件，点击“添加渠道”。
2. 选择 `new2api` 或 `sub2api`，填写站点地址和凭据。
3. 在设置中选择模型、代理和刷新间隔。
4. 点击“全部刷新”。

凭据仅保存在本机，sub2api 登录令牌仅保存在内存中。

## 刷新说明

软件会在每次请求中避免缓存，并按设置的分钟间隔刷新。主动监控通常可提供分钟级记录；部分 new2api 或 sub2api V2 接口只提供小时数据，软件会保留这些真实数据，并将每次刷新结果追加为本地分钟采样，不会伪造上游明细。

## 配置位置

| 环境 | 路径 |
| --- | --- |
| Windows | `%APPDATA%\api-monitor\sites.json` |
| macOS | `~/Library/Application Support/api-monitor/sites.json` |
| 源码开发 | `config/sites.json` |

推荐在软件界面中配置。字段示例见 [`config/sites.example.json`](./config/sites.example.json)。

## 源码运行

需要 Node.js 18+、Rust stable 和 [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)。

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

安装包输出在 `src-tauri/target/release/bundle/`。

## 隐私与许可

软件没有遥测，只访问已配置站点和 GitHub Release 更新接口。请勿公开令牌、邮箱或密码。

[MIT License](./LICENSE)

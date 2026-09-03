mod config;
mod http;
mod models;
mod new2api;
mod persist;
mod state;
mod sub2api;

use std::time::Instant;

use config::{AppConfig, SiteConfig, SiteType};
use state::{AppState, SiteResult};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// 读取配置（含代理地址与站点列表）
#[tauri::command]
fn get_config() -> Result<AppConfig, String> {
    config::load_config()
}

/// 整体保存配置（设置对话框写回 sites.json）
#[tauri::command]
fn save_config(cfg: AppConfig, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for site in &cfg.sites {
        if site.id.trim().is_empty() {
            return Err("存在未填写 ID 的站点".to_string());
        }
        if !seen.insert(site.id.trim().to_string()) {
            return Err(format!("站点 ID「{}」重复，请更换 ID", site.id));
        }
        if site.base_url.trim().is_empty() || !site.base_url.starts_with("http") {
            return Err(format!("站点「{}」的 URL 无效", site.name));
        }
    }
    config::save_config(&cfg)?;
    // 仅清理已删除站点的缓存；保留站点的登录令牌不动（改密/换号后 401 会自动清缓存重登）
    let keep: Vec<String> = cfg.sites.iter().map(|s| s.id.clone()).collect();
    state.prune_sites(&keep);
    Ok(())
}

/// 刷新单个站点
#[tauri::command]
async fn refresh_site(id: String, state: tauri::State<'_, AppState>) -> Result<SiteResult, String> {
    let cfg = config::load_config()?;
    let site = cfg
        .sites
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| format!("未找到站点: {id}"))?;
    let result = refresh_one_cached(
        &site,
        &cfg.proxy.url,
        None,
        &cfg.monitor.models,
        cfg.debug,
        &state,
    )
    .await;
    // 持久化当前快照，失败仅记录日志，不影响返回
    if let Err(e) = persist::save(&state.results_map()) {
        eprintln!("持久化结果失败: {e}");
    }
    update_tray_tooltip(&state);
    Ok(result)
}

/// 并发刷新全部站点（互不阻塞），按配置顺序返回结果
#[tauri::command]
async fn refresh_all(state: tauri::State<'_, AppState>) -> Result<Vec<SiteResult>, String> {
    let cfg = config::load_config()?;
    let inner = state.inner().clone();

    // 客户端构建失败（如代理地址非法）直接返回明确错误，不再静默降级
    let direct = http::build_client(None)?;
    let via_proxy = http::build_client(Some(&cfg.proxy.url))?;
    let debug = cfg.debug;

    let mut handles = Vec::new();
    for site in cfg.sites {
        let app_state = inner.clone();
        let shared = if site.vpn {
            via_proxy.clone()
        } else {
            direct.clone()
        };
        let models = cfg.monitor.models.clone();
        handles.push((
            site.clone(),
            tauri::async_runtime::spawn(async move {
                refresh_one_shared(&site, &shared, &models, debug, &app_state).await
            }),
        ));
    }

    // 单个任务失败只影响该站点，不影响其余结果
    let mut results = Vec::new();
    for (site, handle) in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => {
                let result = SiteResult::error(&site, format!("内部错误: {e}"));
                state.set_result(result.clone());
                results.push(result);
            }
        }
    }
    // 持久化当前快照，失败仅记录日志，不影响返回
    if let Err(e) = persist::save(&state.results_map()) {
        eprintln!("持久化结果失败: {e}");
    }
    update_tray_tooltip(&state);
    Ok(results)
}

/// 读取各站点最近一次结果缓存（按配置顺序）
#[tauri::command]
fn get_results(state: tauri::State<'_, AppState>) -> Vec<SiteResult> {
    let Ok(map) = state.results.lock() else {
        return Vec::new();
    };
    match config::load_config() {
        Ok(cfg) => cfg
            .sites
            .iter()
            .filter_map(|s| map.get(&s.id).cloned())
            .collect(),
        Err(_) => map.values().cloned().collect(),
    }
}

/// 测试代理连通性（经代理访问轻量探测地址）
#[tauri::command]
async fn test_proxy() -> Result<String, String> {
    let cfg = config::load_config()?;
    let client = http::build_client(Some(&cfg.proxy.url))?;
    let start = Instant::now();
    let response = client
        .get("https://cp.cloudflare.com")
        .send()
        .await
        .map_err(|e| format!("代理请求失败（请确认 Clash 已开启混合代理）: {e}"))?;
    Ok(format!(
        "代理可用（HTTP {}，{}ms）",
        response.status(),
        start.elapsed().as_millis()
    ))
}

/// 内嵌登录窗口注入脚本：劫持 fetch / XHR，捕获登录响应或后续请求头里的
/// Bearer 令牌，通过导航到不可达主机 apimon-token.local 回传给 Rust 端拦截。
/// 捕获规则（防止残留的过期会话让窗口刚打开就被误关）：
/// - 上报前对 JWT 做本地过期检查——过期令牌直接丢弃，页面自己会走 401 -> 重新登录；
/// - 请求头 Bearer 必须等该请求返回 200 才上报（公开接口对无效令牌也 200，
///   依赖上一条 JWT 检查兜底）；
/// - 响应体里的 token 字段只认登录/认证类 URL，普通接口误报不触发；
/// - 仅主框架生效：Turnstile 等 iframe 内部请求与登录无关，劫持反而可能弄坏验证组件。
const WEB_LOGIN_SCRIPT: &str = r#"(function () {
  if (window.__apimonHooked) return;
  window.__apimonHooked = true;
  try {
    if (window.top !== window) return;
  } catch (e) {
    return;
  }

  // JWT 本地过期检查：webview 的 localStorage 可能残留过期令牌，SPA 会带着它请求
  // 公开接口（公开接口恒 200，不能证明会话有效），过期的令牌直接丢弃不上报，
  // 让页面自己走 401 -> 重新登录，避免窗口刚打开就被误关
  function jwtExpired(token) {
    try {
      var parts = String(token).split(".");
      if (parts.length !== 3) return false;
      var b64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
      while (b64.length % 4) b64 += "=";
      var payload = JSON.parse(atob(b64));
      return typeof payload.exp === "number" && payload.exp * 1000 <= Date.now();
    } catch (e) {
      return false;
    }
  }

  function report(token, refreshToken) {
    try {
      if (jwtExpired(token)) return;
      window.__apimonSent = true;
      location.href =
        "http://apimon-token.local/#" +
        encodeURIComponent(
          JSON.stringify({ auth_token: token, refresh_token: refreshToken || null })
        );
    } catch (e) {}
  }

  // 从登录响应（兼容 data 包裹）里提取令牌
  function reportFromObject(obj) {
    try {
      var data =
        obj && typeof obj === "object" && obj.data && typeof obj.data === "object"
          ? obj.data
          : obj;
      if (!data) return false;
      var token = data.auth_token || data.access_token || data.token;
      if (typeof token !== "string" || token.length < 16) return false;
      report(token, typeof data.refresh_token === "string" ? data.refresh_token : null);
      return true;
    } catch (e) {
      return false;
    }
  }

  function bearerFromHeaders(headers) {
    try {
      if (!headers) return null;
      var value = null;
      if (typeof headers.get === "function") {
        value = headers.get("Authorization") || headers.get("authorization");
      } else {
        var keys = Object.keys(headers);
        for (var i = 0; i < keys.length; i++) {
          if (String(keys[i]).toLowerCase() === "authorization") {
            value = headers[keys[i]];
            break;
          }
        }
      }
      var match = /^Bearer\s+(\S{16,})/.exec(String(value || ""));
      return match ? match[1] : null;
    } catch (e) {
      return null;
    }
  }

  // 登录/认证类接口（含相对路径），如 /api/v1/auth/login、/auth/linuxdo/callback
  function isAuthUrl(url) {
    var u = String(url || "");
    return (
      u.indexOf("/auth/") !== -1 ||
      /\/(login|logout|callback|oauth|token|session|signin|sso)([\/?#]|$)/i.test(u)
    );
  }

  // 已登录的会话不发登录请求，但页面加载后会携带 Bearer 调用 API：
  // 记下 Bearer，等响应 200 确认会话仍有效再上报
  var origFetch = window.fetch;
  if (typeof origFetch === "function") {
    window.fetch = function (input, init) {
      var pendingBearer = null;
      if (!window.__apimonSent) {
        var headers =
          (init && init.headers) || (input && typeof input === "object" ? input.headers : null);
        pendingBearer = bearerFromHeaders(headers);
      }
      var url =
        typeof input === "string" ? input : (input && typeof input === "object" ? input.url : "");
      return origFetch.apply(this, arguments).then(function (resp) {
        if (window.__apimonSent || resp.status !== 200) return resp;
        if (pendingBearer) report(pendingBearer, null);
        try {
          var type =
            resp.headers && resp.headers.get ? resp.headers.get("content-type") || "" : "";
          if (type.indexOf("json") === -1 || !isAuthUrl(url)) return resp;
          resp.clone().text().then(function (text) {
            try {
              reportFromObject(JSON.parse(text));
            } catch (e) {}
          });
        } catch (e) {}
        return resp;
      });
    };
  }

  // axios 等基于 XHR 的请求库：Bearer 同样延迟到 load + 200 再上报
  var origSend = XMLHttpRequest.prototype.send;
  var origSetHeader = XMLHttpRequest.prototype.setRequestHeader;
  XMLHttpRequest.prototype.setRequestHeader = function (name, value) {
    if (!window.__apimonSent && String(name).toLowerCase() === "authorization") {
      var match = /^Bearer\s+(\S{16,})/.exec(String(value));
      if (match) this.__apimonBearer = match[1];
    }
    return origSetHeader.apply(this, arguments);
  };
  XMLHttpRequest.prototype.send = function () {
    var xhr = this;
    if (!window.__apimonSent) {
      xhr.addEventListener("load", function () {
        try {
          if (window.__apimonSent || xhr.status !== 200) return;
          if (xhr.__apimonBearer) {
            report(xhr.__apimonBearer, null);
            return;
          }
          var type = xhr.getResponseHeader("content-type") || "";
          if (type.indexOf("json") === -1) return;
          if (!isAuthUrl(xhr.responseURL)) return;
          reportFromObject(JSON.parse(xhr.responseText));
        } catch (e) {}
      });
    }
    return origSend.apply(this, arguments);
  };
})();"#;

/// %XX 解码（on_navigation 收到的 fragment 保持编码形态）
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let hex = |b: u8| -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    };
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 打开内嵌浏览器登录窗口（sub2api 专用）：用户在窗口内正常完成登录/人机验证，
/// 注入脚本自动捕获令牌并写入缓存，之后刷新不再受 Turnstile 阻拦。
/// 注意：验证由用户本人完成，程序不绕过人机验证本身。
/// 必须为 async：Windows 上同步命令在主线程执行，创建 WebView 窗口会因
/// WebView2 初始化等待消息循环而死锁（官方文档要求 command 内建窗口用 async）
#[tauri::command]
async fn open_web_login(
    site: SiteConfig,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if site.site_type != SiteType::Sub2api {
        return Err("仅 sub2api 站点支持内嵌浏览器登录".to_string());
    }
    let label = format!("web-login-{}", site.id);
    // 已有登录窗口时聚焦复用，避免标签冲突
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }
    let base = site.base_url.trim_end_matches('/');
    let login_url = format!("{base}/login");
    let url: tauri::Url = login_url
        .parse()
        .map_err(|_| format!("无效的站点地址：{login_url}"))?;

    let nav_state = state.inner().clone();
    let nav_site_id = site.id.clone();
    // on_navigation 只回传 URL，窗口操作改由 AppHandle 完成
    let nav_app = app.clone();
    let nav_label = label.clone();
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(url))
        .title(format!("登录 · {}", site.name))
        .inner_size(420.0, 680.0)
        .min_inner_size(360.0, 500.0)
        .initialization_script(WEB_LOGIN_SCRIPT)
        .on_navigation(move |url| {
            // 仅拦截令牌回传导航，其余导航放行
            if url.host_str() != Some("apimon-token.local") {
                return true;
            }
            let token = url
                .fragment()
                .and_then(|f| serde_json::from_str::<serde_json::Value>(&percent_decode(f)).ok())
                .and_then(|v| sub2api::parse_web_token(&v));
            let ok = token.is_some();
            // 令牌写入缓存并落盘 tokens.json，重启后免登录；不回传前端
            if let Some(token) = token {
                nav_state.set_token(&nav_site_id, token);
            }
            let _ = nav_app.emit(
                "web-login-done",
                serde_json::json!({ "id": nav_site_id, "ok": ok }),
            );
            if let Some(win) = nav_app.get_webview_window(&nav_label) {
                let _ = win.close();
            }
            false
        })
        .build()
        .map_err(|e| format!("打开登录窗口失败：{e}"))?;
    Ok(())
}

/// 按 vpn 开关选择直连 / 代理客户端（传入 None 时在此构建），结果写入缓存
async fn refresh_one_cached(
    site: &SiteConfig,
    proxy_url: &str,
    shared: Option<reqwest::Client>,
    models: &config::MonitorModels,
    debug: bool,
    state: &AppState,
) -> SiteResult {
    let client = match shared {
        Some(c) => c,
        None => {
            let proxy = if site.vpn { Some(proxy_url) } else { None };
            match http::build_client(proxy) {
                Ok(c) => c,
                Err(e) => {
                    let result = SiteResult::error(site, e);
                    state.set_result(result.clone());
                    return result;
                }
            }
        }
    };
    refresh_one_shared(site, &client, models, debug, state).await
}

/// 用已构建的客户端检查站点，结果写入缓存
async fn refresh_one_shared(
    site: &SiteConfig,
    client: &reqwest::Client,
    models: &config::MonitorModels,
    debug: bool,
    state: &AppState,
) -> SiteResult {
    let mut result = match site.site_type {
        SiteType::New2api => new2api::check(client, site, models).await,
        SiteType::Sub2api => sub2api::check(client, site, models, state).await,
    };
    // 非调试模式剥离原始响应片段，避免敏感/冗长数据进缓存与落盘
    if !debug {
        result.raw = None;
    }
    state.merge_and_set_result(result)
}

/// 显示主窗口并聚焦（窗口不存在等情况直接忽略）
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// 用当前结果汇总刷新托盘悬停提示：N 正常 · M 异常 · 余额合计
fn update_tray_tooltip(state: &AppState) {
    let Some(app) = state.app_handle.get() else {
        return;
    };
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };
    let map = state.results_map();
    let sites = config::load_config().map(|c| c.sites).unwrap_or_default();
    let checked = sites
        .iter()
        .filter(|s| map.get(&s.id).is_some_and(|r| r.checked_at > 0))
        .count();
    let ok = sites
        .iter()
        .filter(|s| map.get(&s.id).is_some_and(|r| r.ok))
        .count();
    let fail = checked - ok;
    let balance: f64 = sites
        .iter()
        .filter_map(|s| map.get(&s.id))
        .filter_map(|r| r.balance_usd)
        .sum();
    let tip: String = format!("API Monitor · {ok} 正常 · {fail} 异常 · 余额 ${balance:.2}");
    let _ = tray.set_tooltip(Some(tip));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::with_persisted();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // 应用内更新：检查 / 下载 / 安装 / 重启（同 cc-switch 方案）
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // 单实例：再次启动时聚焦已有主窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            refresh_site,
            refresh_all,
            get_results,
            test_proxy,
            open_web_login
        ])
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            // 记录 AppHandle 供托盘摘要更新使用；并立即用持久化结果刷新一次 tooltip
            let _ = app.state::<AppState>().app_handle.set(app.handle().clone());
            update_tray_tooltip(app.state::<AppState>().inner());

            // 托盘菜单：显示主窗口 / 退出
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("API Monitor")
                .show_menu_on_left_click(false)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // 左键单击抬起时显示主窗口
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭主窗口 = 隐藏到托盘后台；其余窗口（如内嵌登录）正常销毁
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

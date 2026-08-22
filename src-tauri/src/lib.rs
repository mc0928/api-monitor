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
use tauri::Manager;

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
    let keep: Vec<String> = cfg.sites.iter().map(|s| s.id.clone()).collect();
    for id in &keep {
        state.clear_token(id);
    }
    state.prune_sites(&keep);
    Ok(())
}

/// 刷新单个站点
#[tauri::command]
async fn refresh_site(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<SiteResult, String> {
    let cfg = config::load_config()?;
    let site = cfg
        .sites
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| format!("未找到站点: {id}"))?;
    let result =
        refresh_one_cached(&site, &cfg.proxy.url, None, &cfg.monitor.models, cfg.debug, &state)
            .await;
    // 持久化当前快照，失败仅记录日志，不影响返回
    if let Err(e) = persist::save(&state.results_map()) {
        eprintln!("持久化结果失败: {e}");
    }
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
        SiteType::Sub2api => sub2api::check(client, site, state).await,
    };
    // 非调试模式剥离原始响应片段，避免敏感/冗长数据进缓存与落盘
    if !debug {
        result.raw = None;
    }
    state.set_result(result.clone());
    result
}

/// 显示主窗口并聚焦（窗口不存在等情况直接忽略）
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::with_persisted();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            refresh_site,
            refresh_all,
            get_results,
            test_proxy
        ])
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

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
            // 关闭窗口 = 隐藏到托盘后台，刷新与通知仍可工作
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

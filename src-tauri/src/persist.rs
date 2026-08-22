//! 结果持久化：把最近一次各站点监控结果快照落盘（与 sites.json 同目录的 last-results.json），
//! 应用重启时先装载该快照，避免界面空白直到首次刷新完成。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::state::SiteResult;

static SAVE_LOCK: Mutex<()> = Mutex::new(());

/// 结果文件位置：config_path() 的父目录下的 last-results.json（config_path 可能是相对路径，直接用）
fn results_path() -> Result<PathBuf, String> {
    let base = crate::config::config_path()?;
    let parent = base
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(parent.join("last-results.json"))
}

/// 读取持久化的结果快照；任何错误（文件不存在 / 格式错误）都返回空 map，不报错
pub fn load() -> HashMap<String, SiteResult> {
    let Ok(path) = results_path() else {
        return HashMap::new();
    };
    let Ok(raw) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// 紧凑 JSON 原子写入：先写 last-results.json.tmp，再 rename 覆盖目标文件
pub fn save(map: &HashMap<String, SiteResult>) -> Result<(), String> {
    let _guard = SAVE_LOCK
        .lock()
        .map_err(|_| "结果保存锁已损坏".to_string())?;
    let path = results_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建结果目录失败: {e}"))?;
    }
    let raw = serde_json::to_string(map).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw).map_err(|e| format!("写入结果文件失败（{}）: {e}", tmp.display()))?;
    // Windows 上 rename 到已存在目标会报错，先删除旧文件（不存在则忽略）
    let _ = fs::remove_file(&path);
    fs::rename(&tmp, &path).map_err(|e| format!("替换结果文件失败（{}）: {e}", path.display()))
}

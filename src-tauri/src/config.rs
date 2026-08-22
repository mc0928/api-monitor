use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 站点类型：new2api（令牌查余额）/ sub2api（登录拉渠道监控）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SiteType {
    New2api,
    Sub2api,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    #[serde(default)]
    pub url: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:7897".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub site_type: SiteType,
    pub base_url: String,
    /// 为 true 时该站点的请求经代理（Clash 混合代理）发出
    #[serde(default)]
    pub vpn: bool,
    /// new2api 站点：个人访问令牌（可选；留空则仅拉取公开的模型广场性能数据）
    #[serde(default)]
    pub token: Option<String>,
    /// new2api 站点：New-Api-User 请求头用的用户 ID（新版 new-api 鉴权需要）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// sub2api 站点：登录账号（邮箱）
    #[serde(default)]
    pub username: Option<String>,
    /// sub2api 站点：登录密码
    #[serde(default)]
    pub password: Option<String>,
}

/// 自动刷新间隔（分钟）：0 = 关闭，可选 0 / 1 / 2 / 5 / 10 / 30，默认 1
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RefreshConfig {
    #[serde(default = "default_interval_minutes")]
    pub interval_minutes: u32,
}

fn default_interval_minutes() -> u32 {
    1
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            interval_minutes: default_interval_minutes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorModels {
    #[serde(default)]
    pub gpt: Vec<String>,
    #[serde(default)]
    pub claude: Vec<String>,
    #[serde(default)]
    pub grok: Vec<String>,
    #[serde(default)]
    pub kimi: Vec<String>,
    #[serde(default)]
    pub gemini: Vec<String>,
    #[serde(default = "default_qwen_models")]
    pub qwen: Vec<String>,
    #[serde(default = "default_seedream_models")]
    pub seedream: Vec<String>,
}

fn default_qwen_models() -> Vec<String> {
    vec!["Qwen/Qwen3-Embedding-0.6B".into()]
}

fn default_seedream_models() -> Vec<String> {
    vec!["byte-plus-seedream-4-5".into()]
}

impl Default for MonitorModels {
    fn default() -> Self {
        Self {
            gpt: vec!["gpt-5.6-sol".into(), "gpt-5.6-terra".into()],
            claude: vec!["claude-sonnet-5".into(), "claude-opus-5".into()],
            grok: vec!["grok-4.6".into()],
            kimi: vec!["kimi-k3".into()],
            gemini: vec!["gemini-2.5-pro".into(), "gemini-2.5-flash".into()],
            qwen: default_qwen_models(),
            seedream: default_seedream_models(),
        }
    }
}

impl MonitorModels {
    fn is_empty(&self) -> bool {
        self.gpt.is_empty()
            && self.claude.is_empty()
            && self.grok.is_empty()
            && self.kimi.is_empty()
            && self.gemini.is_empty()
            && self.qwen.is_empty()
            && self.seedream.is_empty()
    }

    /// 全部配置模型名（去重、去空白）；全空时回退默认
    pub fn all_names(&self) -> Vec<String> {
        let src = if self.is_empty() {
            Self::default()
        } else {
            self.clone()
        };
        let mut names: Vec<String> = Vec::new();
        for name in src
            .gpt
            .iter()
            .chain(&src.claude)
            .chain(&src.grok)
            .chain(&src.kimi)
            .chain(&src.gemini)
            .chain(&src.qwen)
            .chain(&src.seedream)
        {
            let trimmed = name.trim();
            if !trimmed.is_empty() && !names.iter().any(|n| n == trimmed) {
                names.push(trimmed.to_string());
            }
        }
        names
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonitorConfig {
    #[serde(default)]
    pub models: MonitorModels,
}

/// 卡片排序方式：auto（按成功率自动）/ manual（按配置顺序，设置里可拖动调整）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    Auto,
    Manual,
}

impl Default for SortBy {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub sites: Vec<SiteConfig>,
    #[serde(default)]
    pub monitor: MonitorConfig,
    #[serde(default)]
    pub refresh: RefreshConfig,
    /// 调试模式：结果中保留原始响应片段（raw 字段）
    #[serde(default)]
    pub debug: bool,
    #[serde(default)]
    pub sort_by: SortBy,
}

fn user_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library").join("Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        });

    root.map(|root| root.join("api-monitor").join("sites.json"))
}

/// 开发环境优先使用项目内的 config/sites.json；安装环境使用系统用户配置目录。
pub fn config_path() -> Result<PathBuf, String> {
    let rel = PathBuf::from("config").join("sites.json");

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(&rel));
    }
    if let Ok(exe) = std::env::current_exe() {
        // exe 位于 src-tauri/target/{debug,release}（测试时在 deps 子目录），逐级向上最多找 5 层
        let mut dir = exe.parent().map(PathBuf::from);
        for _ in 0..5 {
            let Some(d) = dir else { break };
            candidates.push(d.join(&rel));
            dir = d.parent().map(PathBuf::from);
        }
    }

    Ok(candidates
        .into_iter()
        .find(|p| p.is_file())
        .or_else(user_config_path)
        .unwrap_or(rel))
}

pub fn load_config() -> Result<AppConfig, String> {
    let path = config_path()?;
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(AppConfig::default()),
        Err(e) => return Err(format!("读取配置文件失败（{}）: {e}", path.display())),
    };
    serde_json::from_str(&raw).map_err(|e| format!("配置文件格式错误: {e}"))
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 config 目录失败: {e}"))?;
    }
    let raw = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| format!("写入配置文件失败（{}）: {e}", path.display()))
}

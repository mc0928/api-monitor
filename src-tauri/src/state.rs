use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{SiteConfig, SiteType};

/// 单个用量窗口（对齐 sub2api MonitorQuotaTier）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaTier {
    pub window: String,
    pub label: Option<String>,
    pub used_percent: f64,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub reset_at: Option<String>,
}

/// 渠道余额（单币种）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelBalance {
    pub currency: String,
    pub balance: f64,
}

/// 趋势线上的一个点：t 为时间标签（bucket 起始 ISO 时间），v 为成功率百分数（0~100）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    pub t: String,
    pub v: f64,
}

/// 单个渠道的状态（new2api / sub2api 站点通用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub name: String,
    pub online: bool,
    pub detail: String,
    /// operational | degraded | failed | unknown
    pub status: String,
    pub plan_level: Option<String>,
    /// gpt | claude | grok | kimi | gemini
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// 统一后的模型名，如 claude-sonnet-4-6
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub availability: Option<f64>,
    pub latency_ms: Option<i64>,
    pub tiers: Vec<QuotaTier>,
    pub balances: Vec<ChannelBalance>,
    /// 成功率趋势（V2 被动监控的逐时桶数据）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trend: Option<Vec<TrendPoint>>,
}

/// 单个站点最近一次检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteResult {
    pub id: String,
    pub name: String,
    pub site_type: SiteType,
    pub ok: bool,
    /// 毫秒时间戳
    pub checked_at: u64,
    pub error: Option<String>,
    /// 非致命的补充提示（如令牌无权限拉取渠道列表）
    pub note: Option<String>,
    /// new2api：quota 原值（500000 = $1）
    pub quota: Option<i64>,
    /// 站点余额（美元）：new2api 账户余额，或 sub2api 渠道 USD 合计
    pub balance_usd: Option<f64>,
    /// new2api：累计请求数
    pub request_count: Option<u64>,
    /// 渠道状态列表
    pub channels: Vec<ChannelStatus>,
    /// 保留原始响应片段，便于调试字段解析
    pub raw: Option<String>,
}

impl SiteResult {
    pub fn base(site: &SiteConfig, ok: bool, error: Option<String>) -> Self {
        Self {
            id: site.id.clone(),
            name: site.name.clone(),
            site_type: site.site_type,
            ok,
            checked_at: now_millis(),
            error,
            note: None,
            quota: None,
            balance_usd: None,
            request_count: None,
            channels: Vec::new(),
            raw: None,
        }
    }

    pub fn error(site: &SiteConfig, message: String) -> Self {
        Self::base(site, false, Some(message))
    }
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// sub2api 站点的令牌缓存（仅内存，不落盘）
#[derive(Debug, Clone)]
pub struct TokenCache {
    pub auth_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
    pub balance: Option<f64>,
}

/// 全局状态：各站 token 缓存 + 最近一次结果缓存 + 应用句柄（托盘等需要）
#[derive(Default, Clone)]
pub struct AppState {
    pub tokens: Arc<Mutex<HashMap<String, TokenCache>>>,
    pub results: Arc<Mutex<HashMap<String, SiteResult>>>,
    /// setup 阶段写入，供刷新后更新托盘 tooltip 等使用
    pub app_handle: std::sync::OnceLock<tauri::AppHandle>,
}

impl AppState {
    /// 以持久化的上次结果快照构造（重启后立即恢复展示，无需等首次刷新）
    pub fn with_persisted() -> Self {
        let state = Self::default();
        let persisted = crate::persist::load();
        if !persisted.is_empty() {
            if let Ok(mut map) = state.results.lock() {
                *map = persisted;
            }
        }
        state
    }

    /// 克隆当前全部结果快照（供持久化落盘）
    pub fn results_map(&self) -> HashMap<String, SiteResult> {
        self.results.lock().map(|m| m.clone()).unwrap_or_default()
    }

    pub fn get_token(&self, site_id: &str) -> Option<TokenCache> {
        self.tokens.lock().ok()?.get(site_id).cloned()
    }

    pub fn set_token(&self, site_id: &str, token: TokenCache) {
        if let Ok(mut map) = self.tokens.lock() {
            map.insert(site_id.to_string(), token);
        }
    }

    pub fn clear_token(&self, site_id: &str) {
        if let Ok(mut map) = self.tokens.lock() {
            map.remove(site_id);
        }
    }

    pub fn set_result(&self, result: SiteResult) {
        if let Ok(mut map) = self.results.lock() {
            map.insert(result.id.clone(), result);
        }
    }

    pub fn prune_sites(&self, keep: &[String]) {
        if let Ok(mut map) = self.results.lock() {
            map.retain(|id, _| keep.iter().any(|k| k == id));
        }
        if let Ok(mut map) = self.tokens.lock() {
            map.retain(|id, _| keep.iter().any(|k| k == id));
        }
    }
}

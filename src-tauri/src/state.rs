use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{SiteConfig, SiteType};
use crate::models::{as_percent, unix_to_iso};

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
    /// gpt | claude | grok | kimi | gemini | qwen | seedream
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// 统一后的模型名，如 claude-sonnet-4-6
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub availability: Option<f64>,
    pub latency_ms: Option<i64>,
    pub tiers: Vec<QuotaTier>,
    pub balances: Vec<ChannelBalance>,
    /// new-api 渠道分组的模型倍率
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ratio: Option<f64>,
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

    /// 合并站点原始趋势与本地分钟采样，然后写入缓存并返回同一份结果。
    pub fn merge_and_set_result(&self, mut result: SiteResult) -> SiteResult {
        if let Ok(mut map) = self.results.lock() {
            if let Some(previous) = map.get(&result.id) {
                merge_trends(previous, &mut result);
            } else {
                add_minute_samples(&mut result);
            }
            map.insert(result.id.clone(), result.clone());
        }
        result
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

fn channel_key(channel: &ChannelStatus) -> (&str, &str) {
    (&channel.name, channel.model.as_deref().unwrap_or(""))
}

fn merge_trends(previous: &SiteResult, current: &mut SiteResult) {
    for channel in &mut current.channels {
        let old = previous
            .channels
            .iter()
            .find(|candidate| channel_key(candidate) == channel_key(channel));
        let mut points = BTreeMap::new();
        if let Some(old) = old.and_then(|candidate| candidate.trend.as_ref()) {
            points.extend(old.iter().map(|point| (point.t.clone(), point.v)));
        }
        if let Some(fresh) = channel.trend.take() {
            points.extend(fresh.into_iter().map(|point| (point.t, point.v)));
        }
        if let Some(value) = channel.availability {
            points.insert(minute_label(current.checked_at), as_percent(value));
        }
        let merged = last_points(points, 1_440);
        channel.trend = (!merged.is_empty()).then_some(merged);
    }
}

fn add_minute_samples(result: &mut SiteResult) {
    for channel in &mut result.channels {
        let Some(value) = channel.availability else {
            continue;
        };
        let mut points = BTreeMap::new();
        if let Some(fresh) = channel.trend.take() {
            points.extend(fresh.into_iter().map(|point| (point.t, point.v)));
        }
        points.insert(minute_label(result.checked_at), as_percent(value));
        let merged = last_points(points, 1_440);
        channel.trend = (!merged.is_empty()).then_some(merged);
    }
}

fn minute_label(checked_at: u64) -> String {
    unix_to_iso(((checked_at / 60_000) * 60) as i64)
}

fn last_points(points: BTreeMap<String, f64>, limit: usize) -> Vec<TrendPoint> {
    let skip = points.len().saturating_sub(limit);
    points
        .into_iter()
        .skip(skip)
        .map(|(t, v)| TrendPoint { t, v })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(availability: f64, trend: Vec<TrendPoint>) -> ChannelStatus {
        ChannelStatus {
            name: "vip".into(),
            online: true,
            detail: String::new(),
            status: "operational".into(),
            plan_level: None,
            provider: Some("gpt".into()),
            model: Some("gpt-5.6-sol".into()),
            availability: Some(availability),
            latency_ms: None,
            tiers: Vec::new(),
            balances: Vec::new(),
            model_ratio: Some(0.16),
            trend: Some(trend),
        }
    }

    #[test]
    fn merge_trends_keeps_history_and_adds_minute_sample() {
        let site = SiteConfig {
            id: "site".into(),
            name: "Site".into(),
            site_type: SiteType::New2api,
            base_url: "https://example.com".into(),
            vpn: false,
            token: None,
            user_id: None,
            username: None,
            password: None,
        };
        let mut previous = SiteResult::base(&site, true, None);
        previous.channels = vec![channel(
            90.0,
            vec![TrendPoint {
                t: "2026-08-22T12:00:00Z".into(),
                v: 90.0,
            }],
        )];
        let mut current = SiteResult::base(&site, true, None);
        current.checked_at = 1_787_403_660_000;
        current.channels = vec![channel(96.0, Vec::new())];

        merge_trends(&previous, &mut current);

        let trend = current.channels[0].trend.as_ref().unwrap();
        assert_eq!(trend.len(), 2);
        assert_eq!(trend[0].t, "2026-08-22T12:00:00Z");
        assert_eq!(trend[1].v, 96.0);
        assert!(trend[1].t.ends_with(":00Z"));
    }
}

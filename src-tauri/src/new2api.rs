use std::collections::HashMap;
use std::time::Duration;

use reqwest::Client;
use serde_json::Value;

use crate::config::{MonitorModels, SiteConfig};
use crate::http::truncate;
use crate::models::{
    detect_provider, format_channel_detail, models_match, sort_by_success_rate, Provider,
};
use crate::state::{ChannelStatus, SiteResult};

const PERF_REQ_TIMEOUT: Duration = Duration::from_secs(5);

/// New API 换算规则：500000 quota = $1
const QUOTA_PER_USD: f64 = 500_000.0;

/// new2api：查余额（/api/user/self，需令牌）+ 分组性能（/api/perf-metrics/*，公开接口）。
/// 未配置令牌时跳过余额查询，仅拉取模型广场性能数据。
pub async fn check(client: &Client, site: &SiteConfig, monitor: &MonitorModels) -> SiteResult {
    let base = site.base_url.trim_end_matches('/');
    let raw_token = site.token.clone().unwrap_or_default();
    let token = raw_token.trim();
    let has_token = !token.is_empty() && !token.ends_with("...");
    let token = has_token.then(|| token.to_string());
    let user_id = site
        .user_id
        .clone()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());

    let self_h = token.clone().map(|token| {
        spawn_authorized_get(
            client.clone(),
            format!("{base}/api/user/self"),
            Some(token),
            user_id.clone(),
            None,
            2,
        )
    });
    let summary_h = spawn_authorized_get(
        client.clone(),
        format!("{base}/api/perf-metrics/summary?hours=24"),
        token.clone(),
        user_id.clone(),
        None,
        1,
    );

    let summary = match summary_h.await {
        Ok(Ok((status, body))) if (200..300).contains(&status) => {
            serde_json::from_str(&body).unwrap_or(Value::Null)
        }
        _ => Value::Null,
    };

    let available = summary_models(&summary);
    let perf_h = {
        let client = client.clone();
        let base = base.to_string();
        let token = token.clone();
        let user_id = user_id.clone();
        tauri::async_runtime::spawn(async move {
            fetch_group_perfs(&client, &base, token.as_deref(), user_id.as_deref(), &available)
                .await
        })
    };

    // 余额与请求数仅在配置了令牌时查询；匿名模式保持 None
    let (quota, request_count) = match self_h {
        Some(self_h) => {
            let self_body = match self_h.await {
                Ok(Ok((status, body))) if (200..300).contains(&status) => body,
                Ok(Ok((status, body))) => {
                    return SiteResult::error(
                        site,
                        format!("HTTP {status}：{}", truncate(body.trim(), 200)),
                    );
                }
                Ok(Err(e)) => return SiteResult::error(site, e),
                Err(e) => return SiteResult::error(site, format!("内部错误: {e}")),
            };

            let value: Value = match serde_json::from_str(&self_body) {
                Ok(v) => v,
                Err(e) => return SiteResult::error(site, format!("响应不是合法 JSON: {e}")),
            };

            let data = value.get("data").unwrap_or(&value);
            let quota = pick_number(data, "quota").map(|v| v as i64);
            let request_count = pick_number(data, "request_count").map(|v| v as u64);

            if quota.is_none() && request_count.is_none() {
                let mut result = SiteResult::error(
                    site,
                    "响应中未找到 quota / request_count 字段".to_string(),
                );
                result.raw = Some(truncate(&self_body, 2000));
                return result;
            }
            (quota, request_count)
        }
        None => (None, None),
    };

    let perfs = match perf_h.await {
        Ok(map) => map,
        Err(_) => HashMap::new(),
    };

    let mut result = SiteResult::base(site, true, None);
    result.quota = quota;
    result.balance_usd = quota.map(|q| q as f64 / QUOTA_PER_USD);
    result.request_count = request_count;
    result.channels = parse_plaza_channels(&perfs, monitor);
    if !has_token {
        result.note = Some("未配置访问令牌，仅显示模型广场数据（余额不可用）".to_string());
    } else if result.channels.is_empty() {
        result.note = Some("暂无分组性能数据".to_string());
    }
    result
}

#[derive(Default, Clone)]
struct GroupPerf {
    group: String,
    latency_ms: i64,
    success_rate: f64,
    model: String,
}

fn summary_models(summary: &Value) -> Vec<String> {
    summary
        .pointer("/data/models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    item.get("model_name")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn fetch_group_perfs(
    client: &Client,
    base: &str,
    token: Option<&str>,
    user_id: Option<&str>,
    models: &[String],
) -> HashMap<String, GroupPerf> {
    if models.is_empty() {
        return HashMap::new();
    }

    const CONCURRENCY: usize = 8;
    let mut perfs = HashMap::new();
    for chunk in models.chunks(CONCURRENCY) {
        let mut handles = Vec::new();
        for model in chunk {
            let client = client.clone();
            let url = format!(
                "{base}/api/perf-metrics?model={}&hours=24",
                urlencoding_lite(model)
            );
            let token = token.map(str::to_string);
            let user_id = user_id.map(str::to_string);
            handles.push(tauri::async_runtime::spawn(async move {
                crate::http::authorized_get(
                    &client,
                    &url,
                    token.as_deref(),
                    user_id.as_deref(),
                    Some(PERF_REQ_TIMEOUT),
                    0,
                )
                .await
            }));
        }
        for handle in handles {
            let Ok(Ok((status, body))) = handle.await else {
                continue;
            };
            if !(200..300).contains(&status) {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&body) else {
                continue;
            };
            merge_group_perfs(&mut perfs, &value);
        }
    }
    perfs
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn merge_group_perfs(perfs: &mut HashMap<String, GroupPerf>, value: &Value) {
    let model = value
        .pointer("/data/model_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let Some(groups) = value.pointer("/data/groups").and_then(|v| v.as_array()) else {
        return;
    };
    for item in groups {
        let Some(name) = item.get("group").and_then(|v| v.as_str()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let key = format!("{name}\x1f{model}");
        perfs.insert(
            key,
            GroupPerf {
                group: name.to_string(),
                latency_ms: pick_number(item, "avg_latency_ms").unwrap_or(0.0) as i64,
                success_rate: pick_number(item, "success_rate").unwrap_or(0.0),
                model: model.clone(),
            },
        );
    }
}

fn model_preference(group: &str, model: &str, wanted: &[String]) -> (u8, usize) {
    let group_family = detect_provider(group);
    let model_family = detect_provider(model);
    let family_rank = match (model_family, group_family) {
        (Some(a), Some(b)) if a == b => 0,
        (_, None) => 1,
        (None, _) => 2,
        _ => 3,
    };
    let want_rank = wanted
        .iter()
        .position(|want| models_match(model, want))
        .unwrap_or(usize::MAX);
    (family_rank, want_rank)
}

/// 每个分组一行：优先用设置里的最新模型，没有则回退该分组已有数据
fn collapse_group_perfs(
    perfs: &HashMap<String, GroupPerf>,
    monitor: &MonitorModels,
) -> Vec<GroupPerf> {
    let wanted = monitor.all_names();
    let mut by_group: HashMap<String, Vec<&GroupPerf>> = HashMap::new();
    for perf in perfs.values() {
        by_group.entry(perf.group.clone()).or_default().push(perf);
    }
    by_group
        .into_values()
        .filter_map(|mut items| {
            items.sort_by_key(|perf| model_preference(&perf.group, &perf.model, &wanted));
            items.first().map(|perf| (*perf).clone())
        })
        .collect()
}

/// 广场性能：每个渠道分组一行，默认按成功率降序
fn parse_plaza_channels(
    perfs: &HashMap<String, GroupPerf>,
    monitor: &MonitorModels,
) -> Vec<ChannelStatus> {
    let mut channels: Vec<ChannelStatus> = collapse_group_perfs(perfs, monitor)
        .into_iter()
        .map(|perf| {
            let availability = Some(perf.success_rate);
            let latency_ms = (perf.latency_ms > 0).then_some(perf.latency_ms);
            let pct = crate::models::as_percent(perf.success_rate);
            let status = if pct >= 95.0 {
                "operational"
            } else if pct >= 80.0 {
                "degraded"
            } else {
                "failed"
            };
            let provider = detect_provider(&perf.model)
                .or_else(|| detect_provider(&perf.group));
            ChannelStatus {
                name: perf.group.clone(),
                online: status != "failed",
                detail: format_channel_detail(&perf.model, latency_ms, availability),
                status: status.into(),
                plan_level: None,
                provider: provider.map(Provider::id).map(str::to_string),
                model: (!perf.model.is_empty()).then(|| perf.model.clone()),
                availability,
                latency_ms,
                tiers: Vec::new(),
                balances: Vec::new(),
            }
        })
        .collect();
    sort_by_success_rate(&mut channels, |c| c.availability);
    channels
}

fn spawn_authorized_get(
    client: Client,
    url: String,
    token: Option<String>,
    user_id: Option<String>,
    timeout: Option<Duration>,
    retries: u32,
) -> tauri::async_runtime::JoinHandle<Result<(u16, String), String>> {
    tauri::async_runtime::spawn(async move {
        crate::http::authorized_get(
            &client,
            &url,
            token.as_deref(),
            user_id.as_deref(),
            timeout,
            retries,
        )
        .await
    })
}

/// 宽松取数字字段：兼容数字与字符串数字
fn pick_number(obj: &Value, key: &str) -> Option<f64> {
    obj.get(key).and_then(|v| match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plaza_group_perf_table() {
        let mut perfs = HashMap::new();
        perfs.insert(
            "gpt 易燃易爆炸\x1fgpt-5.6-sol".into(),
            GroupPerf {
                group: "gpt 易燃易爆炸".into(),
                latency_ms: 13740,
                success_rate: 99.3,
                model: "gpt-5.6-sol".into(),
            },
        );
        perfs.insert(
            "grok\x1fgrok-4.6".into(),
            GroupPerf {
                group: "grok".into(),
                latency_ms: 16111,
                success_rate: 92.2,
                model: "grok-4.6".into(),
            },
        );
        let channels = parse_plaza_channels(&perfs, &MonitorModels::default());
        assert_eq!(channels[0].name, "gpt 易燃易爆炸");
        let gpt = channels.iter().find(|c| c.name == "gpt 易燃易爆炸").unwrap();
        assert_eq!(gpt.status, "operational");
        assert_eq!(gpt.provider.as_deref(), Some("gpt"));
        assert_eq!(gpt.detail, "gpt-5.6-sol · 13740ms · 99.3%");
        assert!((gpt.availability.unwrap() - 99.3).abs() < 0.01);
        let grok = channels.iter().find(|c| c.name == "grok").unwrap();
        assert_eq!(grok.status, "degraded");
    }

    #[test]
    fn collapse_prefers_latest_configured_model() {
        let mut perfs = HashMap::new();
        perfs.insert(
            "gpt\x1fgpt-5.4".into(),
            GroupPerf {
                group: "gpt".into(),
                success_rate: 99.9,
                model: "gpt-5.4".into(),
                ..Default::default()
            },
        );
        perfs.insert(
            "gpt\x1fgpt-5.6-sol".into(),
            GroupPerf {
                group: "gpt".into(),
                latency_ms: 2100,
                success_rate: 96.0,
                model: "gpt-5.6-sol".into(),
                ..Default::default()
            },
        );
        let channels = parse_plaza_channels(&perfs, &MonitorModels::default());
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(channels[0].detail, "gpt-5.6-sol · 2100ms · 96.0%");
    }

    #[test]
    fn merge_group_perfs_from_api() {
        let value = serde_json::json!({
            "data": {
                "model_name": "gpt-5.6-sol",
                "groups": [
                    {
                        "group": "vip",
                        "avg_ttft_ms": 3430,
                        "avg_latency_ms": 13220,
                        "success_rate": 99.1,
                        "avg_tps": 52.4
                    }
                ]
            }
        });
        let mut perfs = HashMap::new();
        merge_group_perfs(&mut perfs, &value);
        let vip = perfs.values().find(|p| p.group == "vip").unwrap();
        assert!((vip.success_rate - 99.1).abs() < f64::EPSILON);
        assert_eq!(vip.model, "gpt-5.6-sol");
    }
}

use std::collections::HashMap;
use std::time::Duration;

use reqwest::Client;
use serde_json::Value;

use crate::config::{MonitorModels, SiteConfig};
use crate::http::truncate;
use crate::models::{
    detect_provider, format_channel_detail, is_non_chat_model, models_match, normalize_model,
    sort_by_success_rate, unix_to_iso, Provider,
};
use crate::state::{now_millis, ChannelStatus, SiteResult, TrendPoint};

const PERF_REQ_TIMEOUT: Duration = Duration::from_secs(6);

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
        )
    });
    let summary_h = spawn_authorized_get(
        client.clone(),
        format!(
            "{base}/api/perf-metrics/summary?hours=24&_ts={}",
            now_millis()
        ),
        token.clone(),
        user_id.clone(),
        None,
    );
    let group_ratio_h = spawn_authorized_get(
        client.clone(),
        format!(
            "{base}{}?_ts={}",
            if has_token {
                "/api/user/self/groups"
            } else {
                "/api/user/groups"
            },
            now_millis()
        ),
        token.clone(),
        user_id.clone(),
        None,
    );

    let summary = match summary_h.await {
        Ok(Ok((status, body))) if (200..300).contains(&status) => {
            serde_json::from_str(&body).unwrap_or(Value::Null)
        }
        _ => Value::Null,
    };

    let available = select_monitored_models(&summary_models(&summary), monitor);
    let perf_h = {
        let client = client.clone();
        let base = base.to_string();
        let token = token.clone();
        let user_id = user_id.clone();
        tauri::async_runtime::spawn(async move {
            fetch_group_perfs(
                &client,
                &base,
                token.as_deref(),
                user_id.as_deref(),
                &available,
            )
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
                let mut result =
                    SiteResult::error(site, "响应中未找到 quota / request_count 字段".to_string());
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
    let group_ratios = match group_ratio_h.await {
        Ok(Ok((status, body))) if (200..300).contains(&status) => serde_json::from_str(&body)
            .ok()
            .map(|value| parse_group_ratios(&value))
            .unwrap_or_default(),
        _ => HashMap::new(),
    };

    let mut result = SiteResult::base(site, true, None);
    result.quota = quota;
    result.balance_usd = quota.map(|q| q as f64 / QUOTA_PER_USD);
    result.request_count = request_count;
    result.channels = parse_plaza_channels(&perfs, monitor, &group_ratios);
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
    /// 分组的逐时成功率趋势（series，近 24h）
    trend: Vec<TrendPoint>,
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

/// 只请求设置中列出的对话模型，并使用归一化后的精确匹配。
fn select_monitored_models(available: &[String], monitor: &MonitorModels) -> Vec<String> {
    let mut selected = Vec::new();
    for wanted in monitor.all_names() {
        if is_non_chat_model(&wanted) {
            continue;
        }
        let wanted_normalized = normalize_model(&wanted);
        let matched = available
            .iter()
            .find(|model| !is_non_chat_model(model) && normalize_model(model) == wanted_normalized);
        if let Some(model) = matched {
            if !selected.iter().any(|item| item == model) {
                selected.push(model.clone());
            }
        }
    }
    selected
}

fn parse_group_ratios(value: &Value) -> HashMap<String, f64> {
    value
        .get("data")
        .and_then(|data| data.as_object())
        .map(|groups| {
            groups
                .iter()
                .filter_map(|(name, detail)| {
                    pick_number(detail, "ratio").map(|ratio| (name.clone(), ratio))
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

    // 模型性能接口彼此独立；单轮受控并发避免少量慢模型把整站刷新拖成多轮。
    const CONCURRENCY: usize = 16;
    let mut perfs = HashMap::new();
    for chunk in models.chunks(CONCURRENCY) {
        let mut handles = Vec::new();
        for model in chunk {
            let client = client.clone();
            let url = format!(
                "{base}/api/perf-metrics?model={}&hours=24&_ts={}",
                urlencoding_lite(model),
                now_millis()
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
        // series[]：{ts(unix 秒), success_rate} 逐时桶 → 趋势线
        let mut trend: Vec<TrendPoint> = item
            .get("series")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let ts = p.get("ts")?.as_i64()?;
                        let rate = p.get("success_rate")?.as_f64()?;
                        Some(TrendPoint {
                            t: unix_to_iso(ts),
                            v: crate::models::as_percent(rate),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        trend.sort_by(|a, b| a.t.cmp(&b.t));
        trend.dedup_by(|a, b| {
            if a.t == b.t {
                a.v = b.v;
                true
            } else {
                false
            }
        });
        let aggregate = pick_number(item, "success_rate").unwrap_or(0.0);
        let current = trend.last().map(|point| point.v).unwrap_or(aggregate);
        perfs.insert(
            key,
            GroupPerf {
                group: name.to_string(),
                latency_ms: pick_number(item, "avg_latency_ms").unwrap_or(0.0) as i64,
                // 最新小时桶比 24 小时聚合值更及时；没有序列时回退聚合值。
                success_rate: current,
                model: model.clone(),
                trend,
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
            let mut chosen = items.first().map(|perf| (*perf).clone())?;
            // 首选模型可能没有 series（新分组/接口缺数据），借用同分组内最长的趋势
            if chosen.trend.is_empty() {
                if let Some(best) = items
                    .iter()
                    .filter(|p| !p.trend.is_empty())
                    .max_by_key(|p| p.trend.len())
                {
                    chosen.trend = best.trend.clone();
                }
            }
            Some(chosen)
        })
        .collect()
}

/// 广场性能：每个渠道分组一行，默认按成功率降序
fn parse_plaza_channels(
    perfs: &HashMap<String, GroupPerf>,
    monitor: &MonitorModels,
    group_ratios: &HashMap<String, f64>,
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
            let provider = detect_provider(&perf.model).or_else(|| detect_provider(&perf.group));
            ChannelStatus {
                name: perf.group.clone(),
                label: None,
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
                model_ratio: group_ratios.get(&perf.group).copied(),
                trend: (!perf.trend.is_empty()).then(|| perf.trend.clone()),
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
) -> tauri::async_runtime::JoinHandle<Result<(u16, String), String>> {
    tauri::async_runtime::spawn(async move {
        crate::http::authorized_get(&client, &url, token.as_deref(), user_id.as_deref(), timeout)
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
                trend: Vec::new(),
            },
        );
        perfs.insert(
            "grok\x1fgrok-4.6".into(),
            GroupPerf {
                group: "grok".into(),
                latency_ms: 16111,
                success_rate: 92.2,
                model: "grok-4.6".into(),
                trend: Vec::new(),
            },
        );
        let channels = parse_plaza_channels(&perfs, &MonitorModels::default(), &HashMap::new());
        assert_eq!(channels[0].name, "gpt 易燃易爆炸");
        let gpt = channels
            .iter()
            .find(|c| c.name == "gpt 易燃易爆炸")
            .unwrap();
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
        let channels = parse_plaza_channels(&perfs, &MonitorModels::default(), &HashMap::new());
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
                        "avg_tps": 52.4,
                        "series": [
                            { "ts": 1787302800, "success_rate": 98.5 },
                            { "ts": 1787306400, "success_rate": 99.9 },
                            { "ts": 1787310000, "success_rate": 100.0 }
                        ]
                    }
                ]
            }
        });
        let mut perfs = HashMap::new();
        merge_group_perfs(&mut perfs, &value);
        let vip = perfs.values().find(|p| p.group == "vip").unwrap();
        assert!((vip.success_rate - 100.0).abs() < f64::EPSILON);
        assert_eq!(vip.model, "gpt-5.6-sol");
        assert_eq!(vip.trend.len(), 3);
        assert_eq!(vip.trend[0].t, "2026-08-21T09:00:00Z");
        assert!((vip.trend[2].v - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn single_point_series_keeps_trend() {
        // 新分组的 series 可能只有 1 个桶，仍应展示（前端画平线）
        let value = serde_json::json!({
            "data": {
                "model_name": "gpt-5.6-sol",
                "groups": [
                    {
                        "group": "pro测试",
                        "avg_latency_ms": 3341,
                        "success_rate": 100,
                        "series": [
                            { "ts": 1787317200, "success_rate": 100 }
                        ]
                    }
                ]
            }
        });
        let mut perfs = HashMap::new();
        merge_group_perfs(&mut perfs, &value);
        let channels = parse_plaza_channels(&perfs, &MonitorModels::default(), &HashMap::new());
        let pro = channels.iter().find(|c| c.name == "pro测试").unwrap();
        let trend = pro.trend.as_ref().expect("单点趋势应保留");
        assert_eq!(trend.len(), 1);
        assert_eq!(trend[0].t, "2026-08-21T13:00:00Z");
    }

    #[test]
    fn collapse_falls_back_to_other_model_trend() {
        let mut perfs = HashMap::new();
        // 首选模型（配置中的 gpt-5.6-sol）没有 series
        perfs.insert(
            "vip\x1fgpt-5.6-sol".into(),
            GroupPerf {
                group: "vip".into(),
                latency_ms: 2100,
                success_rate: 96.0,
                model: "gpt-5.6-sol".into(),
                trend: Vec::new(),
            },
        );
        // 同分组的其他模型带有趋势，应被借用
        perfs.insert(
            "vip\x1fgpt-5.4".into(),
            GroupPerf {
                group: "vip".into(),
                latency_ms: 1800,
                success_rate: 98.0,
                model: "gpt-5.4".into(),
                trend: vec![
                    TrendPoint {
                        t: "2026-08-21T09:00:00Z".into(),
                        v: 99.0,
                    },
                    TrendPoint {
                        t: "2026-08-21T10:00:00Z".into(),
                        v: 97.0,
                    },
                ],
            },
        );
        let channels = parse_plaza_channels(&perfs, &MonitorModels::default(), &HashMap::new());
        assert_eq!(channels.len(), 1);
        // 展示的仍是首选模型的指标
        assert_eq!(channels[0].model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(channels[0].detail, "gpt-5.6-sol · 2100ms · 96.0%");
        let trend = channels[0]
            .trend
            .as_ref()
            .expect("应借用同分组其他模型的趋势");
        assert_eq!(trend.len(), 2);
    }

    #[test]
    fn monitored_selection_prefers_exact_model_and_excludes_images() {
        let available = vec![
            "gpt-image-2".to_string(),
            "gpt-5.6-sol-openai-compact".to_string(),
            "gpt-5.6-sol".to_string(),
            "claude-opus-5".to_string(),
        ];
        let selected = select_monitored_models(&available, &MonitorModels::default());
        assert!(selected.contains(&"gpt-5.6-sol".to_string()));
        assert!(selected.contains(&"claude-opus-5".to_string()));
        assert!(!selected.contains(&"gpt-5.6-sol-openai-compact".to_string()));
        // gpt-image 属于 GPT，但未在默认监控模型中，所以不会被额外抓取。
        assert!(!selected.contains(&"gpt-image-2".to_string()));
    }

    #[test]
    fn group_ratio_is_attached_by_group_name() {
        let value = serde_json::json!({
            "data": {
                "gpt 易燃易爆炸": { "desc": "自动分组链 → vip", "ratio": 0.16 },
                "vip": { "ratio": "0.16" }
            }
        });
        let ratios = parse_group_ratios(&value);
        assert_eq!(ratios.get("gpt 易燃易爆炸"), Some(&0.16));
        assert_eq!(ratios.get("vip"), Some(&0.16));
        assert_eq!(ratios.get("other"), None);

        let mut perfs = HashMap::new();
        perfs.insert(
            "gpt 易燃易爆炸\x1fgpt-5.6-sol".into(),
            GroupPerf {
                group: "gpt 易燃易爆炸".into(),
                model: "gpt-5.6-sol".into(),
                success_rate: 100.0,
                ..Default::default()
            },
        );
        let channels = parse_plaza_channels(&perfs, &MonitorModels::default(), &ratios);
        assert_eq!(channels[0].model_ratio, Some(0.16));
    }
}

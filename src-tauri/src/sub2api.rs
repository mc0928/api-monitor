use reqwest::Client;
use serde_json::Value;

use crate::config::SiteConfig;
use crate::http::truncate;
use crate::models::{detect_provider, format_channel_detail, sort_by_success_rate, Provider};
use crate::state::{
    now_secs, AppState, ChannelBalance, ChannelStatus, QuotaTier, SiteResult, TokenCache,
    TrendPoint,
};

/// sub2api 站点采集：确保登录态 -> 拉取渠道监控列表；401 时清缓存重登一次
pub async fn check(client: &Client, site: &SiteConfig, state: &AppState) -> SiteResult {
    let mut token = match ensure_token(client, site, state).await {
        Ok(t) => t,
        Err(e) => return SiteResult::error(site, e),
    };

    let base = site.base_url.trim_end_matches('/');
    let url = format!("{base}/api/v1/channel-monitors");

    let (status, body) = match fetch_monitors(client, &url, &token.auth_token).await {
        Ok(r) => r,
        Err(e) => return SiteResult::error(site, e),
    };

    // 401：令牌失效，清除缓存重新登录后再取一次
    let (status, body) = if status == 401 {
        state.clear_token(&site.id);
        token = match ensure_token(client, site, state).await {
            Ok(t) => t,
            Err(e) => return SiteResult::error(site, e),
        };
        match fetch_monitors(client, &url, &token.auth_token).await {
            Ok(r) => r,
            Err(e) => return SiteResult::error(site, e),
        }
    } else {
        (status, body)
    };

    if status == 401 {
        state.clear_token(&site.id);
        return SiteResult::error(site, "channel-monitors 返回 401，令牌已失效".to_string());
    }

    let mut result = SiteResult::base(site, true, None);
    let mut channels: Vec<ChannelStatus> = Vec::new();
    // 主动监控接口不可用（如被动模式站点返回 403 模式不匹配、旧版本 404）时不直接报错，
    // 记录原因后继续尝试 V2 被动监控——不同站点部署的监控形式可能不同
    let mut active_error: Option<String> = None;

    if (200..300).contains(&status) {
        let value: Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => return SiteResult::error(site, format!("响应不是合法 JSON: {e}")),
        };
        channels = rank_channels(parse_channels(&value));
        // 字段结构未最终确定前，保留原始响应片段便于调试（调试开关关闭时由 lib.rs 剥离）
        result.raw = Some(truncate(&body, 2000));
    } else {
        active_error = Some(format!("HTTP {status}：{}", truncate(body.trim(), 200)));
    }

    // 主动监控无数据或不可用时，回退到 V2 被动监控（部分站点数据只在此接口）
    if channels.is_empty() {
        if let Ok(v2) = fetch_v2_matrix(client, base, &token.auth_token).await {
            if !v2.is_empty() {
                channels = rank_channels(v2);
            }
        }
    }

    result.balance_usd = site_balance_from_channels(&channels).or(token.balance);
    result.channels = channels;
    if result.channels.is_empty() {
        result.note = Some(match active_error {
            Some(err) => format!(
                "渠道监控接口不可用（{err}），且 V2 被动监控无数据；该站点可能使用了不兼容的监控模式或未开启监控"
            ),
            None => "站点未返回渠道监控，请确认已在网站上添加".to_string(),
        });
    }
    result
}

/// V2 被动监控：按分组聚合的成功率/错误率/首 Token 延迟
/// GET /api/v1/channel-monitor-v2/matrix?range=24h
async fn fetch_v2_matrix(
    client: &Client,
    base: &str,
    token: &str,
) -> Result<Vec<ChannelStatus>, String> {
    let url = format!("{base}/api/v1/channel-monitor-v2/matrix?range=24h");
    let (status, body) = crate::http::authorized_get(client, &url, Some(token), None, None, 1)
        .await
        .map_err(|e| format!("V2 监控请求失败: {e}"))?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|e| format!("V2 监控响应不是合法 JSON: {e}"))?;
    Ok(parse_v2_matrix(&value))
}

/// 解析 V2 matrix：data.items[] = { platform, group_name, metrics{success_rate,ttft}, health{overall} }
fn parse_v2_matrix(value: &Value) -> Vec<ChannelStatus> {
    let Some(items) = value.pointer("/data/items").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let name = item
                .get("group_name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("未知分组")
                .to_string();
            let platform = item.get("platform").and_then(|v| v.as_str()).unwrap_or("");
            // health.overall: healthy / warning / critical
            let overall = item
                .pointer("/health/overall")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let (online, status) = match overall {
                "healthy" => (true, "operational"),
                "warning" => (true, "degraded"),
                "critical" => (false, "failed"),
                _ => (false, "unknown"),
            };
            // success_rate 为 0~1 比率，p50_ms 为首 Token 中位延迟
            let availability = item
                .pointer("/metrics/success_rate")
                .and_then(|v| v.as_f64());
            let latency_ms = item
                .pointer("/metrics/ttft/p50_ms")
                .and_then(|v| v.as_f64())
                .map(|v| v as i64);
            // buckets：24 个逐时桶，转成趋势线点（成功率百分数）
            let trend = item.get("buckets").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        let t = b.get("bucket_start")?.as_str()?.to_string();
                        let v = b.pointer("/metrics/success_rate")?.as_f64()? * 100.0;
                        Some(TrendPoint { t, v })
                    })
                    .collect::<Vec<_>>()
            });
            let trend = trend.filter(|t| !t.is_empty());
            ChannelStatus {
                name,
                online,
                detail: format_channel_detail("", latency_ms, availability),
                status: status.into(),
                plan_level: None,
                provider: Provider::from_id(platform)
                    .map(Provider::id)
                    .map(str::to_string),
                model: None,
                availability,
                latency_ms,
                tiers: Vec::new(),
                balances: Vec::new(),
                trend,
            }
        })
        .collect()
}

/// 拉取渠道监控列表（共用 GET，带一次传输层重试）
async fn fetch_monitors(client: &Client, url: &str, token: &str) -> Result<(u16, String), String> {
    crate::http::authorized_get(client, url, Some(token), None, None, 1).await
}

/// 站点级余额：优先取 USD，否则取渠道余额合计（同币种才汇总）
fn site_balance_from_channels(channels: &[ChannelStatus]) -> Option<f64> {
    let usd: Vec<f64> = channels
        .iter()
        .flat_map(|c| c.balances.iter())
        .filter(|b| b.currency.eq_ignore_ascii_case("usd") || b.currency == "$")
        .map(|b| b.balance)
        .collect();
    if !usd.is_empty() {
        return Some(usd.iter().sum());
    }
    let mut currency: Option<&str> = None;
    let mut total = 0.0;
    let mut any = false;
    for b in channels.iter().flat_map(|c| c.balances.iter()) {
        match currency {
            None => currency = Some(&b.currency),
            Some(c) if c != b.currency => return None,
            Some(_) => {}
        }
        total += b.balance;
        any = true;
    }
    any.then_some(total)
}

/// 无 token 或已过期：先 refresh，失败再 login；成功后写入缓存
async fn ensure_token(
    client: &Client,
    site: &SiteConfig,
    state: &AppState,
) -> Result<TokenCache, String> {
    let cached = state.get_token(&site.id);

    if let Some(ref c) = cached {
        // 提前 60s 视为过期
        if !c.auth_token.is_empty() && c.expires_at > now_secs() + 60 {
            return Ok(c.clone());
        }
        if let Some(ref refresh) = c.refresh_token {
            if let Ok(new_token) = refresh_token(client, site, refresh).await {
                state.set_token(&site.id, new_token.clone());
                return Ok(new_token);
            }
        }
    }

    let username = site.username.clone().unwrap_or_default();
    let password = site.password.clone().unwrap_or_default();
    if username.trim().is_empty() || password.is_empty() {
        return Err("未配置账号密码，请在设置中补填".to_string());
    }

    let new_token = login(client, site, username.trim(), &password).await?;
    state.set_token(&site.id, new_token.clone());
    Ok(new_token)
}

async fn login(
    client: &Client,
    site: &SiteConfig,
    email: &str,
    password: &str,
) -> Result<TokenCache, String> {
    let url = format!("{}/api/v1/auth/login", site.base_url.trim_end_matches('/'));
    let payload = serde_json::json!({ "email": email, "password": password });

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("登录请求失败: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "登录失败（HTTP {}）：{}",
            status,
            truncate(body.trim(), 200)
        ));
    }

    let value: Value = serde_json::from_str(&body)
        .map_err(|e| format!("登录响应不是合法 JSON: {e}"))?;
    parse_token(&value).ok_or_else(|| "登录响应中未找到 auth_token".to_string())
}

async fn refresh_token(
    client: &Client,
    site: &SiteConfig,
    refresh: &str,
) -> Result<TokenCache, String> {
    let url = format!("{}/api/v1/auth/refresh", site.base_url.trim_end_matches('/'));
    let payload = serde_json::json!({ "refresh_token": refresh });

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("refresh 请求失败: {e}"))?;
    if !response.status().is_success() {
        return Err("refresh 失败".to_string());
    }
    let body = response.text().await.unwrap_or_default();
    let value: Value = serde_json::from_str(&body)
        .map_err(|e| format!("refresh 响应不是合法 JSON: {e}"))?;
    parse_token(&value).ok_or_else(|| "refresh 响应中未找到 auth_token".to_string())
}

/// 宽松解析令牌响应：兼容顶层或 data 包裹，token / expires_at 字段名容错
fn parse_token(value: &Value) -> Option<TokenCache> {
    let data = value.get("data").unwrap_or(value);
    let auth_token = data
        .get("auth_token")
        .or_else(|| data.get("access_token"))
        .or_else(|| data.get("token"))
        .and_then(|v| v.as_str())?
        .to_string();
    let refresh_token = data
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(String::from);
    let expires_at = data
        .get("token_expires_at")
        .or_else(|| data.get("expires_at"))
        .and_then(|v| v.as_i64())
        .or_else(|| {
            data.get("expires_in")
                .and_then(|v| v.as_i64())
                .map(|secs| now_secs() + secs)
        })
        // 拿不到过期时间时保守按 1 小时处理
        .unwrap_or_else(|| now_secs() + 3600);
    let user = data.get("user").unwrap_or(data);
    let balance = pick_number(user, "balance").or_else(|| pick_number(data, "balance"));
    Some(TokenCache {
        auth_token,
        refresh_token,
        expires_at,
        balance,
    })
}

/// 宽松解析渠道列表：数组可能在顶层 / data / data.items / channels 下。
/// 对齐 sub2api UserMonitorView：primary_status + latest_quota。
fn parse_channels(value: &Value) -> Vec<ChannelStatus> {
    extract_items(value)
        .iter()
        .map(parse_channel)
        .collect()
}

fn extract_items(value: &Value) -> Vec<Value> {
    if let Some(arr) = value.as_array() {
        return arr.clone();
    }
    let data = value.get("data").unwrap_or(value);
    if let Some(arr) = data.as_array() {
        return arr.clone();
    }
    for key in ["items", "list", "channels", "monitors", "results"] {
        for node in [data, value] {
            if let Some(arr) = node.get(key).and_then(|v| v.as_array()) {
                return arr.clone();
            }
        }
    }
    Vec::new()
}

fn parse_channel(item: &Value) -> ChannelStatus {
    let name = ["group_name", "name", "channel_name", "title", "channel", "primary_model", "id"]
        .iter()
        .find_map(|k| item.get(*k).and_then(as_string).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "未知渠道".to_string());

    let metrics = item.get("primary_metrics");
    let (online, status) = parse_status(item);
    let (plan_level, tiers, balances) = parse_quota(item);

    let model = item
        .get("primary_model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let provider = item
        .get("provider")
        .and_then(|v| v.as_str())
        .and_then(Provider::from_id)
        .or_else(|| model.as_deref().and_then(detect_provider))
        .or_else(|| detect_provider(&name));

    let availability = pick_number(item, "availability_7d")
        .or_else(|| pick_number(item, "availability"))
        .or_else(|| metrics.and_then(|m| pick_number(m, "availability_pct")));
    let latency_ms = pick_number(item, "primary_latency_ms")
        .or_else(|| pick_number(item, "latency_ms"))
        .or_else(|| metrics.and_then(|m| pick_number(m, "total_latency_p50_ms")))
        .map(|v| v as i64);

    let remark = ["message", "error", "remark", "desc"]
        .iter()
        .find_map(|k| item.get(*k).and_then(|v| v.as_str()))
        .filter(|s| !s.is_empty());
    let mut detail = format_channel_detail(model.as_deref().unwrap_or(""), latency_ms, availability);
    if let Some(r) = remark {
        if !detail.is_empty() {
            detail.push_str(" · ");
        }
        detail.push_str(r);
    }

    // timeline[]：历史检测记录（新→旧），状态映射为站点自身使用的权重
    let mut trend: Vec<TrendPoint> = item
        .get("timeline")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let t = c.get("checked_at")?.as_str()?.to_string();
                    let v = match c.get("status").and_then(|s| s.as_str())? {
                        "operational" => 100.0,
                        "degraded" => 65.0,
                        "error" | "failed" => 35.0,
                        "empty" => 15.0,
                        _ => return None,
                    };
                    Some(TrendPoint { t, v })
                })
                .collect()
        })
        .unwrap_or_default();
    trend.reverse();

    ChannelStatus {
        name,
        online,
        detail,
        status,
        plan_level,
        provider: provider.map(Provider::id).map(str::to_string),
        model,
        availability,
        latency_ms,
        tiers,
        balances,
        trend: (!trend.is_empty()).then_some(trend),
    }
}

fn rank_channels(mut channels: Vec<ChannelStatus>) -> Vec<ChannelStatus> {
    sort_by_success_rate(&mut channels, |c| c.availability);
    channels
}

fn parse_status(item: &Value) -> (bool, String) {
    let raw = item
        .get("primary_status")
        .or_else(|| item.get("status"))
        .or_else(|| item.get("state"))
        .or_else(|| item.get("online"))
        .or_else(|| item.get("is_online"));

    match raw {
        Some(Value::String(s)) => match s.to_lowercase().as_str() {
            "operational" | "online" | "up" | "active" | "enabled" | "ok" | "healthy"
            | "true" | "1" | "success" => (true, "operational".into()),
            "degraded" | "warning" | "warn" | "slow" => (true, "degraded".into()),
            "failed" | "error" | "offline" | "down" | "disabled" | "false" | "0" => {
                (false, "failed".into())
            }
            other => (false, other.to_string()),
        },
        Some(Value::Bool(true)) => (true, "operational".into()),
        Some(Value::Bool(false)) => (false, "failed".into()),
        Some(Value::Number(n)) if n.as_i64() == Some(1) => (true, "operational".into()),
        Some(Value::Number(n)) if n.as_i64() == Some(0) => (false, "failed".into()),
        _ => (false, "unknown".into()),
    }
}

fn parse_quota(item: &Value) -> (Option<String>, Vec<QuotaTier>, Vec<ChannelBalance>) {
    let quota = item
        .get("latest_quota")
        .or_else(|| item.get("quota"))
        .filter(|v| !v.is_null() && v.is_object());
    let Some(q) = quota else {
        return (None, Vec::new(), Vec::new());
    };

    let plan_level = q
        .get("plan_level")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    let tiers = q
        .get("tiers")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_tier).collect())
        .unwrap_or_default();

    let mut balances = Vec::new();
    if let Some(arr) = q.get("balances").and_then(|v| v.as_array()) {
        for b in arr {
            if let Some(balance) = pick_number(b, "balance") {
                let currency = b
                    .get("currency")
                    .and_then(|v| v.as_str())
                    .unwrap_or("USD")
                    .to_string();
                balances.push(ChannelBalance { currency, balance });
            }
        }
    }
    if balances.is_empty() {
        if let Some(balance) = pick_number(q, "balance") {
            let currency = q
                .get("currency")
                .and_then(|v| v.as_str())
                .unwrap_or("USD")
                .to_string();
            balances.push(ChannelBalance { currency, balance });
        }
    }

    (plan_level, tiers, balances)
}

fn parse_tier(value: &Value) -> Option<QuotaTier> {
    let window = value
        .get("window")
        .and_then(as_string)
        .unwrap_or_else(|| "total".to_string());
    let used_percent = pick_number(value, "used_percent")?;
    Some(QuotaTier {
        window,
        label: value
            .get("label")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        used_percent,
        used: pick_number(value, "used"),
        limit: pick_number(value, "limit"),
        reset_at: value
            .get("reset_at")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
    })
}

fn as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn pick_number(obj: &Value, key: &str) -> Option<f64> {
    obj.get(key).and_then(as_number)
}

fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_operational_with_quota_and_balance() {
        let json = serde_json::json!({
            "code": 0,
            "data": [{
                "id": 1,
                "name": "Claude",
                "primary_model": "claude-sonnet-4",
                "primary_status": "operational",
                "primary_latency_ms": 120,
                "availability_7d": 0.992,
                "latest_quota": {
                    "source": "usage",
                    "success": true,
                    "plan_level": "Pro",
                    "balance": 12.5,
                    "currency": "USD",
                    "tiers": [
                        { "window": "5h", "used_percent": 42.0, "reset_at": "2026-08-18T12:00:00Z" },
                        { "window": "7d", "used_percent": 78.3, "used": 78.3, "limit": 100.0 }
                    ]
                }
            }]
        });
        let channels = parse_channels(&json);
        assert_eq!(channels.len(), 1);
        let ch = &channels[0];
        assert!(ch.online);
        assert_eq!(ch.status, "operational");
        assert_eq!(ch.name, "Claude");
        assert_eq!(ch.plan_level.as_deref(), Some("Pro"));
        assert_eq!(ch.tiers.len(), 2);
        assert_eq!(ch.tiers[0].window, "5h");
        assert!((ch.tiers[1].used_percent - 78.3).abs() < f64::EPSILON);
        assert_eq!(ch.balances.len(), 1);
        assert!((ch.balances[0].balance - 12.5).abs() < f64::EPSILON);
        assert_eq!(site_balance_from_channels(&channels), Some(12.5));
    }

    #[test]
    fn parse_failed_and_degraded() {
        let json = serde_json::json!([
            { "name": "A", "primary_status": "failed" },
            { "name": "B", "primary_status": "degraded" }
        ]);
        let channels = parse_channels(&json);
        assert!(!channels[0].online);
        assert_eq!(channels[0].status, "failed");
        assert!(channels[1].online);
        assert_eq!(channels[1].status, "degraded");
    }

    #[test]
    fn parse_skips_empty_group_name() {
        let json = serde_json::json!({
            "data": {
                "items": [{
                    "group_name": "",
                    "name": "ChatGPT-Plus【高并发-特惠通道】",
                    "primary_model": "gpt-5.6-sol",
                    "primary_status": "error",
                    "availability_7d": 86.5
                }]
            }
        });
        let channels = parse_channels(&json);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].name, "ChatGPT-Plus【高并发-特惠通道】");
        assert_eq!(channels[0].status, "failed");
    }

    #[test]
    fn parse_data_items_wrapper() {
        let json = serde_json::json!({
            "code": 0,
            "data": {
                "items": [{
                    "group_name": "Claude｜Kiro 分组(power)",
                    "name": "Claude｜Kiro 分组(power)｜内部健康监控",
                    "provider": "anthropic",
                    "primary_model": "claude-opus-5",
                    "primary_status": "operational",
                    "primary_latency_ms": 1531,
                    "availability_7d": 98.8
                }]
            }
        });
        let channels = parse_channels(&json);
        assert_eq!(channels.len(), 1);
        assert!(channels[0].online);
        assert_eq!(channels[0].name, "Claude｜Kiro 分组(power)");
        assert_eq!(channels[0].provider.as_deref(), Some("claude"));
        assert_eq!(channels[0].model.as_deref(), Some("claude-opus-5"));
        assert_eq!(channels[0].detail, "claude-opus-5 · 1531ms · 98.8%");
        assert!((channels[0].availability.unwrap() - 98.8).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_access_token_and_balance() {
        let json = serde_json::json!({
            "code": 0,
            "data": {
                "access_token": "tok",
                "refresh_token": "ref",
                "expires_in": 86400,
                "user": { "balance": -0.004 }
            }
        });
        let token = parse_token(&json).expect("应解析到 token");
        assert_eq!(token.auth_token, "tok");
        assert_eq!(token.refresh_token.as_deref(), Some("ref"));
        assert!(token.expires_at > now_secs());
        assert!((token.balance.unwrap() + 0.004).abs() < 0.0001);
    }

    #[test]
    fn rank_channels_sorts_by_success() {
        let json = serde_json::json!([
            {
                "name": "slow-claude",
                "primary_model": "claude-sonnet-4-6",
                "primary_status": "degraded",
                "primary_latency_ms": 3114,
                "availability_7d": 36.5
            },
            {
                "name": "fast-gpt",
                "primary_model": "gpt-5.4",
                "primary_status": "operational",
                "primary_latency_ms": 800,
                "availability_7d": 99.1
            },
            {
                "name": "other",
                "primary_model": "deepseek-v3",
                "primary_status": "operational",
                "availability_7d": 100.0
            }
        ]);
        let channels = rank_channels(parse_channels(&json));
        assert_eq!(channels.len(), 3);
        assert_eq!(channels[0].model.as_deref(), Some("deepseek-v3"));
        assert_eq!(channels[1].model.as_deref(), Some("gpt-5.4"));
        assert_eq!(channels[2].model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(channels[2].detail, "claude-sonnet-4-6 · 3114ms · 36.5%");
    }

    #[test]
    fn parse_timeline_into_trend() {
        let json = serde_json::json!([{
            "name": "A",
            "primary_status": "operational",
            "timeline": [
                { "status": "operational", "checked_at": "2026-08-22T08:00:00Z" },
                { "status": "error", "checked_at": "2026-08-22T07:00:00Z" },
                { "status": "degraded", "checked_at": "2026-08-22T06:00:00Z" }
            ]
        }]);
        let channels = parse_channels(&json);
        let trend = channels[0].trend.as_ref().expect("应有趋势");
        // timeline 新→旧，解析后应翻转为时间升序
        assert_eq!(trend.len(), 3);
        assert_eq!(trend[0].t, "2026-08-22T06:00:00Z");
        assert!((trend[0].v - 65.0).abs() < f64::EPSILON);
        assert!((trend[1].v - 35.0).abs() < f64::EPSILON);
        assert!((trend[2].v - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_timeline_single_point_keeps_trend() {
        // 新建监控只有 1 条检测记录，也应展示趋势（前端画平线）
        let json = serde_json::json!([{
            "name": "A",
            "primary_status": "operational",
            "timeline": [
                { "status": "operational", "checked_at": "2026-08-22T08:00:00Z" }
            ]
        }]);
        let channels = parse_channels(&json);
        let trend = channels[0].trend.as_ref().expect("单点趋势应保留");
        assert_eq!(trend.len(), 1);
        assert_eq!(trend[0].t, "2026-08-22T08:00:00Z");
        assert!((trend[0].v - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_multi_currency_balances() {
        let json = serde_json::json!([{
            "name": "CN",
            "primary_status": "operational",
            "latest_quota": {
                "success": true,
                "balances": [
                    { "currency": "CNY", "balance": 8.2 },
                    { "currency": "USD", "balance": 1.1 }
                ]
            }
        }]);
        let channels = parse_channels(&json);
        assert_eq!(channels[0].balances.len(), 2);
        assert_eq!(site_balance_from_channels(&channels), Some(1.1));
    }

    #[test]
    fn parse_v2_matrix_items() {
        let json = serde_json::json!({
            "code": 0,
            "data": {
                "group_by": "platform_group",
                "items": [
                    {
                        "platform": "anthropic",
                        "group_id": 11,
                        "group_name": "kiro",
                        "metrics": {
                            "success_rate": 0.040,
                            "ttft": { "p50_ms": null, "sample_count": 0 }
                        },
                        "health": { "overall": "critical", "score": 33.0 }
                    },
                    {
                        "platform": "openai",
                        "group_id": 12,
                        "group_name": "GPT 稳定分组",
                        "metrics": {
                            "success_rate": 0.248,
                            "ttft": { "p50_ms": 10000 }
                        },
                        "health": { "overall": "warning", "score": 51.9 }
                    }
                ]
            }
        });
        let channels = parse_v2_matrix(&json);
        assert_eq!(channels.len(), 2);
        let kiro = channels.iter().find(|c| c.name == "kiro").unwrap();
        assert!(!kiro.online);
        assert_eq!(kiro.status, "failed");
        assert_eq!(kiro.provider.as_deref(), Some("claude"));
        assert!((kiro.availability.unwrap() - 0.04).abs() < 1e-9);
        let gpt = channels.iter().find(|c| c.name == "GPT 稳定分组").unwrap();
        assert!(gpt.online);
        assert_eq!(gpt.status, "degraded");
        assert_eq!(gpt.provider.as_deref(), Some("gpt"));
        assert_eq!(gpt.latency_ms, Some(10000));
        assert_eq!(gpt.detail, "10000ms · 24.8%");
    }
}

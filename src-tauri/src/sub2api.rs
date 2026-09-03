use base64::Engine as _;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashSet;

use crate::config::{MonitorModels, SiteConfig};
use crate::http::truncate;
use crate::models::{
    detect_provider, format_channel_detail, is_non_chat_model, normalize_model,
    sort_by_success_rate, Provider,
};
use crate::state::{
    now_millis, now_secs, AppState, ChannelBalance, ChannelStatus, QuotaTier, SiteResult,
    TokenCache, TrendPoint,
};

/// sub2api 站点采集：确保登录态 -> 拉取渠道监控列表；401 时清缓存重登一次
pub async fn check(
    client: &Client,
    site: &SiteConfig,
    monitor: &MonitorModels,
    state: &AppState,
) -> SiteResult {
    let mut token = match ensure_token(client, site, state).await {
        Ok(t) => t,
        Err(e) => return SiteResult::error(site, e),
    };

    let base = site.base_url.trim_end_matches('/');
    let url = format!("{base}/api/v1/channel-monitors?_ts={}", now_millis());

    let (status, body) = match fetch_monitors(client, &url, &token.auth_token).await {
        Ok(r) => r,
        Err(e) => return SiteResult::error(site, e),
    };

    // 401：令牌失效时清缓存重新登录后再取一次
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
        return SiteResult::error(
            site,
            "channel-monitors 返回 401，令牌已失效；请重新「浏览器登录」或检查账号密码".to_string(),
        );
    }

    // 倍率接口与监控数据互不依赖，提前并发请求，避免额外串行等待。
    let rates_client = client.clone();
    let rates_base = base.to_string();
    let rates_token = token.auth_token.clone();
    let group_rates_h = tauri::async_runtime::spawn(async move {
        fetch_group_rates(&rates_client, &rates_base, &rates_token).await
    });

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
        let parsed = parse_channels(&value);
        channels = rank_channels(filter_monitored_channels(parsed.clone(), monitor));
        // 站点探测模型与监控配置版本不同步时（如站点监控 grok-4.5 而配置监控 grok-4.6），
        // 精确匹配会清空全部渠道；回退到按模型族匹配，避免整站被误判为“未返回渠道监控”
        if channels.is_empty() && !parsed.is_empty() {
            channels = rank_channels(filter_channels_by_family(parsed, monitor));
            if !channels.is_empty() {
                result.note =
                    Some("站点探测模型与监控配置不一致，已按模型族匹配展示渠道".to_string());
            }
        }
        // 字段结构未最终确定前，保留原始响应片段便于调试（调试开关关闭时由 lib.rs 剥离）
        result.raw = Some(truncate(&body, 2000));
    } else {
        active_error = Some(format!("HTTP {status}：{}", truncate(body.trim(), 200)));
    }

    // 主动监控无数据或不可用时，回退到 V2 被动监控（部分站点数据只在此接口）
    if channels.is_empty() {
        if let Ok(v2) = fetch_v2_matrix(client, base, &token.auth_token).await {
            if !v2.is_empty() {
                channels = rank_channels(filter_monitored_channels(v2, monitor));
            }
        }
    }

    let group_rates = group_rates_h
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    apply_group_rates(&mut channels, &group_rates);

    // 渠道没带余额（站点未开配额探测、渠道被过滤、V2 被动监控）时，拉用户信息兜底：
    // 浏览器登录捕获的令牌不含余额，密码登录缓存的余额也会随 refresh 响应丢失
    result.balance_usd = match site_balance_from_channels(&channels) {
        Some(balance) => Some(balance),
        None => fetch_user_balance(client, base, &token.auth_token)
            .await
            .or(token.balance),
    };
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

fn filter_monitored_channels(
    channels: Vec<ChannelStatus>,
    monitor: &MonitorModels,
) -> Vec<ChannelStatus> {
    let wanted = monitor.all_names();
    channels
        .into_iter()
        .filter(|channel| {
            if let Some(model) = channel.model.as_deref() {
                if is_non_chat_model(model) {
                    return false;
                }
                let normalized = normalize_model(model);
                return wanted
                    .iter()
                    .any(|item| normalize_model(item) == normalized);
            }
            match channel.provider.as_deref().and_then(Provider::from_id) {
                Some(Provider::Gpt) => !monitor.gpt.is_empty(),
                Some(Provider::Claude) => !monitor.claude.is_empty(),
                Some(Provider::Grok) => !monitor.grok.is_empty(),
                Some(Provider::Kimi) => !monitor.kimi.is_empty(),
                Some(Provider::Gemini) => !monitor.gemini.is_empty(),
                Some(Provider::Qwen) => !monitor.qwen.is_empty(),
                Some(Provider::Deepseek) => !monitor.deepseek.is_empty(),
                None => false,
            }
        })
        .collect()
}

/// 按模型族兜底匹配：渠道探测模型可识别出已监控的厂商（或渠道 provider 字段属于已监控厂商）即保留。
/// 精确匹配全部落空时使用——渠道探测用的模型版本常与监控配置不一致，但渠道本身仍是相关厂商的。
fn filter_channels_by_family(
    channels: Vec<ChannelStatus>,
    monitor: &MonitorModels,
) -> Vec<ChannelStatus> {
    channels
        .into_iter()
        .filter(|channel| {
            if let Some(model) = channel.model.as_deref() {
                if is_non_chat_model(model) {
                    return false;
                }
                // 渠道的 provider 字段可能只是兼容口径（如 DeepSeek 渠道标成 openai），
                // 优先按模型名识别厂商
                if let Some(provider) = detect_provider(model) {
                    return monitor.watches(provider);
                }
            }
            channel
                .provider
                .as_deref()
                .and_then(Provider::from_id)
                .is_some_and(|provider| monitor.watches(provider))
        })
        .collect()
}

/// V2 被动监控：按分组聚合的成功率/错误率/首 Token 延迟
/// GET /api/v1/channel-monitor-v2/matrix?range=24h
async fn fetch_v2_matrix(
    client: &Client,
    base: &str,
    token: &str,
) -> Result<Vec<ChannelStatus>, String> {
    let url = format!(
        "{base}/api/v1/channel-monitor-v2/matrix?range=24h&_ts={}",
        now_millis()
    );
    let (status, body) = crate::http::authorized_get(client, &url, Some(token), None, None)
        .await
        .map_err(|e| format!("V2 监控请求失败: {e}"))?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let value: Value =
        serde_json::from_str(&body).map_err(|e| format!("V2 监控响应不是合法 JSON: {e}"))?;
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
            // 分组名通常含模型族关键词；platform 是平台口径，DeepSeek 等渠道可能标成 openai
            let provider = detect_provider(&name).or_else(|| Provider::from_id(platform));
            ChannelStatus {
                name,
                label: None,
                online,
                detail: format_channel_detail("", latency_ms, availability),
                status: status.into(),
                plan_level: None,
                provider: provider.map(Provider::id).map(str::to_string),
                model: None,
                availability,
                latency_ms,
                tiers: Vec::new(),
                balances: Vec::new(),
                model_ratio: None,
                trend,
            }
        })
        .collect()
}

/// 拉取渠道监控列表。交互刷新快速失败，下一分钟会自动重试。
async fn fetch_monitors(client: &Client, url: &str, token: &str) -> Result<(u16, String), String> {
    crate::http::authorized_get(client, url, Some(token), None, None).await
}

/// 用户可访问分组的倍率。用户级覆盖（/groups/rates）优先于分组默认倍率。
async fn fetch_group_rates(
    client: &Client,
    base: &str,
    token: &str,
) -> Result<Vec<GroupRate>, String> {
    let request_id = now_millis();
    let available_url = format!("{base}/api/v1/groups/available?_ts={request_id}");
    let rates_url = format!("{base}/api/v1/groups/rates?_ts={request_id}");
    let available_client = client.clone();
    let available_token = token.to_string();
    let available_h = tauri::async_runtime::spawn(async move {
        crate::http::authorized_get(
            &available_client,
            &available_url,
            Some(&available_token),
            None,
            None,
        )
        .await
    });
    let overrides_client = client.clone();
    let overrides_token = token.to_string();
    let overrides_h = tauri::async_runtime::spawn(async move {
        crate::http::authorized_get(
            &overrides_client,
            &rates_url,
            Some(&overrides_token),
            None,
            None,
        )
        .await
    });

    let (status, body) = available_h
        .await
        .map_err(|e| format!("分组倍率任务失败: {e}"))?
        .map_err(|e| format!("分组倍率请求失败: {e}"))?;
    if !(200..300).contains(&status) {
        return Err(format!("分组倍率请求返回 HTTP {status}"));
    }
    let available: Value =
        serde_json::from_str(&body).map_err(|e| format!("分组倍率响应不是合法 JSON: {e}"))?;
    let overrides = overrides_h
        .await
        .ok()
        .and_then(Result::ok)
        .filter(|(status, _)| (200..300).contains(status))
        .and_then(|(_, body)| serde_json::from_str::<Value>(&body).ok())
        .unwrap_or(Value::Null);
    Ok(parse_group_rates(&available, &overrides))
}

/// 一个可访问分组的倍率：保留原始名，供分级匹配使用
struct GroupRate {
    name: String,
    rate: f64,
}

fn parse_group_rates(available: &Value, overrides: &Value) -> Vec<GroupRate> {
    let override_data = overrides.get("data").unwrap_or(overrides);
    let mut rates = Vec::new();
    for group in extract_items(available) {
        let Some(name) = group.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let id = group.get("id").and_then(as_string);
        let overridden = id
            .as_deref()
            .and_then(|id| override_data.get(id))
            .and_then(as_number);
        if let Some(rate) = overridden.or_else(|| pick_number(&group, "rate_multiplier")) {
            rates.push(GroupRate {
                name: name.to_string(),
                rate,
            });
        }
    }
    rates
}

fn normalize_group_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn simplified_group_name(name: &str) -> String {
    let mut depth = 0_u32;
    let without_notes: String = normalize_group_name(name)
        .chars()
        .filter(|c| match c {
            '(' | '（' => {
                depth += 1;
                false
            }
            ')' | '）' => {
                depth = depth.saturating_sub(1);
                false
            }
            _ => depth == 0,
        })
        .collect();
    // 部分站点修改了分组显示名，但旧监控仍保留 free/version 等描述。
    without_notes.replace("free", "")
}

/// 渠道分组与站点分组的显示名经常不同步（站点加后缀、改描述、整个分组改名/删除），
/// 倍率按缺宁勿滥原则多级匹配，任一级命中即止：
/// 1-4 渠道显示名：归一化全等 -> 去 free/括号备注后全等 -> 渠道词元全部出现在分组词元中（唯一
///    “最贴近”者）-> 【】前缀段全等；3/4 仅在唯一候选时采用，多个并列宁缺毋滥。
/// 5   监控标签（用户自建监控时的名称，常含池子备注）重复 1-4 级。
/// 6-7 词元重叠兜底：显示名/标签与分组名的共享词元 ≥2 且唯一最大者才采用。
/// 8   最后从标签里直接解析标注的倍率（x0.25 / 1倍 / 0.08）。
fn apply_group_rates(channels: &mut [ChannelStatus], rates: &[GroupRate]) {
    let groups: Vec<PreparedGroup> = rates
        .iter()
        .map(|g| PreparedGroup {
            normalized: normalize_group_name(&g.name),
            simplified: simplified_group_name(&g.name),
            tokens: name_tokens(&g.name),
            bracket: bracket_content(&g.name),
            rate: g.rate,
        })
        .collect();
    for channel in channels {
        channel.model_ratio =
            match_rate_by_name(&groups, &channel.name, &name_tokens(&channel.name))
                .or_else(|| {
                    channel
                        .label
                        .as_deref()
                        .and_then(|label| match_rate_by_name(&groups, label, &label_tokens(label)))
                })
                .or_else(|| match_group_by_overlap(&groups, &name_tokens(&channel.name)))
                .or_else(|| {
                    channel
                        .label
                        .as_deref()
                        .and_then(|label| match_group_by_overlap(&groups, &label_tokens(label)))
                })
                .or_else(|| channel.label.as_deref().and_then(label_rate));
    }
}

/// 对单个名称跑 1-4 级匹配；tokens 由调用方传入（标签词元需先剔除倍率标记）。
fn match_rate_by_name(groups: &[PreparedGroup], name: &str, tokens: &[String]) -> Option<f64> {
    let normalized = normalize_group_name(name);
    let simplified = simplified_group_name(name);
    let bracket = bracket_content(name);

    groups
        .iter()
        .find(|g| g.normalized == normalized)
        .map(|g| g.rate)
        .or_else(|| {
            let mut matches = groups
                .iter()
                .filter(|g| g.simplified == simplified)
                .map(|g| g.rate);
            let first = matches.next()?;
            matches.next().is_none().then_some(first)
        })
        .or_else(|| match_group_by_tokens(groups, tokens))
        .or_else(|| match_group_by_bracket(groups, bracket.as_deref()))
}

struct PreparedGroup {
    normalized: String,
    simplified: String,
    tokens: Vec<String>,
    bracket: Option<String>,
    rate: f64,
}

/// 词元包含匹配：渠道名词元是分组名词元的子集，且“多出部分”最短的分组唯一时采用。
/// 不看【】段：站点改名常发生在【】内部（如【Grok 】→【Grok heavy】），交给子集+唯一性判别。
fn match_group_by_tokens(groups: &[PreparedGroup], tokens: &[String]) -> Option<f64> {
    if tokens.is_empty() {
        return None;
    }
    let mut candidates: Vec<&PreparedGroup> = groups
        .iter()
        .filter(|g| !g.tokens.is_empty() && tokens.iter().all(|t| g.tokens.contains(t)))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by_key(|g| extra_token_count(g, tokens));
    if candidates.len() > 1
        && extra_token_count(candidates[1], tokens) == extra_token_count(candidates[0], tokens)
    {
        return None;
    }
    Some(candidates[0].rate)
}

/// 【】前缀段全等匹配：站点改了分组描述但【】里的主名未变（如【DeepSeek 稳定】官方池 ↔【DeepSeek 稳定】国外版 3.5折）
fn match_group_by_bracket(groups: &[PreparedGroup], bracket: Option<&str>) -> Option<f64> {
    let bracket = bracket?;
    let mut matches = groups
        .iter()
        .filter(|g| g.bracket.as_deref() == Some(bracket))
        .map(|g| g.rate);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// 词元重叠兜底：渠道词元与分组词元的共享数 ≥2 且唯一最大时采用。
/// 用于分组被站点改到面目全非（如【DeepSeek/GLM】友商平价 ↔【deepseek-v4 】2.5折 友商）、
/// 子集匹配失效的场景；并列最大说明区分度不足，宁可缺失。
/// 词元按去重后计数（如「仅Claude客户端」与【Claude Code】会重复产出 claude）。
fn match_group_by_overlap(groups: &[PreparedGroup], tokens: &[String]) -> Option<f64> {
    let wanted: HashSet<&String> = tokens.iter().collect();
    let mut best: Option<(&PreparedGroup, usize)> = None;
    let mut ambiguous = false;
    for group in groups {
        let seen: HashSet<&String> = group.tokens.iter().collect();
        let shared = seen.iter().filter(|t| wanted.contains(**t)).count();
        if shared < 2 {
            continue;
        }
        match best {
            Some((_, best_shared)) if shared == best_shared => ambiguous = true,
            Some((_, best_shared)) if shared < best_shared => {}
            _ => {
                best = Some((group, shared));
                ambiguous = false;
            }
        }
    }
    (!ambiguous).then_some(best?.0.rate)
}

/// 标签词元：剔除 x0.25 这类倍率标记词元——标签里的倍率是备注，不是分组身份的一部分
fn label_tokens(label: &str) -> Vec<String> {
    name_tokens(label)
        .into_iter()
        .filter(|token| match token.strip_prefix('x') {
            Some(rest) => {
                !(rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit() || c == '.'))
            }
            None => true,
        })
        .collect()
}

/// 从监控标签解析用户标注的倍率：x0.25 / ×0.25 / *1.5 前缀式优先，其次「N倍」，
/// 最后独立小数（须带小数点且 ≤1，避免误吞模型版本号 5.6、上下文 500k 之类）。
fn label_rate(label: &str) -> Option<f64> {
    let lower = label.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let plausible = |rate: f64| rate > 0.0 && rate <= 100.0;

    // 前缀式：x / × / * 紧跟数字
    for (i, c) in chars.iter().enumerate() {
        if matches!(c, 'x' | '×' | '*') {
            if let Some((rate, _)) = parse_number_at(&chars, i + 1) {
                if plausible(rate) {
                    return Some(rate);
                }
            }
        }
    }
    // 「N倍」式
    for (i, c) in chars.iter().enumerate() {
        if *c == '倍' {
            let mut start = i;
            while start > 0 && (chars[start - 1].is_ascii_digit() || chars[start - 1] == '.') {
                start -= 1;
            }
            if start < i {
                let text: String = chars[start..i].iter().collect();
                if let Ok(rate) = text.parse::<f64>() {
                    if plausible(rate) {
                        return Some(rate);
                    }
                }
            }
        }
    }
    // 独立小数：段内仅数字和一个小数点，两端都不是字母数字，且值 ≤1
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut dots = 0;
        while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && dots == 0)) {
            if chars[i] == '.' {
                dots += 1;
            }
            i += 1;
        }
        let bounded_before = start == 0 || !chars[start - 1].is_ascii_alphanumeric();
        let bounded_after = i == chars.len() || !chars[i].is_ascii_alphanumeric();
        if dots == 1 && bounded_before && bounded_after {
            let text: String = chars[start..i].iter().collect();
            if let Ok(rate) = text.parse::<f64>() {
                if plausible(rate) && rate <= 1.0 {
                    return Some(rate);
                }
            }
        }
    }
    None
}

/// 从 chars[from] 起解析一段数字（可含一个小数点），返回 (值, 结束下标)
fn parse_number_at(chars: &[char], from: usize) -> Option<(f64, usize)> {
    let mut i = from;
    let mut dots = 0;
    while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && dots == 0)) {
        if chars[i] == '.' {
            dots += 1;
        }
        i += 1;
    }
    if i == from {
        return None;
    }
    let text: String = chars[from..i].iter().collect();
    text.parse::<f64>().ok().map(|v| (v, i))
}

fn extra_token_count(group: &PreparedGroup, tokens: &[String]) -> usize {
    group.tokens.iter().filter(|t| !tokens.contains(*t)).count()
}

/// 分词：小写后按字母数字（含小数点）连续段与 CJK 连续段切分，其余字符视为分隔符。
/// CJK 段拆成相邻二字词元：既保留「特惠」这类相邻性（子集匹配不至于过松），
/// 又容忍个别字改动（「特惠版」仍含「特惠」）。
fn name_tokens(name: &str) -> Vec<String> {
    let lower = name.to_lowercase();
    let mut parts: Vec<String> = Vec::new();
    let mut ascii = String::new();
    let mut cjk = String::new();
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            if !cjk.is_empty() {
                parts.push(std::mem::take(&mut cjk));
            }
            ascii.push(c);
        } else if is_cjk_token_char(c) {
            if !ascii.is_empty() {
                parts.push(std::mem::take(&mut ascii));
            }
            cjk.push(c);
        } else if !ascii.is_empty() || !cjk.is_empty() {
            if !ascii.is_empty() {
                parts.push(std::mem::take(&mut ascii));
            }
            if !cjk.is_empty() {
                parts.push(std::mem::take(&mut cjk));
            }
        }
    }
    if !ascii.is_empty() {
        parts.push(ascii);
    }
    if !cjk.is_empty() {
        parts.push(cjk);
    }
    let mut tokens = Vec::new();
    for part in parts {
        let chars: Vec<char> = part.chars().collect();
        if is_cjk_token_char(chars[0]) {
            if chars.len() >= 2 {
                for pair in chars.windows(2) {
                    tokens.push(pair.iter().collect::<String>());
                }
            }
            // 单个汉字不成词元：单字子集匹配判别力太弱
        } else {
            tokens.push(part);
        }
    }
    tokens
}

fn is_cjk_token_char(c: char) -> bool {
    let u = c as u32;
    (0x4E00..=0x9FFF).contains(&u)
        || (0x3400..=0x4DBF).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
}

/// 取名称里第一个【…】段的内容（去空白、小写）
fn bracket_content(name: &str) -> Option<String> {
    let start = name.find('【')?;
    let rest = &name[start + '【'.len_utf8()..];
    let end = rest.find('】')?;
    let content = &rest[..end];
    let normalized = normalize_group_name(content);
    (!normalized.is_empty()).then_some(normalized)
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

/// 用户信息里的账户余额（USD）。不同部署的用户信息路径不同，
/// 依次尝试 /user/profile 与 /auth/me；余额缺失或接口不可用时回退下一路径。
async fn fetch_user_balance(client: &Client, base: &str, token: &str) -> Option<f64> {
    for path in ["/api/v1/user/profile", "/api/v1/auth/me"] {
        let url = format!("{base}{path}?_ts={}", now_millis());
        let Ok((status, body)) =
            crate::http::authorized_get(client, &url, Some(token), None, None).await
        else {
            continue;
        };
        if !(200..300).contains(&status) {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&body) else {
            continue;
        };
        if let Some(balance) = parse_user_balance(&value) {
            return Some(balance);
        }
    }
    None
}

/// 宽松解析账户余额：balance 可能在顶层 / data / data.user 下
fn parse_user_balance(value: &Value) -> Option<f64> {
    let data = value.get("data").unwrap_or(value);
    let user = data.get("user").unwrap_or(data);
    pick_number(user, "balance").or_else(|| pick_number(data, "balance"))
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
        return Err("未配置账号密码，请在设置中补填，或使用「浏览器登录」获取令牌".to_string());
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
        let mut message = format!(
            "登录失败（HTTP {}）：{}",
            status,
            truncate(body.trim(), 200)
        );
        // 站点开启 Cloudflare Turnstile 人机验证时程序无法登录，提示用内嵌浏览器登录
        if body.to_ascii_lowercase().contains("turnstile") {
            message.push_str("；该站点开启了人机验证，请在设置中使用「浏览器登录」");
        }
        return Err(message);
    }

    let value: Value =
        serde_json::from_str(&body).map_err(|e| format!("登录响应不是合法 JSON: {e}"))?;
    parse_token(&value).ok_or_else(|| "登录响应中未找到 auth_token".to_string())
}

async fn refresh_token(
    client: &Client,
    site: &SiteConfig,
    refresh: &str,
) -> Result<TokenCache, String> {
    let url = format!(
        "{}/api/v1/auth/refresh",
        site.base_url.trim_end_matches('/')
    );
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
    let value: Value =
        serde_json::from_str(&body).map_err(|e| format!("refresh 响应不是合法 JSON: {e}"))?;
    parse_token(&value).ok_or_else(|| "refresh 响应中未找到 auth_token".to_string())
}

/// 宽松解析令牌响应：兼容顶层或 data 包裹，token / expires_at 字段名容错
fn parse_token(value: &Value) -> Option<TokenCache> {
    parse_token_with_ttl(value, 3600)
}

/// 内嵌浏览器登录回传的令牌：结构与登录响应同构；拿不到过期时间时
/// 放宽到 7 天，实际失效交给 401 检测兜底
pub fn parse_web_token(value: &Value) -> Option<TokenCache> {
    parse_token_with_ttl(value, 7 * 24 * 3600)
}

fn parse_token_with_ttl(value: &Value, default_ttl_secs: i64) -> Option<TokenCache> {
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
        // auth_token 是 JWT 时自带 exp（秒级时间戳），比默认 TTL 更准：
        // sub2api 的令牌实际只有 24h，按 7 天兜底会长期拿死令牌打接口
        .or_else(|| jwt_exp(&auth_token))
        .unwrap_or_else(|| now_secs() + default_ttl_secs);
    let user = data.get("user").unwrap_or(data);
    let balance = pick_number(user, "balance").or_else(|| pick_number(data, "balance"));
    Some(TokenCache {
        auth_token,
        refresh_token,
        expires_at,
        balance,
    })
}

/// 解码 JWT 载荷里的 exp（base64url 无 padding，形如 header.payload.signature）
fn jwt_exp(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    value.get("exp").and_then(|v| v.as_i64())
}

/// 宽松解析渠道列表：数组可能在顶层 / data / data.items / channels 下。
/// 对齐 sub2api UserMonitorView：primary_status + latest_quota。
fn parse_channels(value: &Value) -> Vec<ChannelStatus> {
    extract_items(value).iter().map(parse_channel).collect()
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
    let name = [
        "group_name",
        "name",
        "channel_name",
        "title",
        "channel",
        "primary_model",
        "id",
    ]
    .iter()
    .find_map(|k| item.get(*k).and_then(as_string).filter(|s| !s.is_empty()))
    .unwrap_or_else(|| "未知渠道".to_string());

    // 监控条目的 name 是用户自建的监控标签（常含 x0.25 等倍率与池子备注），
    // 与 group_name 不同时保留下来，供倍率匹配兜底（分组被站点改名/删除后 group_name 已对不上）
    let label = item
        .get("name")
        .and_then(as_string)
        .filter(|s| !s.is_empty())
        .filter(|_| item.get("group_name").is_some())
        .filter(|s| normalize_group_name(s) != normalize_group_name(&name));

    let metrics = item.get("primary_metrics");
    let (online, status) = parse_status(item);
    let (plan_level, tiers, balances) = parse_quota(item);

    let model = item
        .get("primary_model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    // provider 字段可能只是兼容口径（如 DeepSeek 渠道标成 openai），
    // 按模型名 / 渠道名识别模型族，provider 字段仅作最后兜底。
    let provider = model
        .as_deref()
        .and_then(detect_provider)
        .or_else(|| detect_provider(&name))
        .or_else(|| {
            item.get("provider")
                .and_then(|v| v.as_str())
                .and_then(Provider::from_id)
        });

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
    let mut detail =
        format_channel_detail(model.as_deref().unwrap_or(""), latency_ms, availability);
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
    // 不依赖接口返回顺序；同一时间点保留最后一条，确保刷新后最新点立即替换旧点。
    trend.sort_by(|a, b| a.t.cmp(&b.t));
    trend.dedup_by(|a, b| {
        if a.t == b.t {
            a.v = b.v;
            true
        } else {
            false
        }
    });

    ChannelStatus {
        name,
        label,
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
        model_ratio: None,
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
            "operational" | "online" | "up" | "active" | "enabled" | "ok" | "healthy" | "true"
            | "1" | "success" => (true, "operational".into()),
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
    fn parse_user_balance_handles_wrappers() {
        // /user/profile 响应：data 直接是用户对象
        assert_eq!(
            parse_user_balance(&serde_json::json!({
                "code": 0,
                "data": { "email": "a@b.c", "balance": 12.3, "frozen_balance": 0.5 }
            })),
            Some(12.3)
        );
        // /auth/me 响应：data.user 包裹
        assert_eq!(
            parse_user_balance(&serde_json::json!({
                "code": 0,
                "data": { "user": { "balance": -0.004 } }
            })),
            Some(-0.004)
        );
        // 数字字符串也能解析（as_number 容错）
        assert_eq!(
            parse_user_balance(&serde_json::json!({ "balance": "3.2" })),
            Some(3.2)
        );
        assert_eq!(
            parse_user_balance(&serde_json::json!({ "code": 0, "data": {} })),
            None
        );
    }

    #[test]
    fn web_token_uses_jwt_exp_over_default_ttl() {
        // 载荷 {"user_id":260,"exp":2000000000,"iat":1788406708}
        let jwt = format!("eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJ1c2VyX2lkIjoyNjAsImV4cCI6MjAwMDAwMDAwMCwiaWF0IjoxNzg4NDA2NzA4fQ.sig");
        let json = serde_json::json!({ "auth_token": jwt });
        let token = parse_web_token(&json).expect("应解析到 token");
        assert_eq!(token.expires_at, 2_000_000_000);
    }

    #[test]
    fn web_token_expired_jwt_not_treated_as_long_lived() {
        // 载荷 {"user_id":260,"exp":1000000000,"iat":999999999}
        let jwt = format!("eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJ1c2VyX2lkIjoyNjAsImV4cCI6MTAwMDAwMDAwMCwiaWF0Ijo5OTk5OTk5OTl9.sig");
        let json = serde_json::json!({ "auth_token": jwt });
        let token = parse_web_token(&json).expect("应解析到 token");
        // 过期 JWT 不应被当成还有 7 天寿命，ensure_token 会立刻走 refresh/login
        assert!(token.expires_at <= now_secs() + 60);
    }

    #[test]
    fn jwt_exp_handles_non_jwt_and_garbage() {
        assert_eq!(jwt_exp("not-a-jwt"), None);
        assert_eq!(jwt_exp("a.!!!.b"), None);
        // 载荷不含 exp：{"user_id":1}
        assert_eq!(jwt_exp("e30.eyJ1c2VyX2lkIjoxfQ.c"), None);
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

    #[test]
    fn parse_channel_prefers_model_family_over_compatible_provider_field() {
        // sub2api 把 DeepSeek 渠道的 provider 标成 openai（兼容口径），应按模型名归入 deepseek
        let json = serde_json::json!([
            {
                "name": "【DeepSeek 稳定】官方池",
                "provider": "openai",
                "primary_model": "deepseek-v4-flash",
                "primary_status": "degraded",
                "primary_latency_ms": 6112,
                "availability_7d": 96.8
            }
        ]);
        let channels = parse_channels(&json);
        assert_eq!(channels[0].provider.as_deref(), Some("deepseek"));
    }

    #[test]
    fn v2_matrix_prefers_group_name_over_generic_platform() {
        let json = serde_json::json!({
            "data": {
                "items": [
                    {
                        "platform": "openai",
                        "group_name": "DeepSeek 稳定",
                        "metrics": { "success_rate": 0.97, "ttft": { "p50_ms": 6112 } },
                        "health": { "overall": "warning" }
                    }
                ]
            }
        });
        let channels = parse_v2_matrix(&json);
        assert_eq!(channels[0].provider.as_deref(), Some("deepseek"));
    }

    #[test]
    fn monitored_filter_requires_exact_chat_model() {
        let json = serde_json::json!([
            { "name": "chat", "provider": "openai", "primary_model": "gpt-5.6-sol", "primary_status": "operational" },
            { "name": "compact", "provider": "openai", "primary_model": "gpt-5.6-sol-openai-compact", "primary_status": "operational" },
            { "name": "image", "provider": "openai", "primary_model": "gpt-image-2", "primary_status": "operational" }
        ]);
        let filtered = filter_monitored_channels(parse_channels(&json), &MonitorModels::default());
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "chat");
        assert_eq!(filtered[0].model_ratio, None);
    }

    #[test]
    fn family_fallback_keeps_off_version_probe_models() {
        // 站点探测模型版本与监控配置不一致（监控 grok-4.6 但站点监控 grok-4.5 等）：
        // 精确匹配清空渠道时，按模型族兜底保留同厂商渠道
        let json = serde_json::json!([
            { "name": "Grok｜0.15", "provider": "grok", "primary_model": "grok-4.5", "primary_status": "operational" },
            { "name": "GPT Pro｜特惠", "provider": "openai", "primary_model": "gpt-5.5", "primary_status": "operational" },
            { "name": "Claude Max", "provider": "anthropic", "primary_model": "claude-opus-4-6", "primary_status": "operational" },
            { "name": "DeepSeek｜官方池", "provider": "openai", "primary_model": "deepseek-v4-flash", "primary_status": "degraded" }
        ]);
        let parsed = parse_channels(&json);
        let monitor = serde_json::from_value::<MonitorModels>(serde_json::json!({
            "gpt": ["gpt-5.6-sol"],
            "claude": ["claude-opus-5"],
            "grok": ["grok-4.6"],
            "deepseek": ["deepseek-chat"]
        }))
        .unwrap();
        assert!(filter_monitored_channels(parsed.clone(), &monitor).is_empty());
        let filtered = filter_channels_by_family(parsed, &monitor);
        assert_eq!(filtered.len(), 4);
        assert!(filtered
            .iter()
            .any(|c| c.provider.as_deref() == Some("grok")));
        // DeepSeek 渠道 provider 字段标成 openai，但按模型名识别为 deepseek 族
        assert!(filtered
            .iter()
            .any(|c| c.model.as_deref() == Some("deepseek-v4-flash")));
    }

    #[test]
    fn family_fallback_respects_unwatched_families_and_non_chat() {
        let json = serde_json::json!([
            { "name": "Grok｜0.15", "provider": "grok", "primary_model": "grok-4.5", "primary_status": "operational" },
            { "name": "Kimi｜直连", "provider": "kimi", "primary_status": "operational" },
            { "name": "image", "provider": "openai", "primary_model": "dall-e-3", "primary_status": "operational" }
        ]);
        let monitor = serde_json::from_value::<MonitorModels>(serde_json::json!({
            "gpt": ["gpt-5.6-sol"]
        }))
        .unwrap();
        let filtered = filter_channels_by_family(parse_channels(&json), &monitor);
        assert!(filtered.is_empty());

        let monitor_grok = serde_json::from_value::<MonitorModels>(serde_json::json!({
            "grok": ["grok-4.6"]
        }))
        .unwrap();
        let filtered = filter_channels_by_family(parse_channels(&json), &monitor_grok);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Grok｜0.15");

        let monitor_kimi = serde_json::from_value::<MonitorModels>(serde_json::json!({
            "kimi": ["kimi-k3"]
        }))
        .unwrap();
        let filtered = filter_channels_by_family(parse_channels(&json), &monitor_kimi);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Kimi｜直连");
    }

    #[test]
    fn group_rates_use_user_override_and_normalized_name() {
        let groups = serde_json::json!({
            "data": [
                { "id": 9, "name": "GPT 稳定分组 ", "rate_multiplier": 0.08, "platform": "openai" },
                { "id": 18, "name": "Grok free", "rate_multiplier": 0.04, "platform": "grok" }
            ]
        });
        let overrides = serde_json::json!({ "data": { "9": 0.06 } });
        let rates = parse_group_rates(&groups, &overrides);
        let rate_of = |key: &str| {
            rates
                .iter()
                .find(|g| normalize_group_name(&g.name) == key)
                .map(|g| g.rate)
        };
        assert_eq!(rate_of("gpt稳定分组"), Some(0.06));
        assert_eq!(rate_of("grokfree"), Some(0.04));

        let mut channels = parse_v2_matrix(&serde_json::json!({
            "data": { "items": [{
                "platform": "openai",
                "group_name": "GPT 稳定分组",
                "metrics": { "success_rate": 1.0, "ttft": {} },
                "health": { "overall": "healthy" }
            }] }
        }));
        apply_group_rates(&mut channels, &rates);
        assert_eq!(channels[0].model_ratio, Some(0.06));
    }

    #[test]
    fn group_rates_match_stale_monitor_labels_without_guessing_ambiguous_groups() {
        let rates = vec![
            GroupRate {
                name: "Codex｜pro稳定分组".into(),
                rate: 0.20,
            },
            GroupRate {
                name: "grok|gork free分组（支持grok4.6,上下文500k ）".into(),
                rate: 0.05,
            },
        ];
        let mut channels = vec![
            parse_channel(&serde_json::json!({
                "group_name": "Codex｜pro稳定分组 (无限制无视封号警告可随意破限)",
                "primary_model": "gpt-5.6-sol"
            })),
            parse_channel(&serde_json::json!({
                "group_name": "grok|gork分组（支持grok4.5,上下文500k ）",
                "primary_model": "grok-4.5"
            })),
        ];
        apply_group_rates(&mut channels, &rates);
        assert_eq!(channels[0].model_ratio, Some(0.20));
        assert_eq!(channels[1].model_ratio, Some(0.05));
    }

    #[test]
    fn group_rates_match_renamed_suffix_groups_by_tokens() {
        // 5yuantoken 实测：站点在分组名后追加了倍率/备注，监控侧保留旧短名
        let rates = vec![
            GroupRate {
                name: "【ChatGPT Pro】特惠 0.18".into(),
                rate: 0.18,
            },
            GroupRate {
                name: "【ChatGPT Pro】兜底 0.25  官方正价号".into(),
                rate: 0.25,
            },
            GroupRate {
                name: "【Claude Code】Kiro 0.15".into(),
                rate: 0.15,
            },
            GroupRate {
                name: "【Claude Code】Kiro 按次 0.025".into(),
                rate: 1.0,
            },
            GroupRate {
                name: "【Grok heavy】 0.15".into(),
                rate: 0.15,
            },
            GroupRate {
                name: "【Grok free】 0.1 ".into(),
                rate: 0.1,
            },
            GroupRate {
                name: "【ChatGPT Plus】特惠 0.1".into(),
                rate: 0.1,
            },
        ];
        let mut channels = vec![
            parse_channel(&serde_json::json!({
                "group_name": "【ChatGPT Pro】特惠",
                "primary_model": "gpt-5.6-terra"
            })),
            parse_channel(&serde_json::json!({
                "group_name": "【ChatGPT Pro】兜底",
                "primary_model": "gpt-5.6-terra"
            })),
            parse_channel(&serde_json::json!({
                "group_name": "【Claude Code】Kiro",
                "primary_model": "claude-sonnet-5"
            })),
            parse_channel(&serde_json::json!({
                "group_name": "【Grok 】0.15",
                "primary_model": "grok-4.5"
            })),
            parse_channel(&serde_json::json!({
                "group_name": "【Grok 】0.1",
                "primary_model": "grok-4.5"
            })),
            // 同名不同族：仅凭「特惠」二字无法区分 Pro/Plus，宁可缺失
            parse_channel(&serde_json::json!({
                "group_name": "【ChatGPT】特惠",
                "primary_model": "gpt-5.6-terra"
            })),
        ];
        apply_group_rates(&mut channels, &rates);
        assert_eq!(channels[0].model_ratio, Some(0.18));
        assert_eq!(channels[1].model_ratio, Some(0.25));
        // 「Kiro」同时命中 Kiro 0.15 与 Kiro 按次：多出的词元更少者唯一（0.15）才采用
        assert_eq!(channels[2].model_ratio, Some(0.15));
        assert_eq!(channels[3].model_ratio, Some(0.15));
        assert_eq!(channels[4].model_ratio, Some(0.1));
        assert_eq!(channels[5].model_ratio, None);
    }

    #[test]
    fn group_rates_match_by_bracket_prefix_when_words_diverge() {
        // 5yuan token：分组描述整段改写，只剩【】里的主名可对上
        let rates = vec![
            GroupRate {
                name: "【DeepSeek 稳定】国外版  3.5折".into(),
                rate: 0.35,
            },
            GroupRate {
                name: "【Claude Code】推荐 1倍率  不限客户端".into(),
                rate: 1.0,
            },
            GroupRate {
                name: "【Claude Code】推荐 1倍率  仅Claude客户端".into(),
                rate: 1.0,
            },
        ];
        let mut channels = vec![
            parse_channel(&serde_json::json!({
                "group_name": "【DeepSeek 稳定】官方池",
                "primary_model": "deepseek-v4-flash"
            })),
            // 同前缀两个分组：歧义时不猜
            parse_channel(&serde_json::json!({
                "group_name": "【Claude Code】Max",
                "primary_model": "claude-opus-4-6"
            })),
        ];
        apply_group_rates(&mut channels, &rates);
        assert_eq!(channels[0].model_ratio, Some(0.35));
        assert_eq!(channels[1].model_ratio, None);
    }

    #[test]
    fn group_rates_fall_back_to_monitor_label_and_overlap() {
        // 5yuantoken 实测：两个监控共用同一个 group_name，真身分组只能靠监控标签区分；
        // Max 与 Plus 的分组已被站点删除，标签里留着建监控时标注的倍率
        let rates = vec![
            GroupRate {
                name: "【deepseek-v4】2折 自建池".into(),
                rate: 0.2,
            },
            GroupRate {
                name: "【deepseek-v4 】2.5折   友商".into(),
                rate: 0.25,
            },
            GroupRate {
                name: "【DeepSeek 稳定】国外版  3.5折".into(),
                rate: 0.35,
            },
            GroupRate {
                name: "【Claude Code】推荐 1倍率  不限客户端".into(),
                rate: 1.0,
            },
            GroupRate {
                name: "【Claude Code】推荐 1倍率  仅Claude客户端".into(),
                rate: 1.0,
            },
            GroupRate {
                name: "【Claude Code】Kiro 0.15".into(),
                rate: 0.15,
            },
            GroupRate {
                name: "【Claude Code】Kiro 按次 0.025".into(),
                rate: 1.0,
            },
            GroupRate {
                name: "【ChatGPT Plus】特惠 0.1".into(),
                rate: 0.1,
            },
        ];
        let mut channels = vec![
            // 同一 group_name 的两个监控：标签里的「自建池」/「友商」分别对上不同分组
            parse_channel(&serde_json::json!({
                "group_name": "【DeepSeek/GLM】友商平价",
                "name": "DeepSeek自建池｜x0.2",
                "primary_model": "deepseek-v4-flash"
            })),
            parse_channel(&serde_json::json!({
                "group_name": "【DeepSeek/GLM】友商平价",
                "name": "DeepSeek/GLM｜友商平价｜x0.25 (Copy) (Copy)",
                "primary_model": "deepseek-v4-flash"
            })),
            // 分组已删除：显示名与标签词元重叠全部平局，最后按标签标注的倍率显示
            parse_channel(&serde_json::json!({
                "group_name": "【Claude Code】Max",
                "name": "Claude Max｜x1.00",
                "primary_model": "claude-opus-4-6"
            })),
            parse_channel(&serde_json::json!({
                "group_name": "监控分组",
                "name": "GPT Plus 0.08",
                "primary_model": "gpt-5.6-terra"
            })),
        ];
        apply_group_rates(&mut channels, &rates);
        assert_eq!(channels[0].model_ratio, Some(0.2));
        assert_eq!(channels[1].model_ratio, Some(0.25));
        assert_eq!(channels[2].model_ratio, Some(1.0));
        assert_eq!(channels[3].model_ratio, Some(0.08));
    }

    #[test]
    fn label_rate_parses_common_notations() {
        assert_eq!(label_rate("Claude Max｜x1.00"), Some(1.0));
        assert_eq!(
            label_rate("DeepSeek/GLM｜友商平价｜x0.25 (Copy)"),
            Some(0.25)
        );
        assert_eq!(label_rate("推荐 ×0.1"), Some(0.1));
        assert_eq!(label_rate("GPT Plus 0.08"), Some(0.08));
        assert_eq!(label_rate("claude-sonnet-4-5"), None);
        assert_eq!(label_rate("gpt-5.6-terra"), None);
        assert_eq!(label_rate("grok-4.5 上下文500k"), None);
        assert_eq!(label_rate("qwen3.8 27b"), None);
    }

    #[test]
    fn group_rate_name_tokens_keep_cjk_adjacency() {
        assert_eq!(
            name_tokens("【ChatGPT Pro】特惠 0.18"),
            vec!["chatgpt", "pro", "特惠", "0.18"]
        );
        assert_eq!(name_tokens("【Grok 】0.1"), vec!["grok", "0.1"]);
        assert_eq!(
            name_tokens("【Claude Code】Kiro 按次 0.025"),
            vec!["claude", "code", "kiro", "按次", "0.025"]
        );
        assert_eq!(
            bracket_content("【DeepSeek 稳定】官方池").as_deref(),
            Some("deepseek稳定")
        );
        assert_eq!(bracket_content("监控分组"), None);
    }
}

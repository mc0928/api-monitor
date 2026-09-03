/// 统一监控的模型族：GPT / Claude / Grok / Kimi / Gemini / Qwen / DeepSeek
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Gpt,
    Claude,
    Grok,
    Kimi,
    Gemini,
    Qwen,
    Deepseek,
}

impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Gpt => "gpt",
            Self::Claude => "claude",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
            Self::Gemini => "gemini",
            Self::Qwen => "qwen",
            Self::Deepseek => "deepseek",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().as_str() {
            "gpt" | "openai" | "chatgpt" => Some(Self::Gpt),
            "claude" | "anthropic" => Some(Self::Claude),
            "grok" | "xai" => Some(Self::Grok),
            "kimi" | "moonshot" => Some(Self::Kimi),
            "gemini" | "google" => Some(Self::Gemini),
            "qwen" | "alibaba" | "dashscope" => Some(Self::Qwen),
            "deepseek" => Some(Self::Deepseek),
            _ => None,
        }
    }
}

pub fn normalize_model(s: &str) -> String {
    let lower = s.trim().to_ascii_lowercase();
    let stripped = lower.rsplit(['/', ':']).next().unwrap_or(lower.as_str());
    stripped
        .chars()
        .map(|c| if c == '.' || c == '_' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// a 是否为 b 的版本前缀：相等，或 b 在 a 之后紧跟 '-'（归一化后均为 ASCII）
fn prefix_token(short: &str, long: &str) -> bool {
    long.strip_prefix(short)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
}

pub fn models_match(a: &str, b: &str) -> bool {
    let a = normalize_model(a);
    let b = normalize_model(b);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || prefix_token(&a, &b) || prefix_token(&b, &a)
}

pub fn detect_provider(text: &str) -> Option<Provider> {
    let raw = text.to_ascii_lowercase();
    let n = normalize_model(text);
    if is_non_chat_model(&n) {
        return None;
    }
    if n.contains("claude")
        || n.contains("sonnet")
        || n.contains("opus")
        || n.contains("haiku")
        || raw.contains("anthropic")
    {
        return Some(Provider::Claude);
    }
    if n.contains("grok") || raw.contains("xai") {
        return Some(Provider::Grok);
    }
    if n.contains("kimi") || n.contains("moonshot") {
        return Some(Provider::Kimi);
    }
    if n.contains("gemini") || raw.contains("google") {
        return Some(Provider::Gemini);
    }
    if n.contains("qwen") || raw.contains("dashscope") || raw.contains("alibaba") {
        return Some(Provider::Qwen);
    }
    if n.contains("deepseek") {
        return Some(Provider::Deepseek);
    }
    if n.contains("gpt")
        || n.contains("chatgpt")
        || n.contains("openai")
        || n.starts_with("o1")
        || n.starts_with("o3")
        || n.starts_with("o4")
    {
        return Some(Provider::Gpt);
    }
    None
}

/// 排除当前未单独展示的图片模型族；gpt-image、Qwen 向量模型和 DeepSeek 会正常归类。
pub fn is_non_chat_model(text: &str) -> bool {
    let n = normalize_model(text);
    ["dall-e", "dalle", "imagen", "nano-banana"]
        .iter()
        .any(|marker| n.contains(marker))
}

pub fn as_percent(value: f64) -> f64 {
    if value < 0.0 {
        0.0
    } else if value <= 1.0 {
        value * 100.0
    } else {
        value.min(100.0)
    }
}

/// Unix 秒转 UTC ISO 时间，避免为简单时间标签引入额外日期依赖。
pub fn unix_to_iso(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// 渠道行右侧小字：`gpt-5.6-sol · 5783ms · 100.0%`
pub fn format_channel_detail(
    model: &str,
    latency_ms: Option<i64>,
    availability: Option<f64>,
) -> String {
    let mut parts = Vec::new();
    if !model.is_empty() {
        parts.push(model.to_string());
    }
    if let Some(ms) = latency_ms {
        if ms > 0 {
            parts.push(format!("{ms}ms"));
        }
    }
    if let Some(avail) = availability {
        parts.push(format!("{:.1}%", as_percent(avail)));
    }
    parts.join(" · ")
}

pub fn sort_by_success_rate<T, F>(items: &mut [T], availability: F)
where
    F: Fn(&T) -> Option<f64>,
{
    items.sort_by(|a, b| {
        let av = |item: &T| availability(item).map(as_percent).unwrap_or(-1.0);
        av(b)
            .partial_cmp(&av(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_vendor_and_dots() {
        assert_eq!(
            normalize_model("anthropic/claude-sonnet-4.6"),
            "claude-sonnet-4-6"
        );
        assert_eq!(normalize_model("gpt-5.4"), "gpt-5-4");
    }

    #[test]
    fn match_allows_version_prefix() {
        assert!(models_match("claude-sonnet-4-6", "claude-sonnet-4.6"));
        assert!(models_match("claude-sonnet-4", "claude-sonnet-4-6"));
        assert!(!models_match("claude-sonnet-4", "claude-opus-4-6"));
    }

    #[test]
    fn detect_families() {
        assert_eq!(detect_provider("claude-sonnet-4-6"), Some(Provider::Claude));
        assert_eq!(detect_provider("gpt-5.4-mini"), Some(Provider::Gpt));
        assert_eq!(detect_provider("grok-4.6"), Some(Provider::Grok));
        assert_eq!(detect_provider("kimi-k2.5"), Some(Provider::Kimi));
        assert_eq!(detect_provider("anthropic"), Some(Provider::Claude));
        assert_eq!(detect_provider("gemini-2.5-pro"), Some(Provider::Gemini));
        assert_eq!(
            detect_provider("Qwen/Qwen3-Embedding-0.6B"),
            Some(Provider::Qwen)
        );
        assert_eq!(detect_provider("deepseek-chat"), Some(Provider::Deepseek));
        assert_eq!(detect_provider("google"), Some(Provider::Gemini));
        assert_eq!(Provider::from_id("google"), Some(Provider::Gemini));
    }

    #[test]
    fn image_and_embedding_models_use_their_configured_families() {
        assert_eq!(detect_provider("gpt-image-2"), Some(Provider::Gpt));
        assert_eq!(detect_provider("deepseek-v3"), Some(Provider::Deepseek));
        assert_eq!(
            detect_provider("Qwen3-Embedding-0.6B"),
            Some(Provider::Qwen)
        );
    }

    #[test]
    fn channel_detail_omits_day_label() {
        let line = format_channel_detail("gpt-5.6-sol", Some(5783), Some(100.0));
        assert_eq!(line, "gpt-5.6-sol · 5783ms · 100.0%");
    }

    #[test]
    fn percent_accepts_ratio_and_absolute() {
        assert!((as_percent(0.365) - 36.5).abs() < f64::EPSILON);
        assert!((as_percent(36.5) - 36.5).abs() < f64::EPSILON);
        assert!((as_percent(1.0) - 100.0).abs() < f64::EPSILON);
    }
}

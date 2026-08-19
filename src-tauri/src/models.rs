/// 统一监控的模型族：GPT / Claude / Grok / Kimi
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Gpt,
    Claude,
    Grok,
    Kimi,
}

impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Gpt => "gpt",
            Self::Claude => "claude",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().as_str() {
            "gpt" | "openai" | "chatgpt" => Some(Self::Gpt),
            "claude" | "anthropic" => Some(Self::Claude),
            "grok" | "xai" => Some(Self::Grok),
            "kimi" | "moonshot" => Some(Self::Kimi),
            _ => None,
        }
    }
}

pub fn normalize_model(s: &str) -> String {
    let lower = s.trim().to_ascii_lowercase();
    let stripped = lower
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(lower.as_str());
    stripped
        .chars()
        .map(|c| if c == '.' || c == '_' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

fn prefix_token(short: &str, long: &str) -> bool {
    if short.is_empty() || !long.starts_with(short) {
        return false;
    }
    long.len() == short.len() || long.as_bytes().get(short.len()) == Some(&b'-')
}

pub fn models_match(a: &str, b: &str) -> bool {
    let a = normalize_model(a);
    let b = normalize_model(b);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || prefix_token(&a, &b) || prefix_token(&b, &a)
}

pub fn best_match(available: &[String], wanted: &str) -> Option<String> {
    let want_n = normalize_model(wanted);
    if want_n.is_empty() {
        return None;
    }
    if let Some(exact) = available
        .iter()
        .find(|item| normalize_model(item) == want_n)
    {
        return Some(exact.clone());
    }
    let mut cands: Vec<&String> = available
        .iter()
        .filter(|item| models_match(item, wanted))
        .collect();
    if cands.is_empty() {
        return None;
    }
    cands.sort_by_key(|item| {
        let n = normalize_model(item);
        (n.len().abs_diff(want_n.len()), n.len())
    });
    Some(cands[0].clone())
}

pub fn detect_provider(text: &str) -> Option<Provider> {
    let raw = text.to_ascii_lowercase();
    let n = normalize_model(text);
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

pub fn as_percent(value: f64) -> f64 {
    if value < 0.0 {
        0.0
    } else if value <= 1.0 {
        value * 100.0
    } else {
        value.min(100.0)
    }
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
    fn best_match_prefers_exact() {
        let available = vec![
            "claude-sonnet-4-6".into(),
            "claude-sonnet-4-6-thinking".into(),
            "claude-sonnet-4".into(),
        ];
        assert_eq!(
            best_match(&available, "claude-sonnet-4-6").as_deref(),
            Some("claude-sonnet-4-6")
        );
    }

    #[test]
    fn detect_families() {
        assert_eq!(detect_provider("claude-sonnet-4-6"), Some(Provider::Claude));
        assert_eq!(detect_provider("gpt-5.4-mini"), Some(Provider::Gpt));
        assert_eq!(detect_provider("grok-4.6"), Some(Provider::Grok));
        assert_eq!(detect_provider("kimi-k2.5"), Some(Provider::Kimi));
        assert_eq!(detect_provider("anthropic"), Some(Provider::Claude));
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

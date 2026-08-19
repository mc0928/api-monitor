use std::time::Duration;

/// 出站请求超时：连接 5s，整体 12s
pub const TIMEOUT: Duration = Duration::from_secs(12);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// 按字符数截断字符串（避免中文截断在字符中间）
pub fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// 构建 reqwest 客户端：传入代理地址则挂代理，否则直连
pub fn build_client(proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_max_idle_per_host(8)
        .tcp_nodelay(true)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) api-monitor/0.1");
    if let Some(url) = proxy_url.map(str::trim).filter(|u| !u.is_empty()) {
        let proxy = reqwest::Proxy::all(url)
            .map_err(|e| format!("代理地址无效（{url}）: {e}"))?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

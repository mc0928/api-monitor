use std::time::Duration;

use reqwest::Client;

/// 出站请求超时：连接 5s，整体 12s
pub const TIMEOUT: Duration = Duration::from_secs(12);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// 按字符数截断字符串（避免中文截断在字符中间）
pub fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// 构建 reqwest 客户端：传入代理地址则挂代理，否则直连
pub fn build_client(proxy_url: Option<&str>) -> Result<Client, String> {
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

/// new2api / sub2api 共用的带鉴权 GET：可选 Bearer 令牌与 New-Api-User 头，
/// 传输层错误（超时/断连）按 retries 重试，HTTP 状态码错误不重试。
pub async fn authorized_get(
    client: &Client,
    url: &str,
    token: Option<&str>,
    user_id: Option<&str>,
    timeout: Option<Duration>,
    retries: u32,
) -> Result<(u16, String), String> {
    let token = token.map(str::trim).filter(|t| !t.is_empty());
    let user_id = user_id.map(str::trim).filter(|u| !u.is_empty());
    let mut last_err = String::new();
    for attempt in 0..=retries {
        let mut req = client.get(url);
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        if let Some(id) = user_id {
            req = req.header("New-Api-User", id);
        }
        if let Some(timeout) = timeout {
            req = req.timeout(timeout);
        }
        match req.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                return Ok((status, body));
            }
            Err(e) => {
                last_err = describe_request_error(&e);
                if attempt < retries {
                    continue;
                }
            }
        }
    }
    Err(last_err)
}

fn describe_request_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        return "连接超时，请稍后重试".to_string();
    }
    if err.is_connect() {
        return "无法连接到站点（网络或证书问题）".to_string();
    }
    let msg = err.to_string().to_ascii_lowercase();
    if msg.contains("dns") || msg.contains("resolve") {
        return "域名解析失败".to_string();
    }
    "无法访问站点".to_string()
}

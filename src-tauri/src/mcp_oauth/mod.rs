// 远程 MCP 服务器 OAuth (RFC 8414 发现 + RFC 7591 DCR + PKCE 授权码)
// 用于在本应用内为 url 型 MCP 服务器（如 Notion）完成授权，
// token 存入 ~/.kirohub/mcp-oauth.json，由本地反代注入 Bearer。

use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::utils::browser::open_browser_keep_session;

/// 授权流程产出：换到 token + 端点信息，供命令层落盘
pub struct AuthorizeOutcome {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
    pub auth_endpoint: String,
    pub token_endpoint: String,
    pub resource: String,
}

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
}

#[derive(Deserialize)]
struct MetadataResp {
    authorization_endpoint: Option<String>,
    token_endpoint: Option<String>,
    registration_endpoint: Option<String>,
}

#[derive(Deserialize)]
struct RegisterResp {
    client_id: String,
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

fn base64_url(data: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.encode(data)
}

/// 生成 PKCE (code_verifier, code_challenge)
fn gen_pkce() -> (String, String) {
    use rand::Rng;
    use sha2::{Digest, Sha256};
    let bytes: Vec<u8> = (0..32).map(|_| rand::thread_rng().gen()).collect();
    let verifier = base64_url(&bytes);
    let challenge = base64_url(&Sha256::new().chain_update(verifier.as_bytes()).finalize());
    (verifier, challenge)
}

/// 从 base_url 推导默认端点（发现失败时的兜底）
fn fallback_endpoints(base_url: &str) -> Endpoints {
    let origin = origin_of(base_url);
    Endpoints {
        authorization_endpoint: format!("{origin}/authorize"),
        token_endpoint: format!("{origin}/token"),
        registration_endpoint: Some(format!("{origin}/register")),
    }
}

/// 取 URL 的 scheme://host[:port]
fn origin_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            let scheme = u.scheme().to_string();
            u.host_str().map(|h| match u.port() {
                Some(p) => format!("{scheme}://{h}:{p}"),
                None => format!("{scheme}://{h}"),
            })
        })
        .unwrap_or_else(|| url.trim_end_matches('/').to_string())
}

/// RFC 8414 元数据发现：尝试 well-known，失败回退推导端点
pub async fn discover_endpoints(base_url: &str) -> Endpoints {
    let origin = origin_of(base_url);
    let client = reqwest::Client::new();
    for path in [
        "/.well-known/oauth-authorization-server",
        "/.well-known/openid-configuration",
    ] {
        let url = format!("{origin}{path}");
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(meta) = resp.json::<MetadataResp>().await {
                    if let (Some(auth), Some(token)) =
                        (meta.authorization_endpoint, meta.token_endpoint)
                    {
                        return Endpoints {
                            authorization_endpoint: auth,
                            token_endpoint: token,
                            registration_endpoint: meta.registration_endpoint,
                        };
                    }
                }
            }
        }
    }
    fallback_endpoints(base_url)
}

/// RFC 7591 动态客户端注册，返回 client_id
pub async fn register_client(
    registration_endpoint: &str,
    redirect_uri: &str,
) -> Result<String, String> {
    let body = serde_json::json!({
        "client_name": "KiroHub MCP",
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none"
    });
    let resp = reqwest::Client::new()
        .post(registration_endpoint)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("DCR 请求失败: {e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 DCR 响应失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("DCR 失败 ({status}): {text}"));
    }
    serde_json::from_str::<RegisterResp>(&text)
        .map(|r| r.client_id)
        .map_err(|e| format!("解析 DCR 响应失败: {e}"))
}

fn build_authorize_url(
    authorization_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
    resource: &str,
) -> String {
    let sep = if authorization_endpoint.contains('?') { '&' } else { '?' };
    format!(
        "{authorization_endpoint}{sep}response_type=code&client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256&resource={}",
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(state),
        urlencoding::encode(code_challenge),
        urlencoding::encode(resource),
    )
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn expires_at_from(expires_in: Option<i64>) -> i64 {
    match expires_in {
        Some(s) if s > 0 => now_secs() + s,
        _ => 0,
    }
}

/// 授权码换 token
async fn exchange_code(
    token_endpoint: &str,
    client_id: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    resource: &str,
) -> Result<TokenResp, String> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
        ("resource", resource),
    ];
    post_token(token_endpoint, &params).await
}

/// refresh_token 刷新；调用方负责处理 refresh_token 轮换（缺省则沿用旧值）
pub async fn refresh_access_token(
    token_endpoint: &str,
    client_id: &str,
    refresh_token: &str,
    resource: &str,
) -> Result<(String, Option<String>, i64), String> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
        ("resource", resource),
    ];
    let t = post_token(token_endpoint, &params).await?;
    Ok((t.access_token, t.refresh_token, expires_at_from(t.expires_in)))
}

async fn post_token(token_endpoint: &str, params: &[(&str, &str)]) -> Result<TokenResp, String> {
    let resp = reqwest::Client::new()
        .post(token_endpoint)
        .form(params)
        .send()
        .await
        .map_err(|e| format!("token 请求失败: {e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 token 响应失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("token 换取失败 ({status}): {text}"));
    }
    serde_json::from_str::<TokenResp>(&text).map_err(|e| format!("解析 token 响应失败: {e}"))
}

// ===== 本地回调服务器（PKCE 授权码交互）=====

fn parse_callback(url: &str, expected_state: &str) -> Result<String, String> {
    let query = url.split('?').nth(1).unwrap_or("");
    let params: std::collections::HashMap<_, _> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    if params.get("state").map(String::as_str) != Some(expected_state) {
        return Err("state 不匹配".to_string());
    }
    if let Some(err) = params.get("error") {
        let desc = params.get("error_description").map_or("未知错误", |s| s);
        return Err(format!("{err}: {desc}"));
    }
    params
        .get("code")
        .cloned()
        .ok_or_else(|| "未收到授权码".to_string())
}

/// 启动本地服务器等待回调，返回授权码（阻塞，带 10 分钟超时）
fn wait_for_code(
    server: Arc<tiny_http::Server>,
    expected_state: &str,
    cancelled: &Arc<AtomicBool>,
) -> Result<String, String> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(600);
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err("授权已取消".to_string());
        }
        if start.elapsed() > timeout {
            return Err("授权超时".to_string());
        }
        if let Ok(Some(request)) = server.try_recv() {
            let url = request.url().to_string();
            if url.starts_with("/oauth/callback") {
                let result = parse_callback(&url, expected_state);
                let page = match &result {
                    Ok(_) => "<html><body><h1>授权成功</h1><p>可以关闭此窗口</p></body></html>"
                        .to_string(),
                    Err(m) => format!("<html><body><h1>授权失败</h1><p>{m}</p></body></html>"),
                };
                let mut response = tiny_http::Response::from_string(page);
                if let Ok(header) = tiny_http::Header::from_bytes(
                    &b"Content-Type"[..],
                    &b"text/html; charset=utf-8"[..],
                ) {
                    response = response.with_header(header);
                }
                let _ = request.respond(response);
                return result;
            }
            let _ = request.respond(tiny_http::Response::empty(404));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// 完整授权流程：发现 -> DCR(可复用 client_id) -> PKCE 授权 -> 换 token
pub async fn run_authorize(
    base_url: &str,
    existing_client_id: Option<String>,
) -> Result<AuthorizeOutcome, String> {
    let endpoints = discover_endpoints(base_url).await;
    let resource = base_url.to_string();

    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| format!("无法启动本地服务器: {e}"))?;
    let server = Arc::new(server);
    let port = server.server_addr().to_ip().map_or(0, |a| a.port());
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth/callback");

    let client_id = match existing_client_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            let reg_ep = endpoints
                .registration_endpoint
                .clone()
                .ok_or("服务器未提供注册端点，无法完成 DCR")?;
            register_client(&reg_ep, &redirect_uri).await?
        }
    };

    let state = uuid::Uuid::new_v4().to_string();
    let (verifier, challenge) = gen_pkce();
    let authorize_url = build_authorize_url(
        &endpoints.authorization_endpoint,
        &client_id,
        &redirect_uri,
        &state,
        &challenge,
        &resource,
    );

    open_browser_keep_session(&authorize_url)?;

    let cancelled = Arc::new(AtomicBool::new(false));
    let code = {
        let state = state.clone();
        let cancelled = cancelled.clone();
        tokio::task::spawn_blocking(move || wait_for_code(server, &state, &cancelled))
            .await
            .map_err(|e| format!("授权任务异常: {e}"))??
    };

    let token = exchange_code(
        &endpoints.token_endpoint,
        &client_id,
        &code,
        &verifier,
        &redirect_uri,
        &resource,
    )
    .await?;

    Ok(AuthorizeOutcome {
        client_id,
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: expires_at_from(token.expires_in),
        auth_endpoint: endpoints.authorization_endpoint,
        token_endpoint: endpoints.token_endpoint,
        resource,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_strips_path() {
        assert_eq!(origin_of("https://mcp.notion.com/mcp"), "https://mcp.notion.com");
        assert_eq!(origin_of("https://h.io:8443/a/b"), "https://h.io:8443");
    }

    #[test]
    fn pkce_challenge_is_deterministic_per_verifier() {
        let (v, c) = gen_pkce();
        use sha2::{Digest, Sha256};
        let expect = base64_url(&Sha256::new().chain_update(v.as_bytes()).finalize());
        assert_eq!(c, expect);
    }

    #[test]
    fn authorize_url_picks_correct_separator() {
        let u = build_authorize_url("https://a/auth", "cid", "http://127.0.0.1/cb", "st", "ch", "https://r");
        assert!(u.starts_with("https://a/auth?response_type=code"));
        let u2 = build_authorize_url("https://a/auth?x=1", "cid", "http://127.0.0.1/cb", "st", "ch", "https://r");
        assert!(u2.contains("auth?x=1&response_type=code"));
    }

    #[test]
    fn parse_callback_validates_state_and_code() {
        assert_eq!(
            parse_callback("/oauth/callback?code=abc&state=s1", "s1").unwrap(),
            "abc"
        );
        assert!(parse_callback("/oauth/callback?code=abc&state=s2", "s1").is_err());
        assert!(parse_callback("/oauth/callback?error=denied&state=s1", "s1").is_err());
    }

    #[test]
    fn expires_at_handles_missing() {
        assert_eq!(expires_at_from(None), 0);
        assert_eq!(expires_at_from(Some(0)), 0);
        assert!(expires_at_from(Some(3600)) > now_secs());
    }
}

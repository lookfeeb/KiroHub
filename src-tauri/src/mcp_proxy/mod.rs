// 本地 MCP 反向代理
// mcp.json 的 url 无法携带认证头，故让其指向 127.0.0.1:<port>/<secret>/<credentialKey>/<serverName>，
// 本服务注入自动刷新的 Authorization: Bearer 后转发到真实上游 MCP 地址，并流式回传。
//
// 安全：仅绑定 127.0.0.1；路径中的 <secret> 做本地校验，防止本机其他进程盗用 token。

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::Response,
    routing::any,
    Router,
};

use crate::commands::app_settings_cmd::{
    decode_credential_key, get_mcp_oauth_store, get_or_init_proxy_runtime, upsert_mcp_oauth_cred,
    McpOAuthCred,
};
use crate::mcp_oauth::refresh_access_token;

#[derive(Clone)]
struct ProxyState {
    secret: String,
    client: reqwest::Client,
}

/// 启动本地反代，返回监听端口（已持久化，跨重启稳定）
pub async fn start_proxy() -> Result<u16, String> {
    let (port, secret) = get_or_init_proxy_runtime()?;
    let state = ProxyState {
        secret,
        client: reqwest::Client::new(),
    };

    let app = Router::new()
        .route("/{secret}/{credential_key}/{server_name}", any(handle))
        .route("/{secret}/{credential_key}/{server_name}/{*rest}", any(handle))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("反代绑定 {addr} 失败: {e}"))?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("MCP 反代退出: {e}");
        }
    });
    log::info!("MCP 反代已启动: http://127.0.0.1:{port}");
    Ok(port)
}

async fn handle(
    State(st): State<ProxyState>,
    Path(params): Path<Vec<(String, String)>>,
    method: Method,
    headers: HeaderMap,
    body: Body,
) -> Response {
    // params 顺序与路由占位一致：secret, credential_key, server_name, [rest]
    let secret = params.first().map(|(_, v)| v.clone()).unwrap_or_default();
    let credential_key = params
        .get(1)
        .map(|(_, v)| decode_credential_key(v))
        .unwrap_or_default();
    let server_name = params.get(2).map(|(_, v)| v.clone()).unwrap_or_default();
    let rest = params.get(3).map(|(_, v)| v.clone());

    if secret != st.secret {
        return text_resp(StatusCode::FORBIDDEN, "invalid proxy secret");
    }

    let Ok(store) = get_mcp_oauth_store() else {
        return text_resp(StatusCode::INTERNAL_SERVER_ERROR, "read store failed");
    };
    let Some(cred) = store.creds_by_key.get(&credential_key).cloned() else {
        return text_resp(StatusCode::NOT_FOUND, "unknown credential key");
    };

    // 拼接上游 URL：mcp_endpoint + 子路径
    let target = match &rest {
        Some(r) => format!("{}/{}", cred.mcp_endpoint.trim_end_matches('/'), r),
        None => cred.mcp_endpoint.clone(),
    };

    // 读取一次 body 字节（便于 401 重试时复用）
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => return text_resp(StatusCode::BAD_REQUEST, &format!("read body failed: {e}")),
    };

    let mut access_token = cred.access_token.clone();
    let mut resp = forward(&st.client, &method, &target, &headers, &body_bytes, &access_token).await;

    // 上游 401：尝试刷新一次后重试
    if let Ok(r) = &resp {
        if r.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(new_token) = try_refresh(&credential_key, &server_name, &cred).await {
                access_token = new_token;
                resp = forward(&st.client, &method, &target, &headers, &body_bytes, &access_token)
                    .await;
            }
        }
    }

    match resp {
        Ok(upstream) => stream_back(upstream),
        Err(e) => text_resp(StatusCode::BAD_GATEWAY, &format!("upstream error: {e}")),
    }
}

/// 转发请求到上游，注入 Bearer，透传方法/头/体
async fn forward(
    client: &reqwest::Client,
    method: &Method,
    target: &str,
    headers: &HeaderMap,
    body: &[u8],
    access_token: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut req = client.request(method.clone(), target);
    for (name, value) in headers {
        let n = name.as_str().to_ascii_lowercase();
        // 这些头由 reqwest/我们重置，不透传
        if matches!(n.as_str(), "host" | "authorization" | "content-length") {
            continue;
        }
        req = req.header(name, value);
    }
    req = req.header("Authorization", format!("Bearer {access_token}"));
    if !body.is_empty() {
        req = req.body(body.to_vec());
    }
    req.send().await
}

/// 刷新 token 并持久化（处理 refresh_token 轮换），返回新 access_token
async fn try_refresh(credential_key: &str, server_name: &str, cred: &McpOAuthCred) -> Option<String> {
    let refresh = cred.refresh_token.clone()?;
    match refresh_access_token(&cred.token_endpoint, &cred.client_id, &refresh, &cred.resource).await
    {
        Ok((access, new_refresh, expires_at)) => {
            let updated = McpOAuthCred {
                access_token: access.clone(),
                refresh_token: new_refresh.or(cred.refresh_token.clone()),
                expires_at,
                ..cred.clone()
            };
            if let Err(e) = upsert_mcp_oauth_cred(credential_key, updated) {
                log::error!("保存刷新后的 MCP 凭证失败: {e}");
            }
            Some(access)
        }
        Err(e) => {
            log::error!("MCP token 刷新失败 ({credential_key}/{server_name}): {e}");
            None
        }
    }
}

/// 将上游响应（含 SSE 流）转回客户端
fn stream_back(upstream: reqwest::Response) -> Response {
    let status = StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::OK);
    let mut builder = Response::builder().status(status);
    for (name, value) in upstream.headers() {
        let n = name.as_str().to_ascii_lowercase();
        if matches!(n.as_str(), "transfer-encoding" | "content-length" | "connection") {
            continue;
        }
        builder = builder.header(name, value);
    }
    let stream = upstream.bytes_stream();
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| text_resp(StatusCode::INTERNAL_SERVER_ERROR, "build response failed"))
}

fn text_resp(status: StatusCode, msg: &str) -> Response {
    let mut resp = Response::new(Body::from(msg.to_string()));
    *resp.status_mut() = status;
    resp.headers_mut().insert(
        "Content-Type",
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp
}

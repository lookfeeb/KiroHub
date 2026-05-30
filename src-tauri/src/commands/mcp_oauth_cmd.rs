// 远程 MCP 服务器 OAuth 命令
// authorize: 发现+DCR+PKCE 授权 -> 存凭证 -> 把 mcp.json 的 url 改写为本地反代地址
// status:    查询授权状态
// revoke:    删凭证 -> 还原 mcp.json 的 url 为真实上游地址

#![allow(clippy::needless_pass_by_value)]

use serde::Serialize;

use crate::commands::app_settings_cmd::{
    get_mcp_oauth_store, get_or_init_proxy_runtime, remove_mcp_oauth_cred, upsert_mcp_oauth_cred,
    McpOAuthCred,
};
use crate::commands::common::run_blocking_task;
use crate::kiro::settings::mcp::{McpConfig, McpServer};
use crate::mcp_oauth::run_authorize;

/// 反代地址：http://127.0.0.1:<port>/<secret>/<serverKey>
fn proxy_url(port: u16, secret: &str, server_key: &str) -> String {
    format!("http://127.0.0.1:{port}/{secret}/{server_key}")
}

/// 读取某 url 型 server 的当前 url（用户级配置）
fn read_server_url(server_key: &str) -> Result<String, String> {
    let config = McpConfig::load()?;
    match config.mcp_servers.get(server_key) {
        Some(McpServer::Url(u)) => Ok(u.url.clone()),
        Some(McpServer::Command(_)) => Err("该服务器不是 url 型，不支持 OAuth".to_string()),
        None => Err(format!("MCP 配置中不存在服务器 {server_key}")),
    }
}

/// 改写某 url 型 server 的 url（用户级配置）
fn write_server_url(server_key: &str, new_url: &str) -> Result<(), String> {
    let mut config = McpConfig::load()?;
    match config.mcp_servers.get_mut(server_key) {
        Some(McpServer::Url(u)) => {
            u.url = new_url.to_string();
            config.save()
        }
        _ => Err(format!("MCP 配置中不存在 url 型服务器 {server_key}")),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthStatus {
    pub authorized: bool,
    pub expires_at: i64,
    pub expiring_soon: bool, // 10 分钟内过期
}

/// 发起授权，完成后凭证落盘并将 mcp.json url 指向本地反代
#[tauri::command]
pub async fn mcp_oauth_authorize(server_key: String) -> Result<(), String> {
    // 确保反代端口/密钥已分配
    let (port, secret) = get_or_init_proxy_runtime()?;
    let proxy = proxy_url(port, &secret, &server_key);

    // 当前 url：若已是反代地址，则真实上游取自现有凭证
    let current_url = read_server_url(&server_key)?;
    let store = get_mcp_oauth_store().unwrap_or_default();
    let existing = store.creds.get(&server_key);
    let mcp_endpoint = if current_url == proxy {
        existing
            .map(|c| c.mcp_endpoint.clone())
            .ok_or("已是反代地址但无凭证记录，请先在 mcp.json 中填回真实地址")?
    } else {
        current_url
    };
    let existing_client_id = existing.map(|c| c.client_id.clone());

    let outcome = run_authorize(&mcp_endpoint, existing_client_id).await?;

    upsert_mcp_oauth_cred(
        &server_key,
        McpOAuthCred {
            client_id: outcome.client_id,
            access_token: outcome.access_token,
            refresh_token: outcome.refresh_token,
            expires_at: outcome.expires_at,
            auth_endpoint: outcome.auth_endpoint,
            token_endpoint: outcome.token_endpoint,
            mcp_endpoint,
            resource: outcome.resource,
        },
    )?;

    // 改写 mcp.json，指向本地反代
    run_blocking_task(move || write_server_url(&server_key, &proxy)).await
}

#[tauri::command]
pub async fn mcp_oauth_status(server_key: String) -> Result<McpOAuthStatus, String> {
    let store = get_mcp_oauth_store().unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(match store.creds.get(&server_key) {
        Some(c) => McpOAuthStatus {
            authorized: true,
            expires_at: c.expires_at,
            expiring_soon: c.expires_at > 0 && c.expires_at - now < 600,
        },
        None => McpOAuthStatus {
            authorized: false,
            expires_at: 0,
            expiring_soon: false,
        },
    })
}

/// 撤销授权：删凭证并把 mcp.json url 还原为真实上游
#[tauri::command]
pub async fn mcp_oauth_revoke(server_key: String) -> Result<(), String> {
    let store = get_mcp_oauth_store().unwrap_or_default();
    let real_url = store.creds.get(&server_key).map(|c| c.mcp_endpoint.clone());
    remove_mcp_oauth_cred(&server_key)?;
    if let Some(url) = real_url {
        run_blocking_task(move || write_server_url(&server_key, &url)).await
    } else {
        Ok(())
    }
}

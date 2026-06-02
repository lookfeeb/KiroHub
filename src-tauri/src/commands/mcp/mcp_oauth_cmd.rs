// 远程 MCP 服务器 OAuth 命令
// 多客户端统一走共享 credentialKey，并把 URL 改写为本地反代地址。

#![allow(clippy::needless_pass_by_value)]

use serde::Serialize;

use crate::commands::app_settings_cmd::{
    bind_mcp_oauth_server, get_mcp_oauth_binding, get_mcp_oauth_store, get_or_init_proxy_runtime,
    normalized_credential_key, proxy_url_for_binding, set_mcp_oauth_refresh_failure,
    unbind_mcp_oauth_server, upsert_mcp_oauth_cred, McpOAuthCred,
};
use crate::commands::common::run_blocking_task;
use crate::commands::mcp_cmd::{
    read_mcp_server_url_for_client, write_mcp_server_url_for_client,
};
use crate::mcp_oauth::{refresh_access_token, run_authorize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthStatus {
    pub authorized: bool,
    pub expires_at: i64,
    pub expiring_soon: bool,
    pub expired: bool,
    pub refresh_failed: bool,
    pub needs_reauth: bool,
    pub credential_key: Option<String>,
    pub message: Option<String>,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn refresh_credential(credential_key: &str) -> Result<McpOAuthCred, String> {
    let store = get_mcp_oauth_store()?;
    let cred = store
        .creds_by_key
        .get(credential_key)
        .cloned()
        .ok_or("未找到共享 OAuth 凭据")?;
    let refresh = cred
        .refresh_token
        .clone()
        .ok_or("该凭据没有 refresh_token，需重新授权")?;

    match refresh_access_token(
        &cred.token_endpoint,
        &cred.client_id,
        &refresh,
        &cred.resource,
    )
    .await
    {
        Ok((access_token, new_refresh, expires_at)) => {
            let updated = McpOAuthCred {
                access_token,
                refresh_token: new_refresh.or(cred.refresh_token.clone()),
                expires_at,
                ..cred
            };
            upsert_mcp_oauth_cred(credential_key, updated.clone())?;
            Ok(updated)
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = set_mcp_oauth_refresh_failure(credential_key, msg.clone());
            Err(msg)
        }
    }
}

fn resolve_real_endpoint(client: &str, server_name: &str, current_url: String) -> Result<String, String> {
    if let Some(credential_key) = get_mcp_oauth_binding(client, server_name)? {
        let store = get_mcp_oauth_store()?;
        if let Some(cred) = store.creds_by_key.get(&credential_key) {
            return Ok(cred.mcp_endpoint.clone());
        }
    }
    Ok(current_url)
}

#[tauri::command]
pub async fn mcp_oauth_authorize_for_client(
    client: String,
    server_name: String,
) -> Result<(), String> {
    let current_url = run_blocking_task({
        let client = client.clone();
        let server_name = server_name.clone();
        move || read_mcp_server_url_for_client(&client, &server_name)
    })
    .await?;
    let mcp_endpoint = resolve_real_endpoint(&client, &server_name, current_url)?;
    let credential_key = normalized_credential_key(&mcp_endpoint);

    let store = get_mcp_oauth_store()?;
    if store.creds_by_key.contains_key(&credential_key) {
        bind_mcp_oauth_server(&client, &server_name, &credential_key)?;
    } else {
        let existing_client_id = store.creds_by_key.get(&credential_key).map(|c| c.client_id.clone());
        let outcome = run_authorize(&mcp_endpoint, existing_client_id).await?;
        upsert_mcp_oauth_cred(
            &credential_key,
            McpOAuthCred {
                client_id: outcome.client_id,
                access_token: outcome.access_token,
                refresh_token: outcome.refresh_token,
                expires_at: outcome.expires_at,
                auth_endpoint: outcome.auth_endpoint,
                token_endpoint: outcome.token_endpoint,
                mcp_endpoint: mcp_endpoint.clone(),
                resource: outcome.resource,
            },
        )?;
        bind_mcp_oauth_server(&client, &server_name, &credential_key)?;
    }

    let (port, secret) = get_or_init_proxy_runtime()?;
    let proxy = proxy_url_for_binding(port, &secret, &credential_key, &server_name);
    run_blocking_task(move || write_mcp_server_url_for_client(&client, &server_name, &proxy)).await
}

#[tauri::command]
pub async fn mcp_oauth_status_for_client(
    client: String,
    server_name: String,
) -> Result<McpOAuthStatus, String> {
    let store = get_mcp_oauth_store()?;
    let Some(credential_key) = store
        .server_bindings
        .get(&crate::commands::app_settings_cmd::binding_key(&client, &server_name))
        .cloned()
    else {
        return Ok(McpOAuthStatus {
            authorized: false,
            expires_at: 0,
            expiring_soon: false,
            expired: false,
            refresh_failed: false,
            needs_reauth: false,
            credential_key: None,
            message: None,
        });
    };

    let Some(cred) = store.creds_by_key.get(&credential_key) else {
        return Ok(McpOAuthStatus {
            authorized: false,
            expires_at: 0,
            expiring_soon: false,
            expired: false,
            refresh_failed: false,
            needs_reauth: true,
            credential_key: Some(credential_key),
            message: Some("绑定存在但凭据已丢失".to_string()),
        });
    };
    let now = now_secs();
    let expired = cred.expires_at > 0 && cred.expires_at <= now;
    let refresh_message = store.refresh_failures.get(&credential_key).cloned();
    let refresh_failed = refresh_message.is_some();
    Ok(McpOAuthStatus {
        authorized: true,
        expires_at: cred.expires_at,
        expiring_soon: cred.expires_at > 0 && cred.expires_at - now < 600 && !expired,
        expired,
        refresh_failed,
        needs_reauth: expired && refresh_failed,
        credential_key: Some(credential_key),
        message: refresh_message,
    })
}

#[tauri::command]
pub async fn mcp_oauth_refresh_for_client(
    client: String,
    server_name: String,
) -> Result<McpOAuthStatus, String> {
    let credential_key = get_mcp_oauth_binding(&client, &server_name)?
        .ok_or("该 MCP 服务器尚未绑定 OAuth 凭据")?;
    refresh_credential(&credential_key).await?;
    mcp_oauth_status_for_client(client, server_name).await
}

#[tauri::command]
pub async fn mcp_oauth_revoke_for_client(
    client: String,
    server_name: String,
) -> Result<(), String> {
    let unbound = unbind_mcp_oauth_server(&client, &server_name)?;
    if let Some((_, cred, _)) = unbound {
        run_blocking_task(move || {
            write_mcp_server_url_for_client(&client, &server_name, &cred.mcp_endpoint)
        })
        .await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn mcp_oauth_authorize(server_key: String) -> Result<(), String> {
    mcp_oauth_authorize_for_client("kiro".to_string(), server_key).await
}

#[tauri::command]
pub async fn mcp_oauth_status(server_key: String) -> Result<McpOAuthStatus, String> {
    mcp_oauth_status_for_client("kiro".to_string(), server_key).await
}

#[tauri::command]
pub async fn mcp_oauth_revoke(server_key: String) -> Result<(), String> {
    mcp_oauth_revoke_for_client("kiro".to_string(), server_key).await
}

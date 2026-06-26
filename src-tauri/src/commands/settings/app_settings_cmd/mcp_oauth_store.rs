use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::shared::data_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthCred {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
    pub auth_endpoint: String,
    pub token_endpoint: String,
    pub mcp_endpoint: String,
    pub resource: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthStore {
    #[serde(default)]
    pub creds: HashMap<String, McpOAuthCred>,
    #[serde(default)]
    pub creds_by_key: HashMap<String, McpOAuthCred>,
    #[serde(default)]
    pub server_bindings: HashMap<String, String>,
    #[serde(default)]
    pub refresh_failures: HashMap<String, String>,
    pub proxy_port: Option<u16>,
    #[serde(default)]
    pub proxy_secret: Option<String>,
}

fn mcp_oauth_path() -> PathBuf {
    data_dir().join("mcp-oauth.json")
}

pub fn get_mcp_oauth_store() -> Result<McpOAuthStore, String> {
    let mut store = if let Some(json) = crate::db::kv_get("mcp_oauth", "store")? {
        serde_json::from_str(&json).map_err(|e| format!("解析 MCP OAuth 失败: {e}"))?
    } else if let Some(store) = migrate_legacy_mcp_oauth()? {
        store
    } else {
        McpOAuthStore::default()
    };

    if normalize_mcp_oauth_store(&mut store) {
        save_mcp_oauth_store(&store)?;
    }

    Ok(store)
}

pub fn save_mcp_oauth_store(store: &McpOAuthStore) -> Result<(), String> {
    let content = serde_json::to_string(store).map_err(|e| format!("序列化失败: {e}"))?;
    crate::db::kv_set("mcp_oauth", "store", &content)
}

fn migrate_legacy_mcp_oauth() -> Result<Option<McpOAuthStore>, String> {
    let path = mcp_oauth_path();
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取旧 MCP OAuth 文件失败 ({}): {e}", path.display()))?;
    let store: McpOAuthStore = serde_json::from_str(&content)
        .map_err(|e| format!("解析旧 MCP OAuth 文件失败 ({}): {e}", path.display()))?;
    std::fs::rename(&path, path.with_extension("json.bak"))
        .map_err(|e| format!("备份旧 MCP OAuth 文件失败 ({}): {e}", path.display()))?;
    Ok(Some(store))
}

fn normalize_mcp_oauth_store(store: &mut McpOAuthStore) -> bool {
    if store.creds.is_empty() {
        return false;
    }

    for (server_key, cred) in std::mem::take(&mut store.creds) {
        let credential_key = normalized_credential_key(&cred.mcp_endpoint);
        store
            .creds_by_key
            .entry(credential_key.clone())
            .or_insert(cred);
        store
            .server_bindings
            .entry(binding_key("kiro", &server_key))
            .or_insert(credential_key);
    }

    true
}

pub fn normalized_credential_key(endpoint: &str) -> String {
    if let Ok(url) = url::Url::parse(endpoint) {
        if let Some(host) = url.host_str() {
            let scheme = url.scheme().to_ascii_lowercase();
            let host = host.to_ascii_lowercase();
            return match url.port() {
                Some(port) => format!("{scheme}://{host}:{port}"),
                None => format!("{scheme}://{host}"),
            };
        }
    }

    endpoint.trim_end_matches('/').to_ascii_lowercase()
}

pub fn mcp_oauth_failure_needs_reauth(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "invalid_grant",
        "grant not found",
        "refresh token expired",
        "invalid refresh token",
        "revoked",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

pub fn encode_credential_key(credential_key: &str) -> String {
    urlencoding::encode(credential_key).into_owned()
}

pub fn decode_credential_key(credential_key: &str) -> String {
    urlencoding::decode(credential_key)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| credential_key.to_string())
}

pub fn binding_key(client: &str, server_name: &str) -> String {
    format!("{}:{}", client.to_ascii_lowercase(), server_name)
}

pub fn proxy_url_for_binding(
    port: u16,
    secret: &str,
    credential_key: &str,
    server_name: &str,
) -> String {
    format!(
        "http://127.0.0.1:{port}/{secret}/{}/{}",
        encode_credential_key(credential_key),
        urlencoding::encode(server_name)
    )
}

pub fn upsert_mcp_oauth_cred(credential_key: &str, cred: McpOAuthCred) -> Result<(), String> {
    let mut store = get_mcp_oauth_store()?;
    store.creds_by_key.insert(credential_key.to_string(), cred);
    store.refresh_failures.remove(credential_key);
    save_mcp_oauth_store(&store)
}

pub fn set_mcp_oauth_refresh_failure(credential_key: &str, message: String) -> Result<(), String> {
    let mut store = get_mcp_oauth_store()?;
    store
        .refresh_failures
        .insert(credential_key.to_string(), message);
    save_mcp_oauth_store(&store)
}

pub fn bind_mcp_oauth_server(
    client: &str,
    server_name: &str,
    credential_key: &str,
) -> Result<(), String> {
    let mut store = get_mcp_oauth_store()?;
    store
        .server_bindings
        .insert(binding_key(client, server_name), credential_key.to_string());
    save_mcp_oauth_store(&store)
}

pub fn get_mcp_oauth_binding(client: &str, server_name: &str) -> Result<Option<String>, String> {
    let store = get_mcp_oauth_store()?;
    Ok(store
        .server_bindings
        .get(&binding_key(client, server_name))
        .cloned())
}

pub fn unbind_mcp_oauth_server(
    client: &str,
    server_name: &str,
) -> Result<Option<(String, McpOAuthCred, bool)>, String> {
    let mut store = get_mcp_oauth_store()?;
    let Some(credential_key) = store
        .server_bindings
        .remove(&binding_key(client, server_name))
    else {
        save_mcp_oauth_store(&store)?;
        return Ok(None);
    };

    let still_used = store.server_bindings.values().any(|v| v == &credential_key);
    let cred = store.creds_by_key.get(&credential_key).cloned();
    let removed_last = !still_used;

    if removed_last {
        store.creds_by_key.remove(&credential_key);
        store.refresh_failures.remove(&credential_key);
    }

    save_mcp_oauth_store(&store)?;
    Ok(cred.map(|c| (credential_key, c, removed_last)))
}

pub fn remove_mcp_oauth_cred(server_key: &str) -> Result<(), String> {
    let _ = unbind_mcp_oauth_server("kiro", server_key)?;
    Ok(())
}

pub fn get_or_init_proxy_runtime() -> Result<(u16, String), String> {
    let mut store = get_mcp_oauth_store()?;
    let mut changed = false;

    if store.proxy_port.is_none() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("分配反代端口失败: {e}"))?;
        store.proxy_port = Some(
            listener
                .local_addr()
                .map_err(|e| format!("获取端口失败: {e}"))?
                .port(),
        );
        changed = true;
    }

    if store.proxy_secret.is_none() {
        store.proxy_secret = Some(uuid::Uuid::new_v4().simple().to_string());
        changed = true;
    }

    if changed {
        save_mcp_oauth_store(&store)?;
    }

    let port = store
        .proxy_port
        .ok_or_else(|| "MCP OAuth 反代端口初始化失败".to_string())?;
    let secret = store
        .proxy_secret
        .ok_or_else(|| "MCP OAuth 反代密钥初始化失败".to_string())?;

    Ok((port, secret))
}

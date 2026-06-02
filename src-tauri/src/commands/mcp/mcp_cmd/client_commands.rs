use crate::commands::app_settings_cmd::{
    get_mcp_oauth_binding, get_mcp_oauth_store, get_or_init_proxy_runtime, proxy_url_for_binding,
    unbind_mcp_oauth_server,
};
use crate::commands::common::run_blocking_task;

use super::adapters::{
    delete_mcp_server_for_client, load_mcp_items_for_client, read_mcp_server_for_client,
    write_mcp_server_for_client, write_mcp_server_url_for_client, McpClientKind,
};
use super::types::{McpClientStats, McpClientsOverview, McpServerItem};

#[tauri::command]
pub async fn get_mcp_config_by_client(client: String) -> Result<Vec<McpServerItem>, String> {
    run_blocking_task(move || load_mcp_items_for_client(McpClientKind::parse(&client)?)).await
}

fn stats_for_items(client: &str, items: &[McpServerItem]) -> McpClientStats {
    let enabled_servers = items.iter().filter(|server| !server.disabled).count();
    McpClientStats {
        client: client.to_string(),
        total_servers: items.len(),
        enabled_servers,
        estimated_tools: enabled_servers * 7,
    }
}

#[tauri::command]
pub async fn get_mcp_tool_stats_by_client(client: String) -> Result<McpClientStats, String> {
    run_blocking_task(move || {
        let kind = McpClientKind::parse(&client)?;
        let items = load_mcp_items_for_client(kind)?;
        Ok(stats_for_items(kind.as_key(), &items))
    })
    .await
}

#[tauri::command]
pub async fn get_mcp_clients_overview() -> Result<McpClientsOverview, String> {
    run_blocking_task(|| {
        let mut clients = Vec::new();
        for kind in [McpClientKind::Kiro, McpClientKind::Codex, McpClientKind::ClaudeCli] {
            let items = load_mcp_items_for_client(kind)?;
            clients.push(stats_for_items(kind.as_key(), &items));
        }

        Ok(McpClientsOverview {
            total_servers: clients.iter().map(|s| s.total_servers).sum(),
            enabled_servers: clients.iter().map(|s| s.enabled_servers).sum(),
            estimated_tools: clients.iter().map(|s| s.estimated_tools).sum(),
            clients,
        })
    })
    .await
}

#[tauri::command]
pub async fn toggle_mcp_server_by_client(
    client: String,
    name: String,
    disabled: bool,
) -> Result<(), String> {
    run_blocking_task(move || {
        let mut item = read_mcp_server_for_client(&client, &name)?;
        if let Some(obj) = item.raw.as_object_mut() {
            obj.insert("disabled".to_string(), serde_json::Value::Bool(disabled));
        }
        write_mcp_server_for_client(&client, &name, item.raw)
    })
    .await
}

#[tauri::command]
pub async fn delete_mcp_server_by_client(client: String, name: String) -> Result<(), String> {
    run_blocking_task(move || {
        let _ = unbind_mcp_oauth_server(&client, &name);
        delete_mcp_server_for_client(&client, &name)
    })
    .await
}

#[tauri::command]
pub async fn copy_mcp_server_to_client(
    from_client: String,
    to_client: String,
    name: String,
    overwrite: bool,
) -> Result<(), String> {
    run_blocking_task(move || {
        let source = read_mcp_server_for_client(&from_client, &name)?;
        if !overwrite && read_mcp_server_for_client(&to_client, &name).is_ok() {
            return Err(format!("{to_client} 已存在 MCP 服务器 {name}"));
        }

        write_mcp_server_for_client(&to_client, &name, source.raw)?;

        if let Some(credential_key) = get_mcp_oauth_binding(&from_client, &name)? {
            crate::commands::app_settings_cmd::bind_mcp_oauth_server(
                &to_client,
                &name,
                &credential_key,
            )?;
            let store = get_mcp_oauth_store()?;
            if store.creds_by_key.contains_key(&credential_key) {
                let (port, secret) = get_or_init_proxy_runtime()?;
                let proxy = proxy_url_for_binding(port, &secret, &credential_key, &name);
                write_mcp_server_url_for_client(&to_client, &name, &proxy)?;
            }
        }

        Ok(())
    })
    .await
}

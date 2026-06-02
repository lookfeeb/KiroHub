use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};

use crate::kiro::settings::mcp::{McpConfig, McpServer};
use crate::utils::fs::atomic_write;

use super::types::McpServerItem;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum McpClientKind {
    Kiro,
    Codex,
    ClaudeCli,
}

impl McpClientKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "kiro" => Ok(Self::Kiro),
            "codex" => Ok(Self::Codex),
            "claude-cli" | "claude_cli" | "claude" => Ok(Self::ClaudeCli),
            _ => Err(format!("不支持的 MCP 客户端: {value}")),
        }
    }

    pub fn as_key(self) -> &'static str {
        match self {
            Self::Kiro => "kiro",
            Self::Codex => "codex",
            Self::ClaudeCli => "claude-cli",
        }
    }
}

fn home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or("无法获取用户目录".to_string())
}

fn codex_config_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".codex").join("config.toml"))
}

fn claude_cli_config_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".claude.json"))
}

fn server_item_from_json(name: &str, client: McpClientKind, raw: serde_json::Value) -> McpServerItem {
    let url = raw.get("url").and_then(|v| v.as_str()).unwrap_or_default();
    let command = raw
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let raw_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or_default();
    let server_type = if !url.is_empty()
        || matches!(raw_type.to_ascii_lowercase().as_str(), "http" | "sse" | "url")
    {
        raw_type
            .is_empty()
            .then_some("url")
            .unwrap_or(raw_type)
            .to_ascii_lowercase()
    } else {
        "command".to_string()
    };

    McpServerItem {
        name: name.to_string(),
        client: client.as_key().to_string(),
        server_type,
        detail: if !url.is_empty() {
            url.to_string()
        } else {
            command.to_string()
        },
        disabled: raw
            .get("disabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        raw,
    }
}

fn kiro_server_to_raw(server: &McpServer) -> serde_json::Value {
    serde_json::to_value(server).unwrap_or_else(|_| serde_json::json!({}))
}

fn raw_to_kiro_server(raw: serde_json::Value) -> Result<McpServer, String> {
    serde_json::from_value(raw).map_err(|e| format!("转换 Kiro MCP 配置失败: {e}"))
}

fn load_kiro_items() -> Result<Vec<McpServerItem>, String> {
    let config = McpConfig::load()?;
    let mut items: Vec<_> = config
        .mcp_servers
        .into_iter()
        .map(|(name, server)| {
            server_item_from_json(&name, McpClientKind::Kiro, kiro_server_to_raw(&server))
        })
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

fn load_codex_doc() -> Result<(PathBuf, DocumentMut), String> {
    let path = codex_config_path()?;
    let content = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("读取 Codex config.toml 失败: {e}"))?
    } else {
        String::new()
    };
    let doc = content
        .parse::<DocumentMut>()
        .map_err(|e| format!("解析 Codex config.toml 失败: {e}"))?;
    Ok((path, doc))
}

fn save_codex_doc(path: &Path, doc: &DocumentMut) -> Result<(), String> {
    atomic_write(path, &doc.to_string(), "Codex config.toml")
}

fn toml_value_to_json(item: &Item) -> serde_json::Value {
    if let Some(value) = item.as_value() {
        if let Some(s) = value.as_str() {
            return serde_json::Value::String(s.to_string());
        }
        if let Some(b) = value.as_bool() {
            return serde_json::Value::Bool(b);
        }
        if let Some(i) = value.as_integer() {
            return serde_json::Value::Number(i.into());
        }
        if let Some(arr) = value.as_array() {
            return serde_json::Value::Array(
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| serde_json::Value::String(s.to_string())))
                    .collect(),
            );
        }
    }
    if let Some(table) = item.as_table() {
        let mut map = serde_json::Map::new();
        for (key, value) in table.iter() {
            map.insert(key.to_string(), toml_value_to_json(value));
        }
        return serde_json::Value::Object(map);
    }
    serde_json::Value::Null
}

fn load_codex_items() -> Result<Vec<McpServerItem>, String> {
    let (_, doc) = load_codex_doc()?;
    let mut items = Vec::new();
    if let Some(table) = doc.get("mcp_servers").and_then(|i| i.as_table()) {
        for (name, item) in table.iter() {
            items.push(server_item_from_json(
                name,
                McpClientKind::Codex,
                toml_value_to_json(item),
            ));
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

fn table_from_json(raw: &serde_json::Value) -> Table {
    let mut table = Table::new();
    if let Some(obj) = raw.as_object() {
        for (key, val) in obj {
            match val {
                serde_json::Value::String(s) => {
                    table.insert(key, value(s.clone()));
                }
                serde_json::Value::Bool(b) => {
                    table.insert(key, value(*b));
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        table.insert(key, value(i));
                    }
                }
                serde_json::Value::Array(arr) => {
                    let mut a = toml_edit::Array::default();
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            a.push(s);
                        }
                    }
                    table.insert(key, value(a));
                }
                serde_json::Value::Object(env) if key == "env" => {
                    let mut env_table = Table::new();
                    for (env_key, env_val) in env {
                        if let Some(s) = env_val.as_str() {
                            env_table.insert(env_key, value(s));
                        }
                    }
                    table.insert(key, Item::Table(env_table));
                }
                _ => {}
            }
        }
    }
    table
}

fn load_claude_root() -> Result<(PathBuf, serde_json::Value), String> {
    let path = claude_cli_config_path()?;
    if !path.exists() {
        return Ok((path, serde_json::json!({ "mcpServers": {} })));
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("读取 Claude CLI 配置失败: {e}"))?;
    let value = serde_json::from_str(&content).map_err(|e| format!("解析 Claude CLI 配置失败: {e}"))?;
    Ok((path, value))
}

fn save_claude_root(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value).map_err(|e| format!("序列化失败: {e}"))?;
    atomic_write(path, &content, "Claude CLI 配置")
}

fn load_claude_items() -> Result<Vec<McpServerItem>, String> {
    let (_, root) = load_claude_root()?;
    let mut items = Vec::new();
    if let Some(servers) = root.get("mcpServers").and_then(|v| v.as_object()) {
        for (name, raw) in servers {
            items.push(server_item_from_json(
                name,
                McpClientKind::ClaudeCli,
                raw.clone(),
            ));
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

pub fn load_mcp_items_for_client(client: McpClientKind) -> Result<Vec<McpServerItem>, String> {
    match client {
        McpClientKind::Kiro => load_kiro_items(),
        McpClientKind::Codex => load_codex_items(),
        McpClientKind::ClaudeCli => load_claude_items(),
    }
}

pub fn read_mcp_server_for_client(client: &str, server_name: &str) -> Result<McpServerItem, String> {
    let client = McpClientKind::parse(client)?;
    load_mcp_items_for_client(client)?
        .into_iter()
        .find(|s| s.name == server_name)
        .ok_or_else(|| format!("{} 中不存在 MCP 服务器 {server_name}", client.as_key()))
}

pub fn read_mcp_server_url_for_client(client: &str, server_name: &str) -> Result<String, String> {
    let item = read_mcp_server_for_client(client, server_name)?;
    if matches!(item.server_type.as_str(), "url" | "http" | "sse") {
        item.raw
            .get("url")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .ok_or_else(|| "该服务器没有 url 字段".to_string())
    } else {
        Err("command 型服务器不支持 OAuth".to_string())
    }
}

pub fn write_mcp_server_for_client(
    client: &str,
    server_name: &str,
    raw: serde_json::Value,
) -> Result<(), String> {
    match McpClientKind::parse(client)? {
        McpClientKind::Kiro => {
            let mut config = McpConfig::load()?;
            config
                .mcp_servers
                .insert(server_name.to_string(), raw_to_kiro_server(raw)?);
            config.save()
        }
        McpClientKind::Codex => {
            let (path, mut doc) = load_codex_doc()?;
            let root = doc.as_table_mut();
            let servers = root
                .entry("mcp_servers")
                .or_insert_with(|| Item::Table(Table::new()))
                .as_table_mut()
                .ok_or("Codex mcp_servers 不是 table")?;
            servers.insert(server_name, Item::Table(table_from_json(&raw)));
            save_codex_doc(&path, &doc)
        }
        McpClientKind::ClaudeCli => {
            let (path, mut root) = load_claude_root()?;
            if !root.is_object() {
                root = serde_json::json!({});
            }
            let obj = root
                .as_object_mut()
                .ok_or_else(|| "Claude MCP 配置根节点不是对象".to_string())?;
            let servers = obj
                .entry("mcpServers".to_string())
                .or_insert_with(|| serde_json::json!({}));
            if !servers.is_object() {
                *servers = serde_json::json!({});
            }
            servers
                .as_object_mut()
                .ok_or_else(|| "Claude mcpServers 节点不是对象".to_string())?
                .insert(server_name.to_string(), raw);
            save_claude_root(&path, &root)
        }
    }
}

pub fn write_mcp_server_url_for_client(
    client: &str,
    server_name: &str,
    new_url: &str,
) -> Result<(), String> {
    let mut item = read_mcp_server_for_client(client, server_name)?;
    if !matches!(item.server_type.as_str(), "url" | "http" | "sse") {
        return Err("command 型服务器不支持 OAuth".to_string());
    }
    if let Some(obj) = item.raw.as_object_mut() {
        obj.insert("url".to_string(), serde_json::Value::String(new_url.to_string()));
        if item.server_type == "url" {
            obj.entry("type".to_string())
                .or_insert_with(|| serde_json::Value::String("http".to_string()));
        }
    }
    write_mcp_server_for_client(client, server_name, item.raw)
}

pub fn delete_mcp_server_for_client(client: &str, name: &str) -> Result<(), String> {
    match McpClientKind::parse(client)? {
        McpClientKind::Kiro => {
            let mut config = McpConfig::load()?;
            config.mcp_servers.remove(name);
            config.save()
        }
        McpClientKind::Codex => {
            let (path, mut doc) = load_codex_doc()?;
            if let Some(table) = doc.get_mut("mcp_servers").and_then(|i| i.as_table_mut()) {
                table.remove(name);
            }
            save_codex_doc(&path, &doc)
        }
        McpClientKind::ClaudeCli => {
            let (path, mut root) = load_claude_root()?;
            if let Some(servers) = root.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
                servers.remove(name);
            }
            save_claude_root(&path, &root)
        }
    }
}

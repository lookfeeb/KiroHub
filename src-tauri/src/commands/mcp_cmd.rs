// MCP 服务器管理命令

#![allow(clippy::needless_pass_by_value)] // Tauri 命令需要按值传递参数

use crate::commands::app_settings_cmd::remove_mcp_oauth_cred;
use crate::commands::common::run_blocking_task;
use crate::kiro::settings::mcp::{McpConfig, McpServer};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 获取 MCP 配置（支持项目级合并）
#[tauri::command]
pub async fn get_mcp_config(project_dir: Option<String>) -> Result<McpConfig, String> {
    run_blocking_task(move || McpConfig::load_merged(project_dir.as_deref())).await
}

/// 保存/更新服务器配置
#[tauri::command]
pub async fn save_mcp_server(
    name: String,
    config: McpServer,
    project_dir: Option<String>,
) -> Result<(), String> {
    run_blocking_task(move || {
        // 验证配置
        validate_mcp_server(&config)?;

        if let Some(pd) = project_dir {
            let path = McpConfig::project_config_path(&pd);
            let mut mcp_config = McpConfig::load_from_path(&path)?;
            mcp_config.mcp_servers.insert(name, config);
            mcp_config.save_to_path(&path)
        } else {
            let mut mcp_config = McpConfig::load()?;
            mcp_config.mcp_servers.insert(name, config);
            mcp_config.save()
        }
    })
    .await
}

/// 验证 MCP 服务器配置
fn validate_mcp_server(config: &McpServer) -> Result<(), String> {
    match config {
        McpServer::Command(cmd) => {
            // 验证 command 字段
            if cmd.command.trim().is_empty() {
                return Err("command 字段不能为空".to_string());
            }

            // 验证 autoApprove 字段（可选）
            for tool in &cmd.auto_approve {
                if tool.trim().is_empty() {
                    return Err("autoApprove 中不能包含空字符串".to_string());
                }
            }

            Ok(())
        }
        McpServer::Url(url_config) => {
            // 验证 URL 格式
            if url_config.url.trim().is_empty() {
                return Err("url 字段不能为空".to_string());
            }

            // 简单的 URL 格式验证
            if !url_config.url.starts_with("http://") && !url_config.url.starts_with("https://") {
                return Err("url 必须以 http:// 或 https:// 开头".to_string());
            }

            Ok(())
        }
    }
}

/// 删除服务器
#[tauri::command]
pub async fn delete_mcp_server(name: String, project_dir: Option<String>) -> Result<(), String> {
    run_blocking_task(move || {
        if let Some(pd) = project_dir {
            let path = McpConfig::project_config_path(&pd);
            let mut mcp_config = McpConfig::load_from_path(&path)?;
            mcp_config.mcp_servers.remove(&name);
            mcp_config.save_to_path(&path)
        } else {
            let mut mcp_config = McpConfig::load()?;
            mcp_config.mcp_servers.remove(&name);
            mcp_config.save()
        }
    })
    .await
}

/// 启用/禁用服务器
#[tauri::command]
pub async fn toggle_mcp_server(
    name: String,
    disabled: bool,
    project_dir: Option<String>,
) -> Result<(), String> {
    run_blocking_task(move || {
        if let Some(pd) = project_dir {
            let path = McpConfig::project_config_path(&pd);
            let mut mcp_config = McpConfig::load_from_path(&path)?;
            if let Some(server) = mcp_config.mcp_servers.get_mut(&name) {
                match server {
                    McpServer::Command(cmd) => cmd.disabled = disabled,
                    McpServer::Url(url) => url.disabled = disabled,
                }
                mcp_config.save_to_path(&path)
            } else {
                Err(format!("服务器 {name} 不存在"))
            }
        } else {
            let mut mcp_config = McpConfig::load()?;
            if let Some(server) = mcp_config.mcp_servers.get_mut(&name) {
                match server {
                    McpServer::Command(cmd) => cmd.disabled = disabled,
                    McpServer::Url(url) => url.disabled = disabled,
                }
                mcp_config.save()
            } else {
                Err(format!("服务器 {name} 不存在"))
            }
        }
    })
    .await
}

/// 获取 MCP 工具统计信息（支持项目级合并）
#[tauri::command]
pub async fn get_mcp_tool_stats(project_dir: Option<String>) -> Result<serde_json::Value, String> {
    run_blocking_task(move || {
        let mcp_config = McpConfig::load_merged(project_dir.as_deref())?;

        let total_servers = mcp_config.mcp_servers.len();
        let enabled_servers = mcp_config
            .mcp_servers
            .values()
            .filter(|server| match server {
                McpServer::Command(cmd) => !cmd.disabled,
                McpServer::Url(url) => !url.disabled,
            })
            .count();

        // 估算工具数量：每个启用的服务器平均 5-10 个工具
        // 使用保守估计：每个服务器 7 个工具
        let estimated_tools = enabled_servers * 7;

        Ok(serde_json::json!({
            "totalServers": total_servers,
            "enabledServers": enabled_servers,
            "estimatedTools": estimated_tools,
        }))
    })
    .await
}


/// 扫描外部工具的 MCP 配置并导入 kiro 中尚不存在的服务器，返回新导入的服务器名
#[tauri::command]
pub async fn discover_and_import_mcp_servers() -> Result<Vec<String>, String> {
    run_blocking_task(|| {
        let mut config = McpConfig::load()?;
        let mut imported: Vec<String> = Vec::new();
        for (name, server) in scan_external_mcp_servers() {
            if !config.mcp_servers.contains_key(&name) {
                config.mcp_servers.insert(name.clone(), server);
                imported.push(name);
            }
        }
        if !imported.is_empty() {
            config.save()?;
            imported.sort();
        }
        Ok(imported)
    })
    .await
}

/// 收集候选外部 MCP 配置文件路径（其它工具）
fn external_mcp_config_files() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = [
        home.join(".cursor").join("mcp.json"),
        home.join(".codeium").join("windsurf").join("mcp_config.json"),
        home.join("AppData")
            .join("Roaming")
            .join("Claude")
            .join("claude_desktop_config.json"),
    ]
    .into_iter()
    .filter(|p| p.exists())
    .collect();
    // 递归扫描已知根目录（限定深度，避免全盘扫描）
    collect_mcp_files(&home.join(".codex"), 6, &mut files);
    files
}

/// 限定深度地收集名为 mcp.json / .mcp.json 的文件
fn collect_mcp_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mcp_files(&path, depth - 1, out);
        } else if matches!(
            path.file_name().and_then(|n| n.to_str()),
            Some("mcp.json" | ".mcp.json")
        ) {
            out.push(path);
        }
    }
}

/// 解析候选文件，提取其中可识别的 MCP 服务器（解析失败的条目静默跳过）
fn scan_external_mcp_servers() -> HashMap<String, McpServer> {
    let mut result = HashMap::new();
    for path in external_mcp_config_files() {
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        let Some(servers) = value.get("mcpServers").and_then(|v| v.as_object()) else {
            continue;
        };
        for (name, raw) in servers {
            if let Ok(server) = serde_json::from_value::<McpServer>(raw.clone()) {
                result.entry(name.clone()).or_insert(server);
            }
        }
    }
    result
}

/// 删除 MCP 服务器：移除 OAuth 凭证 + 从 kiro 配置删除 + 同步从外部来源文件删除
#[tauri::command]
pub async fn delete_mcp_server_synced(name: String) -> Result<(), String> {
    run_blocking_task(move || {
        // 1. 移除可能存在的 OAuth 凭证（不存在则忽略）
        let _ = remove_mcp_oauth_cred(&name);
        // 2. 从 kiro 用户级配置删除
        let mut config = McpConfig::load()?;
        config.mcp_servers.remove(&name);
        config.save()?;
        // 3. 同步从外部来源文件删除（多线程并行，保留各文件其余内容）
        let name_ref: &str = &name;
        std::thread::scope(|scope| {
            for path in external_mcp_config_files() {
                scope.spawn(move || remove_server_from_file(&path, name_ref));
            }
        });
        Ok(())
    })
    .await
}

/// 从单个外部配置文件的 mcpServers 中删除同名服务器，并写回（其余内容保留）
fn remove_server_from_file(path: &Path, name: &str) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return;
    };
    let removed = value
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .is_some_and(|m| m.remove(name).is_some());
    if removed {
        if let Ok(s) = serde_json::to_string_pretty(&value) {
            let _ = fs::write(path, s);
        }
    }
}

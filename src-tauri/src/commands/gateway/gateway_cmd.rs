#![allow(clippy::needless_pass_by_value)] // Tauri 命令需要按值传参

use tauri::{AppHandle, State};

use crate::gateway::{
    clear_gateway_request_logs as clear_gateway_request_logs_inner,
    get_gateway_config as get_gateway_config_inner,
    get_gateway_log_dir as get_gateway_log_dir_inner,
    get_gateway_request_logs as get_gateway_request_logs_inner,
    get_gateway_request_stats as get_gateway_request_stats_inner,
    get_gateway_model_stats as get_gateway_model_stats_inner,
    get_gateway_endpoint_stats as get_gateway_endpoint_stats_inner,
    get_gateway_status as get_gateway_status_inner,
    open_gateway_log_dir as open_gateway_log_dir_inner,
    save_gateway_config as save_gateway_config_inner, start_gateway as start_gateway_inner,
    stop_gateway as stop_gateway_inner, GatewayConfig, GatewayRequestLogEntry, GatewayRequestStats, GatewayStatus,
    log_store,
};
use crate::state::AppState;
use crate::utils::fs::atomic_write;

fn config_for_manual_start(config: &GatewayConfig) -> GatewayConfig {
    config.clone()
}

#[cfg(test)]
fn config_after_manual_stop(config: &GatewayConfig) -> GatewayConfig {
    config.clone()
}

#[tauri::command]
pub async fn start_gateway(
    state: State<'_, AppState>,
    config: GatewayConfig,
) -> Result<GatewayStatus, String> {
    start_gateway_inner(&state, config_for_manual_start(&config)).await
}

#[tauri::command]
pub async fn stop_gateway(state: State<'_, AppState>) -> Result<(), String> {
    stop_gateway_inner(&state).await
}

#[tauri::command]
pub async fn get_gateway_status(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    get_gateway_status_inner(&state).await
}

#[tauri::command]
pub async fn get_gateway_config() -> Result<GatewayConfig, String> {
    get_gateway_config_inner()
}

#[tauri::command]
pub async fn save_gateway_config(config: GatewayConfig) -> Result<(), String> {
    save_gateway_config_inner(&config)
}

#[tauri::command]
pub async fn get_gateway_log_dir(app: AppHandle) -> Result<String, String> {
    get_gateway_log_dir_inner(&app)
}

#[tauri::command]
pub async fn get_gateway_request_logs(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<GatewayRequestLogEntry>, String> {
    get_gateway_request_logs_inner(&state, limit).await
}

#[tauri::command]
pub async fn get_gateway_request_stats(
    state: State<'_, AppState>,
) -> Result<GatewayRequestStats, String> {
    get_gateway_request_stats_inner(&state).await
}

#[tauri::command]
pub async fn get_gateway_model_stats(
    state: State<'_, AppState>,
) -> Result<Vec<log_store::ModelStat>, String> {
    get_gateway_model_stats_inner(&state).await
}

#[tauri::command]
pub async fn get_gateway_endpoint_stats(
    state: State<'_, AppState>,
) -> Result<Vec<log_store::EndpointStat>, String> {
    get_gateway_endpoint_stats_inner(&state).await
}

#[tauri::command]
pub async fn open_gateway_log_dir(app: AppHandle) -> Result<String, String> {
    open_gateway_log_dir_inner(&app)
}

#[tauri::command]
pub async fn clear_gateway_request_logs(
    state: State<'_, AppState>,
) -> Result<(), String> {
    clear_gateway_request_logs_inner(&state).await
}

/// 一键配置反代客户端（Claude Code / Codex）
#[tauri::command]
pub async fn configure_proxy_clients(
    clients: Vec<String>,
    host: String,
    port: u16,
    api_key: String,
) -> Result<Vec<ProxyClientConfigResult>, String> {
    let mut results = Vec::new();
    let proxy_origin = if host == "0.0.0.0" || host == "::" {
        format!("http://127.0.0.1:{}", port)
    } else {
        format!("http://{}:{}", host, port)
    };
    let openai_base_url = format!("{}/v1", proxy_origin);

    for client in &clients {
        let result = match client.as_str() {
            "claudeCode" => configure_claude_code(&proxy_origin, &api_key),
            "codex" => configure_codex(&openai_base_url, &api_key),
            _ => Err(format!("不支持的客户端: {}", client)),
        };
        results.push(ProxyClientConfigResult {
            client: client.clone(),
            success: result.is_ok(),
            paths: result.as_ref().map(|p| p.clone()).unwrap_or_default(),
            error: result.err(),
        });
    }

    Ok(results)
}

#[derive(serde::Serialize)]
pub struct ProxyClientConfigResult {
    pub client: String,
    pub success: bool,
    pub paths: Vec<String>,
    pub error: Option<String>,
}

/// 配置 Claude Code: ~/.claude/settings.json
fn configure_claude_code(proxy_origin: &str, api_key: &str) -> Result<Vec<String>, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "无法获取用户主目录".to_string())?;

    let settings_path = std::path::Path::new(&home).join(".claude").join("settings.json");
    let legacy_path = std::path::Path::new(&home).join(".claude").join("claude.json");

    // 选择正确的配置文件路径
    let path = if settings_path.exists() || !legacy_path.exists() {
        &settings_path
    } else {
        &legacy_path
    };

    // 确保目录存在
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 备份原文件
    let mut written_paths = Vec::new();
    if path.exists() {
        let backup_path = format!("{}.kiro-backup-{}", path.display(), chrono::Local::now().format("%Y%m%d%H%M%S"));
        std::fs::copy(path, &backup_path)
            .map_err(|e| format!("备份失败: {}", e))?;
        written_paths.push(backup_path);
    }

    // 读取或创建配置
    let mut config: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取配置失败: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("解析 Claude 配置失败，请先修复 JSON: {}", e))?
    } else {
        serde_json::json!({})
    };

    // 确保 env 字段存在
    if !config.get("env").is_some() {
        config["env"] = serde_json::json!({});
    }

    let env = config["env"].as_object_mut()
        .ok_or("env 字段不是对象".to_string())?;

    // 写入反代配置
    env.insert("ANTHROPIC_BASE_URL".to_string(), serde_json::Value::String(proxy_origin.to_string()));
    env.insert("ANTHROPIC_API_KEY".to_string(), serde_json::Value::String(api_key.to_string()));

    // 写入文件
    let content = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(path, &content, "Claude 配置")?;

    written_paths.insert(0, path.display().to_string());
    Ok(written_paths)
}

/// 配置 Codex: ~/.codex/auth.json + ~/.codex/config.toml
fn configure_codex(openai_base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "无法获取用户主目录".to_string())?;

    let codex_dir = std::path::Path::new(&home).join(".codex");
    let auth_path = codex_dir.join("auth.json");
    let config_path = codex_dir.join("config.toml");

    // 确保目录存在
    std::fs::create_dir_all(&codex_dir)
        .map_err(|e| format!("创建 .codex 目录失败: {}", e))?;

    let mut written_paths = Vec::new();
    let timestamp = chrono::Local::now().format("%Y%m%d%H%M%S");

    // 1. 写入 auth.json
    if auth_path.exists() {
        let backup = format!("{}.kiro-backup-{}", auth_path.display(), timestamp);
        std::fs::copy(&auth_path, &backup)
            .map_err(|e| format!("备份 auth.json 失败: {}", e))?;
    }

    let mut auth: serde_json::Value = if auth_path.exists() {
        let content = std::fs::read_to_string(&auth_path)
            .map_err(|e| format!("读取 auth.json 失败: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("解析 auth.json 失败，请先修复 JSON: {}", e))?
    } else {
        serde_json::json!({})
    };

    if let Some(obj) = auth.as_object_mut() {
        obj.insert("OPENAI_API_KEY".to_string(), serde_json::Value::String(api_key.to_string()));
    }

    let auth_content = serde_json::to_string_pretty(&auth)
        .map_err(|e| format!("序列化 auth.json 失败: {}", e))?;
    atomic_write(&auth_path, &auth_content, "auth.json")?;
    written_paths.push(auth_path.display().to_string());

    // 2. 写入 config.toml
    if config_path.exists() {
        let backup = format!("{}.kiro-backup-{}", config_path.display(), timestamp);
        std::fs::copy(&config_path, &backup)
            .map_err(|e| format!("备份 config.toml 失败: {}", e))?;
    }

    let existing_toml = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .map_err(|e| format!("读取 config.toml 失败: {}", e))?
    } else {
        String::new()
    };

    // 构建新的 config.toml 内容
    let new_toml = build_codex_config_toml(&existing_toml, openai_base_url);
    atomic_write(&config_path, &new_toml, "config.toml")?;
    written_paths.push(config_path.display().to_string());

    Ok(written_paths)
}

/// 构建 Codex config.toml 内容
fn build_codex_config_toml(existing: &str, openai_base_url: &str) -> String {
    let newline = if existing.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = existing.lines().collect();
    let mut result: Vec<String> = Vec::new();
    let mut in_custom_section = false;
    let mut base_url_replaced = false;

    for line in &lines {
        let trimmed = line.trim();

        // 进入 [model_providers.custom] section
        if trimmed == "[model_providers.custom]" || trimmed == "[model_providers.\"custom\"]" {
            in_custom_section = true;
            result.push(line.to_string());
            continue;
        }

        // 离开 custom section（遇到下一个 section）
        if in_custom_section && trimmed.starts_with('[') {
            in_custom_section = false;
        }

        // 在 custom section 内替换 base_url
        if in_custom_section && trimmed.starts_with("base_url") {
            result.push(format!("base_url = \"{}\"", openai_base_url));
            base_url_replaced = true;
            continue;
        }

        result.push(line.to_string());
    }

    // 如果没找到 [model_providers.custom]，追加一个
    if !base_url_replaced {
        result.push(String::new());
        result.push("[model_providers.custom]".to_string());
        result.push("name = \"custom\"".to_string());
        result.push(format!("base_url = \"{}\"", openai_base_url));
        result.push("wire_api = \"responses\"".to_string());
        result.push("requires_openai_auth = true".to_string());
    }

    result.join(newline)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_start_preserves_auto_start_preference() {
        let config = GatewayConfig {
            enabled: false,
            ..GatewayConfig::default()
        };

        let next = config_for_manual_start(&config);

        assert!(
            !next.enabled,
            "manual start should not force auto-start preference on"
        );
    }

    #[test]
    fn manual_stop_preserves_auto_start_preference() {
        let config = GatewayConfig {
            enabled: true,
            ..GatewayConfig::default()
        };

        let next = config_after_manual_stop(&config);

        assert!(
            next.enabled,
            "manual stop should not clear auto-start preference"
        );
    }
}

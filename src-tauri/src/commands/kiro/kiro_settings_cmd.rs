// Kiro IDE 设置命令 (读写 Kiro IDE 的 settings.json)

#![allow(clippy::needless_pass_by_value)] // Tauri 命令需要按值传递参数
#![allow(clippy::too_many_lines)] // 设置命令文件包含多个函数

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::utils::fs::atomic_write;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // 设置结构体需要多个布尔字段来表示不同的开关选项
pub struct KiroSettings {
    pub http_proxy: Option<String>,
    pub model_selection: Option<String>,
    pub enable_codebase_indexing: bool,
    pub trusted_commands_mode: Option<String>,
    pub custom_trusted_commands: Option<String>,
    // Agent 设置
    pub agent_autonomy: Option<String>,
    pub enable_tab_autocomplete: bool,
    pub usage_summary: bool,
    pub enable_debug_logs: bool,
    // 通知设置
    pub notify_action_required: bool,
    pub notify_failure: bool,
    pub notify_success: bool,
    pub notify_billing: bool,
    // 新增设置
    pub trusted_tools: Vec<String>,
    pub reference_tracker: bool,
    pub configure_mcp: String, // "Enabled" | "Disabled"
    // 遥测设置
    pub telemetry_content_collection: bool,
    pub telemetry_usage_analytics: bool,
    pub telemetry_edit_stats: bool,
    pub telemetry_feedback: bool,
}

impl Default for KiroSettings {
    fn default() -> Self {
        Self {
            http_proxy: None,
            model_selection: Some("claude-sonnet-4.5".to_string()),
            enable_codebase_indexing: true,
            trusted_commands_mode: Some("none".to_string()),
            custom_trusted_commands: None,
            agent_autonomy: Some("Supervised".to_string()),
            enable_tab_autocomplete: true,
            usage_summary: true,
            enable_debug_logs: false,
            notify_action_required: true,
            notify_failure: true,
            notify_success: true,
            notify_billing: true,
            trusted_tools: vec![],
            reference_tracker: false,
            configure_mcp: "Enabled".to_string(),
            telemetry_content_collection: false,
            telemetry_usage_analytics: false,
            telemetry_edit_stats: false,
            telemetry_feedback: false,
        }
    }
}

fn get_kiro_settings_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(|appdata| {
            PathBuf::from(appdata)
                .join("Kiro")
                .join("User")
                .join("settings.json")
        })
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME").ok().map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Kiro")
                .join("User")
                .join("settings.json")
        })
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("HOME").ok().map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("Kiro")
                .join("User")
                .join("settings.json")
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

fn load_kiro_settings_json(path: &Path) -> Result<serde_json::Value, String> {
    if path.exists() {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("读取设置文件失败: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("解析 Kiro 设置失败，请先修复 JSON: {e}"))
    } else {
        Ok(serde_json::json!({}))
    }
}

fn write_kiro_settings_json(path: &Path, settings: &serde_json::Value) -> Result<(), String> {
    let content =
        serde_json::to_string_pretty(settings).map_err(|e| format!("序列化设置失败: {e}"))?;

    atomic_write(path, &content, "Kiro 设置文件")
}

async fn run_kiro_blocking<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|e| format!("Task failed: {e}"))?
}

fn upsert_bool_if_changed(json: &mut serde_json::Value, key: &str, desired: bool) -> bool {
    if json.get(key).and_then(serde_json::Value::as_bool) == Some(desired) {
        return false;
    }

    if let Some(obj) = json.as_object_mut() {
        obj.insert(key.to_string(), serde_json::Value::Bool(desired));
        return true;
    }

    false
}

fn upsert_string_if_changed(json: &mut serde_json::Value, key: &str, desired: &str) -> bool {
    if json.get(key).and_then(serde_json::Value::as_str) == Some(desired) {
        return false;
    }

    if let Some(obj) = json.as_object_mut() {
        obj.insert(
            key.to_string(),
            serde_json::Value::String(desired.to_string()),
        );
        return true;
    }

    false
}

fn upsert_json_if_changed(
    json: &mut serde_json::Value,
    key: &str,
    desired: serde_json::Value,
) -> bool {
    if json.get(key) == Some(&desired) {
        return false;
    }

    if let Some(obj) = json.as_object_mut() {
        obj.insert(key.to_string(), desired);
        return true;
    }

    false
}

fn get_string_value(json: &serde_json::Value, key: &str) -> Option<String> {
    json.get(key)
        .and_then(|value| value.as_str())
        .map(std::string::ToString::to_string)
}

fn get_optional_string_array(json: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    json.get(key).and_then(|value| {
        value.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect()
        })
    })
}

fn classify_trusted_commands(commands: &[String]) -> String {
    if commands.iter().any(|item| item == "*") {
        "all".to_string()
    } else if commands.is_empty() {
        "none".to_string()
    } else {
        "common".to_string()
    }
}

fn format_custom_trusted_commands(commands: &[String]) -> Option<String> {
    if commands.iter().any(|item| item == "*") || commands.is_empty() {
        None
    } else {
        Some(commands.join("\n"))
    }
}

fn resolve_trusted_tools(app_tools: Option<Vec<String>>, json: &serde_json::Value) -> Vec<String> {
    app_tools.unwrap_or_else(|| {
        get_optional_string_array(json, "kiroAgent.trustedTools").unwrap_or_default()
    })
}

fn resolve_configure_mcp(app_value: Option<String>, json: &serde_json::Value) -> String {
    app_value.unwrap_or_else(|| {
        get_string_value(json, "kiroAgent.configureMCP").unwrap_or_else(|| "Enabled".to_string())
    })
}

fn sync_optional_trusted_tools_if_changed(
    json: &mut serde_json::Value,
    app_tools: Option<Vec<String>>,
) -> bool {
    let Some(app_tools) = app_tools else {
        return false;
    };

    if app_tools == get_optional_string_array(json, "kiroAgent.trustedTools").unwrap_or_default() {
        return false;
    }

    upsert_json_if_changed(json, "kiroAgent.trustedTools", serde_json::json!(app_tools))
}

fn sync_optional_configure_mcp_if_changed(
    json: &mut serde_json::Value,
    app_value: Option<String>,
) -> bool {
    let Some(app_value) = app_value else {
        return false;
    };

    if app_value
        == get_string_value(json, "kiroAgent.configureMCP").unwrap_or_else(|| "Enabled".to_string())
    {
        return false;
    }

    upsert_string_if_changed(json, "kiroAgent.configureMCP", &app_value)
}

fn get_kiro_settings_inner() -> Result<KiroSettings, String> {
    let path = get_kiro_settings_path().ok_or("无法获取 Kiro 设置路径")?;

    if !path.exists() {
        return Ok(KiroSettings::default());
    }

    // 读取应用设置。读取失败时不能回退默认值继续同步，否则可能覆盖用户已有的 IDE 设置。
    let app_settings = super::app_settings_cmd::get_app_settings_inner()?;

    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取设置文件失败: {e}"))?;

    let mut json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("解析设置文件失败: {e}"))?;

    let mut ide_modified = false;

    // 核心逻辑：以 app-settings.json 为准，强制同步到 Kiro IDE
    // 首次启动时，app_settings 使用默认值，会将默认值写入 IDE

    // enableCodebaseIndexing
    let codebase_indexing = app_settings.enable_codebase_indexing.unwrap_or(true);
    ide_modified |= upsert_bool_if_changed(
        &mut json,
        "kiroAgent.enableCodebaseIndexing",
        codebase_indexing,
    );

    // enableTabAutocomplete
    let tab_autocomplete = app_settings.enable_tab_autocomplete.unwrap_or(true);
    ide_modified |= upsert_bool_if_changed(
        &mut json,
        "kiroAgent.enableTabAutocomplete",
        tab_autocomplete,
    );

    // usageSummary
    let usage_summary = app_settings.usage_summary.unwrap_or(true);
    ide_modified |= upsert_bool_if_changed(&mut json, "kiroAgent.usageSummary", usage_summary);

    // enableDebugLogs
    let debug_logs = app_settings.enable_debug_logs.unwrap_or(false);
    ide_modified |= upsert_bool_if_changed(&mut json, "kiroAgent.enableDebugLogs", debug_logs);

    // notifyActionRequired
    let notify_action = app_settings.notify_action_required.unwrap_or(true);
    ide_modified |= upsert_bool_if_changed(
        &mut json,
        "kiroAgent.notifications.agent.actionRequired",
        notify_action,
    );

    // notifyFailure
    let notify_failure = app_settings.notify_failure.unwrap_or(true);
    ide_modified |= upsert_bool_if_changed(
        &mut json,
        "kiroAgent.notifications.agent.failure",
        notify_failure,
    );

    // notifySuccess
    let notify_success = app_settings.notify_success.unwrap_or(true);
    ide_modified |= upsert_bool_if_changed(
        &mut json,
        "kiroAgent.notifications.agent.success",
        notify_success,
    );

    // notifyBilling
    let notify_billing = app_settings.notify_billing.unwrap_or(true);
    ide_modified |=
        upsert_bool_if_changed(&mut json, "kiroAgent.notifications.billing", notify_billing);

    // trustedTools
    ide_modified |=
        sync_optional_trusted_tools_if_changed(&mut json, app_settings.trusted_tools.clone());

    // referenceTracker
    let reference_tracker = app_settings.reference_tracker.unwrap_or(false);
    ide_modified |= upsert_bool_if_changed(
        &mut json,
        "kiroAgent.codeReferences.referenceTracker",
        reference_tracker,
    );

    // configureMCP
    ide_modified |=
        sync_optional_configure_mcp_if_changed(&mut json, app_settings.configure_mcp.clone());

    // telemetry: contentCollectionForServiceImprovement
    let tele_content = app_settings.telemetry_content_collection.unwrap_or(false);
    ide_modified |= upsert_bool_if_changed(
        &mut json,
        "telemetry.dataSharingAndPromptLogging.contentCollectionForServiceImprovement",
        tele_content,
    );

    // telemetry: usageAnalyticsAndPerformanceMetrics
    let tele_usage = app_settings.telemetry_usage_analytics.unwrap_or(false);
    ide_modified |= upsert_bool_if_changed(
        &mut json,
        "telemetry.dataSharingAndPromptLogging.usageAnalyticsAndPerformanceMetrics",
        tele_usage,
    );

    // telemetry: editStats
    let tele_edit = app_settings.telemetry_edit_stats.unwrap_or(false);
    ide_modified |= upsert_bool_if_changed(&mut json, "telemetry.editStats.enabled", tele_edit);

    // telemetry: feedback
    let tele_feedback = app_settings.telemetry_feedback.unwrap_or(false);
    ide_modified |= upsert_bool_if_changed(&mut json, "telemetry.feedback.enabled", tele_feedback);

    // 如果有修改，写入 Kiro IDE settings.json
    if ide_modified {
        write_kiro_settings_json(&path, &json)?;
    }

    let trusted_commands = get_optional_string_array(&json, "kiroAgent.trustedCommands");

    Ok(KiroSettings {
        http_proxy: get_string_value(&json, "http.proxy"),
        model_selection: get_string_value(&json, "kiroAgent.modelSelection"),
        enable_codebase_indexing: codebase_indexing,
        trusted_commands_mode: trusted_commands
            .as_ref()
            .map(|commands| classify_trusted_commands(commands)),
        custom_trusted_commands: trusted_commands
            .as_ref()
            .and_then(|commands| format_custom_trusted_commands(commands)),
        agent_autonomy: get_string_value(&json, "kiroAgent.agentAutonomy"),
        enable_tab_autocomplete: tab_autocomplete,
        usage_summary,
        enable_debug_logs: debug_logs,
        notify_action_required: notify_action,
        notify_failure,
        notify_success,
        notify_billing,
        trusted_tools: resolve_trusted_tools(app_settings.trusted_tools, &json),
        reference_tracker,
        configure_mcp: resolve_configure_mcp(app_settings.configure_mcp, &json),
        telemetry_content_collection: tele_content,
        telemetry_usage_analytics: tele_usage,
        telemetry_edit_stats: tele_edit,
        telemetry_feedback: tele_feedback,
    })
}

fn set_kiro_model_inner(model: String) -> Result<(), String> {
    let path = get_kiro_settings_path().ok_or("无法获取 Kiro 设置路径")?;

    let mut settings = load_kiro_settings_json(&path)?;

    if let Some(obj) = settings.as_object_mut() {
        obj.insert(
            "kiroAgent.modelSelection".to_string(),
            serde_json::Value::String(model),
        );
    }

    write_kiro_settings_json(&path, &settings)
}

#[tauri::command]
pub async fn get_kiro_settings() -> Result<KiroSettings, String> {
    run_kiro_blocking(get_kiro_settings_inner).await
}

#[tauri::command]
pub async fn set_kiro_model(model: String) -> Result<(), String> {
    run_kiro_blocking(move || set_kiro_model_inner(model)).await
}

// ===== 通用设置写入 =====

/// 统一的 Kiro IDE 设置写入：读 JSON、建父目录、确保对象、应用变更、原子写回。
fn update_kiro_setting<F>(mutate: F) -> Result<(), String>
where
    F: FnOnce(&mut serde_json::Value),
{
    let path = get_kiro_settings_path().ok_or("无法获取 Kiro 设置路径")?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建设置目录失败: {e}"))?;
    }

    let mut settings = load_kiro_settings_json(&path)?;
    if !settings.is_object() {
        settings = serde_json::json!({});
    }

    mutate(&mut settings);
    write_kiro_settings_json(&path, &settings)
}

/// 将前端的 mode + 自定义命令文本转换为 IDE 的 `kiroAgent.trustedCommands` 数组。
fn trusted_commands_value(mode: &str, custom_commands: &str) -> serde_json::Value {
    match mode {
        "all" => serde_json::json!(["*"]),
        "common" => {
            let list: Vec<String> = custom_commands
                .split('\n')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(String::from)
                .collect();
            serde_json::json!(list)
        }
        _ => serde_json::json!([]),
    }
}

#[tauri::command]
pub async fn set_kiro_codebase_indexing(enabled: bool) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_bool_if_changed(json, "kiroAgent.enableCodebaseIndexing", enabled);
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_trusted_commands(mode: String, custom_commands: String) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            let value = trusted_commands_value(&mode, &custom_commands);
            upsert_json_if_changed(json, "kiroAgent.trustedCommands", value);
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_agent_autonomy(autonomy: String) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_string_if_changed(json, "kiroAgent.agentAutonomy", &autonomy);
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_tab_autocomplete(enabled: bool) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_bool_if_changed(json, "kiroAgent.enableTabAutocomplete", enabled);
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_usage_summary(enabled: bool) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_bool_if_changed(json, "kiroAgent.usageSummary", enabled);
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_debug_logs(enabled: bool) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_bool_if_changed(json, "kiroAgent.enableDebugLogs", enabled);
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_trusted_tools(tools: Vec<String>) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_json_if_changed(json, "kiroAgent.trustedTools", serde_json::json!(tools));
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_reference_tracker(enabled: bool) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_bool_if_changed(json, "kiroAgent.codeReferences.referenceTracker", enabled);
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_configure_mcp(mode: String) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_string_if_changed(json, "kiroAgent.configureMCP", &mode);
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_notification(key: String, enabled: bool) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_bool_if_changed(json, &key, enabled);
        })
    })
    .await
}

#[tauri::command]
pub async fn set_kiro_telemetry(key: String, enabled: bool) -> Result<(), String> {
    run_kiro_blocking(move || {
        update_kiro_setting(|json| {
            upsert_bool_if_changed(json, &key, enabled);
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::{
        classify_trusted_commands, format_custom_trusted_commands, get_optional_string_array,
        sync_optional_configure_mcp_if_changed, sync_optional_trusted_tools_if_changed,
        trusted_commands_value, upsert_bool_if_changed, upsert_json_if_changed,
        upsert_string_if_changed,
    };

    #[test]
    fn upsert_bool_if_changed_only_marks_when_value_changes() {
        let mut json = serde_json::json!({
            "kiroAgent.enableDebugLogs": false
        });

        assert!(!upsert_bool_if_changed(
            &mut json,
            "kiroAgent.enableDebugLogs",
            false
        ));
        assert!(upsert_bool_if_changed(
            &mut json,
            "kiroAgent.enableDebugLogs",
            true
        ));
        assert_eq!(
            json.get("kiroAgent.enableDebugLogs")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn upsert_string_and_json_helpers_preserve_expected_values() {
        let mut json = serde_json::json!({
            "kiroAgent.configureMCP": "Enabled",
            "kiroAgent.trustedTools": ["tool-a"]
        });

        assert!(!upsert_string_if_changed(
            &mut json,
            "kiroAgent.configureMCP",
            "Enabled"
        ));
        assert!(upsert_string_if_changed(
            &mut json,
            "kiroAgent.configureMCP",
            "Disabled"
        ));
        assert!(upsert_json_if_changed(
            &mut json,
            "kiroAgent.trustedTools",
            serde_json::json!(["tool-b", "tool-c"])
        ));

        assert_eq!(
            json.get("kiroAgent.configureMCP")
                .and_then(serde_json::Value::as_str),
            Some("Disabled")
        );
        assert_eq!(
            json.get("kiroAgent.trustedTools"),
            Some(&serde_json::json!(["tool-b", "tool-c"]))
        );
    }

    #[test]
    fn upsert_helpers_leave_non_object_json_unchanged() {
        let mut json = serde_json::json!(["not-an-object"]);

        assert!(!upsert_bool_if_changed(
            &mut json,
            "kiroAgent.enableDebugLogs",
            true
        ));
        assert!(!upsert_string_if_changed(
            &mut json,
            "kiroAgent.configureMCP",
            "Disabled"
        ));
        assert!(!upsert_json_if_changed(
            &mut json,
            "kiroAgent.trustedTools",
            serde_json::json!(["tool-a"])
        ));
        assert_eq!(json, serde_json::json!(["not-an-object"]));
    }

    #[test]
    fn trusted_command_helpers_preserve_existing_mode_rules() {        assert_eq!(classify_trusted_commands(&[]), "none");
        assert_eq!(classify_trusted_commands(&["*".to_string()]), "all");
        assert_eq!(
            classify_trusted_commands(&["git status".to_string(), "cargo test *".to_string()]),
            "common"
        );

        assert_eq!(format_custom_trusted_commands(&[]), None);
        assert_eq!(format_custom_trusted_commands(&["*".to_string()]), None);
        assert_eq!(
            format_custom_trusted_commands(&["git status".to_string(), "cargo test *".to_string()]),
            Some("git status\ncargo test *".to_string())
        );
    }

    #[test]
    fn trusted_commands_value_inverts_classify_and_format() {
        assert_eq!(trusted_commands_value("all", ""), serde_json::json!(["*"]));
        assert_eq!(trusted_commands_value("none", "ignored"), serde_json::json!([]));
        assert_eq!(
            trusted_commands_value("common", "git status\n  cargo test *  \n\n"),
            serde_json::json!(["git status", "cargo test *"])
        );
    }

    #[test]
    fn get_optional_string_array_filters_non_string_entries() {
        let json = serde_json::json!({
            "kiroAgent.trustedTools": ["tool-a", 1, null, "tool-b"]
        });

        assert_eq!(
            get_optional_string_array(&json, "kiroAgent.trustedTools"),
            Some(vec!["tool-a".to_string(), "tool-b".to_string()])
        );
        assert_eq!(get_optional_string_array(&json, "missing"), None);
    }

    #[test]
    fn resolve_trusted_tools_prefers_app_settings_then_json_then_empty() {
        let json = serde_json::json!({
            "kiroAgent.trustedTools": ["json-tool"]
        });

        assert_eq!(
            super::resolve_trusted_tools(Some(vec!["app-tool".to_string()]), &json),
            vec!["app-tool".to_string()]
        );
        assert_eq!(
            super::resolve_trusted_tools(None, &json),
            vec!["json-tool".to_string()]
        );
        assert_eq!(
            super::resolve_trusted_tools(None, &serde_json::json!({})),
            Vec::<String>::new()
        );
    }

    #[test]
    fn resolve_configure_mcp_prefers_app_settings_then_json_then_enabled() {
        let json = serde_json::json!({
            "kiroAgent.configureMCP": "Disabled"
        });

        assert_eq!(
            super::resolve_configure_mcp(Some("Enabled".to_string()), &json),
            "Enabled".to_string()
        );
        assert_eq!(
            super::resolve_configure_mcp(None, &json),
            "Disabled".to_string()
        );
        assert_eq!(
            super::resolve_configure_mcp(None, &serde_json::json!({})),
            "Enabled".to_string()
        );
    }

    #[test]
    fn sync_optional_trusted_tools_if_changed_only_updates_for_explicit_app_values() {
        let mut unchanged = serde_json::json!({
            "kiroAgent.trustedTools": ["json-tool"]
        });
        let mut changed = unchanged.clone();
        let mut missing = serde_json::json!({});

        assert!(!sync_optional_trusted_tools_if_changed(
            &mut unchanged,
            None
        ));
        assert_eq!(
            unchanged.get("kiroAgent.trustedTools"),
            Some(&serde_json::json!(["json-tool"]))
        );

        assert!(sync_optional_trusted_tools_if_changed(
            &mut changed,
            Some(vec!["app-tool".to_string()])
        ));
        assert_eq!(
            changed.get("kiroAgent.trustedTools"),
            Some(&serde_json::json!(["app-tool"]))
        );

        assert!(!sync_optional_trusted_tools_if_changed(
            &mut missing,
            Some(vec![])
        ));
        assert_eq!(missing.get("kiroAgent.trustedTools"), None);
    }

    #[test]
    fn sync_optional_configure_mcp_if_changed_only_updates_for_explicit_app_values() {
        let mut unchanged = serde_json::json!({
            "kiroAgent.configureMCP": "Enabled"
        });
        let mut changed = unchanged.clone();
        let mut missing = serde_json::json!({});

        assert!(!sync_optional_configure_mcp_if_changed(
            &mut unchanged,
            None
        ));
        assert_eq!(
            unchanged
                .get("kiroAgent.configureMCP")
                .and_then(serde_json::Value::as_str),
            Some("Enabled")
        );

        assert!(sync_optional_configure_mcp_if_changed(
            &mut changed,
            Some("Disabled".to_string())
        ));
        assert_eq!(
            changed
                .get("kiroAgent.configureMCP")
                .and_then(serde_json::Value::as_str),
            Some("Disabled")
        );

        assert!(!sync_optional_configure_mcp_if_changed(
            &mut missing,
            Some("Enabled".to_string())
        ));
        assert_eq!(missing.get("kiroAgent.configureMCP"), None);
    }
}

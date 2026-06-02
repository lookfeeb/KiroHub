use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::shared::{data_dir, run_blocking_io};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: Option<String>,
    pub locale: Option<String>,
    pub lock_model: Option<bool>,
    pub locked_model: Option<String>,
    pub auto_refresh: Option<bool>,
    pub auto_refresh_interval: Option<i32>,
    pub auto_change_machine_id: Option<bool>,
    pub browser_path: Option<String>,
    pub bind_machine_id_to_account: Option<bool>,
    pub privacy_mode: Option<bool>,
    pub auto_switch_enabled: Option<bool>,
    pub auto_switch_threshold: Option<f64>,
    pub auto_switch_interval: Option<i32>,
    pub enable_codebase_indexing: Option<bool>,
    pub enable_tab_autocomplete: Option<bool>,
    pub usage_summary: Option<bool>,
    pub enable_debug_logs: Option<bool>,
    pub notify_action_required: Option<bool>,
    pub notify_failure: Option<bool>,
    pub notify_success: Option<bool>,
    pub notify_billing: Option<bool>,
    pub trusted_tools: Option<Vec<String>>,
    pub reference_tracker: Option<bool>,
    pub configure_mcp: Option<String>,
    pub telemetry_content_collection: Option<bool>,
    pub telemetry_usage_analytics: Option<bool>,
    pub telemetry_edit_stats: Option<bool>,
    pub telemetry_feedback: Option<bool>,
    pub custom_kiro_path: Option<String>,
    pub close_to_tray: Option<bool>,
    pub current_cli_account_id: Option<String>,
    pub cli_launch_model: Option<String>,
    pub cli_launch_trust_all_tools: Option<bool>,
    pub cli_launch_agent: Option<String>,
    pub cli_launch_extra_args: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: Some("dark".to_string()),
            locale: Some("zh-CN".to_string()),
            lock_model: Some(false),
            locked_model: None,
            auto_refresh: Some(true),
            auto_refresh_interval: Some(50),
            auto_change_machine_id: Some(true),
            browser_path: None,
            bind_machine_id_to_account: Some(true),
            privacy_mode: Some(true),
            auto_switch_enabled: Some(false),
            auto_switch_threshold: Some(1.0),
            auto_switch_interval: Some(5),
            enable_codebase_indexing: Some(true),
            enable_tab_autocomplete: Some(true),
            usage_summary: Some(true),
            enable_debug_logs: Some(false),
            notify_action_required: Some(true),
            notify_failure: Some(true),
            notify_success: Some(true),
            notify_billing: Some(true),
            trusted_tools: None,
            reference_tracker: Some(false),
            configure_mcp: Some("Enabled".to_string()),
            telemetry_content_collection: Some(false),
            telemetry_usage_analytics: Some(false),
            telemetry_edit_stats: Some(false),
            telemetry_feedback: Some(false),
            custom_kiro_path: None,
            close_to_tray: Some(true),
            current_cli_account_id: None,
            cli_launch_model: Some("claude-sonnet-4.5".to_string()),
            cli_launch_trust_all_tools: Some(false),
            cli_launch_agent: None,
            cli_launch_extra_args: None,
        }
    }
}

impl AppSettings {
    fn apply_updates(&mut self, updates: Self) {
        macro_rules! apply_if_some {
            ($field:ident) => {
                if updates.$field.is_some() {
                    self.$field = updates.$field;
                }
            };
        }

        apply_if_some!(theme);
        apply_if_some!(locale);
        apply_if_some!(lock_model);
        apply_if_some!(locked_model);
        apply_if_some!(auto_refresh);
        apply_if_some!(auto_refresh_interval);
        apply_if_some!(auto_change_machine_id);
        apply_if_some!(browser_path);
        apply_if_some!(bind_machine_id_to_account);
        apply_if_some!(privacy_mode);
        apply_if_some!(auto_switch_enabled);
        apply_if_some!(auto_switch_threshold);
        apply_if_some!(auto_switch_interval);
        apply_if_some!(enable_codebase_indexing);
        apply_if_some!(enable_tab_autocomplete);
        apply_if_some!(usage_summary);
        apply_if_some!(enable_debug_logs);
        apply_if_some!(notify_action_required);
        apply_if_some!(notify_failure);
        apply_if_some!(notify_success);
        apply_if_some!(notify_billing);
        apply_if_some!(trusted_tools);
        apply_if_some!(reference_tracker);
        apply_if_some!(configure_mcp);
        apply_if_some!(telemetry_content_collection);
        apply_if_some!(telemetry_usage_analytics);
        apply_if_some!(telemetry_edit_stats);
        apply_if_some!(telemetry_feedback);
        apply_if_some!(custom_kiro_path);
        apply_if_some!(close_to_tray);
        apply_if_some!(current_cli_account_id);
        apply_if_some!(cli_launch_model);
        apply_if_some!(cli_launch_trust_all_tools);
        apply_if_some!(cli_launch_agent);
        apply_if_some!(cli_launch_extra_args);
    }
}

fn app_settings_path() -> PathBuf {
    data_dir().join("app-settings.json")
}

pub fn get_app_settings_inner() -> Result<AppSettings, String> {
    if let Some(json) = crate::db::kv_get("app_settings", "settings")? {
        return serde_json::from_str(&json).map_err(|e| format!("解析设置失败: {e}"));
    }

    let settings = migrate_legacy_app_settings()?.unwrap_or_default();
    save_settings_to_file(&settings)?;
    Ok(settings)
}

pub fn save_settings_to_file(settings: &AppSettings) -> Result<(), String> {
    let content = serde_json::to_string(settings).map_err(|e| format!("序列化失败: {e}"))?;
    crate::db::kv_set("app_settings", "settings", &content)
}

fn migrate_legacy_app_settings() -> Result<Option<AppSettings>, String> {
    let path = app_settings_path();
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取旧设置文件失败 ({}): {e}", path.display()))?;
    let settings: AppSettings = serde_json::from_str(&content)
        .map_err(|e| format!("解析旧设置文件失败 ({}): {e}", path.display()))?;
    std::fs::rename(&path, path.with_extension("json.bak"))
        .map_err(|e| format!("备份旧设置文件失败 ({}): {e}", path.display()))?;
    Ok(Some(settings))
}

fn save_app_settings_inner(updates: AppSettings) -> Result<(), String> {
    let mut current = get_app_settings_inner()?;
    current.apply_updates(updates);
    save_settings_to_file(&current)
}

#[tauri::command]
pub async fn get_app_settings() -> Result<AppSettings, String> {
    run_blocking_io(get_app_settings_inner).await
}

#[tauri::command]
pub async fn save_app_settings(settings: AppSettings) -> Result<(), String> {
    run_blocking_io(move || save_app_settings_inner(settings)).await
}

pub fn get_browser_path() -> Option<String> {
    get_app_settings_inner()
        .ok()
        .and_then(|s| s.browser_path)
        .filter(|p| !p.is_empty())
}

pub fn set_current_cli_account_id_inner(account_id: &str) -> Result<(), String> {
    let mut current = get_app_settings_inner()?;
    current.current_cli_account_id = Some(account_id.to_string());
    save_settings_to_file(&current)
}

#[tauri::command]
pub async fn get_current_cli_account_id() -> Result<Option<String>, String> {
    run_blocking_io(|| get_app_settings_inner().map(|settings| settings.current_cli_account_id))
        .await
}

#[tauri::command]
pub async fn get_custom_kiro_path() -> Result<Option<String>, String> {
    run_blocking_io(|| get_app_settings_inner().map(|s| s.custom_kiro_path)).await
}

#[tauri::command]
pub async fn set_custom_kiro_path(path: String) -> Result<(), String> {
    run_blocking_io(move || {
        save_app_settings_inner(AppSettings {
            custom_kiro_path: Some(path),
            ..Default::default()
        })
    })
    .await
}

#[tauri::command]
pub async fn clear_custom_kiro_path() -> Result<(), String> {
    run_blocking_io(|| {
        save_app_settings_inner(AppSettings {
            custom_kiro_path: Some(String::new()),
            ..Default::default()
        })
    })
    .await
}

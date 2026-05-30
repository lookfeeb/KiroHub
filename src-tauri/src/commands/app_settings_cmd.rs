// 应用自身设置命令 (存到 ~/.kirohub/app-settings.json)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: Option<String>,
    pub locale: Option<String>, // 界面语言
    pub lock_model: Option<bool>,
    pub locked_model: Option<String>,
    pub auto_refresh: Option<bool>,
    pub auto_refresh_interval: Option<i32>,
    pub auto_change_machine_id: Option<bool>, // 切换账号时是否更换机器码（默认 true）
    pub browser_path: Option<String>,
    // 账户机器码绑定功能
    pub bind_machine_id_to_account: Option<bool>, // true=绑定模式（每个账号固定机器码），false=随机模式
    // 隐私模式：脱敏显示邮箱
    pub privacy_mode: Option<bool>,
    // 自动换号设置
    pub auto_switch_enabled: Option<bool>,
    pub auto_switch_threshold: Option<f64>,
    pub auto_switch_interval: Option<i32>,
    // Kiro IDE 开关设置（用户偏好）
    pub enable_codebase_indexing: Option<bool>,
    pub enable_tab_autocomplete: Option<bool>,
    pub usage_summary: Option<bool>,
    pub enable_debug_logs: Option<bool>,
    pub notify_action_required: Option<bool>,
    pub notify_failure: Option<bool>,
    pub notify_success: Option<bool>,
    pub notify_billing: Option<bool>,
    // 新增 Kiro IDE 设置
    pub trusted_tools: Option<Vec<String>>,
    pub reference_tracker: Option<bool>,
    pub configure_mcp: Option<String>,
    pub telemetry_content_collection: Option<bool>,
    pub telemetry_usage_analytics: Option<bool>,
    pub telemetry_edit_stats: Option<bool>,
    pub telemetry_feedback: Option<bool>,
    // Kiro IDE 自定义安装路径
    pub custom_kiro_path: Option<String>,
    // 关闭窗口时的行为
    pub close_to_tray: Option<bool>, // true=最小化到托盘, false=直接退出
    // 当前已切换到 CLI 的账号 id（用于前端标记 CLI 当前账号）
    pub current_cli_account_id: Option<String>,
    // CLI 启动参数（kiro-cli chat）
    pub cli_launch_model: Option<String>,
    pub cli_launch_trust_all_tools: Option<bool>,
    pub cli_launch_agent: Option<String>,
    pub cli_launch_extra_args: Option<String>,
}

// 兼容旧配置文件中的 redeem_server 字段（已废弃）
// 读取时忽略，不再写入

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: Some("dark".to_string()),
            locale: Some("zh-CN".to_string()),
            lock_model: Some(false),
            locked_model: None,
            auto_refresh: Some(true),
            auto_refresh_interval: Some(50),
            auto_change_machine_id: Some(true), // 默认开启
            browser_path: None,
            bind_machine_id_to_account: Some(true),
            privacy_mode: Some(true), // 默认开启
            // 自动换号默认值
            auto_switch_enabled: Some(false),
            auto_switch_threshold: Some(1.0),
            auto_switch_interval: Some(5),
            // Kiro IDE 开关默认值
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
            close_to_tray: Some(false), // 默认直接退出，由用户主动开启最小化到托盘
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

fn get_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home)
        })
        .join(".kirohub")
}

fn get_app_settings_path() -> PathBuf {
    get_data_dir().join("app-settings.json")
}

async fn run_blocking_io<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|e| format!("Task failed: {e}"))?
}

pub fn get_app_settings_inner() -> Result<AppSettings, String> {
    if let Some(json) = crate::db::kv_get("app_settings", "settings")? {
        return serde_json::from_str(&json).map_err(|e| format!("解析设置失败: {e}"));
    }
    // 首次：迁移旧 app-settings.json（若有），否则用默认值；并写入 DB
    let settings = migrate_legacy_app_settings().unwrap_or_default();
    save_settings_to_file(&settings)?;
    Ok(settings)
}

pub fn save_settings_to_file(settings: &AppSettings) -> Result<(), String> {
    let content = serde_json::to_string(settings).map_err(|e| format!("序列化失败: {e}"))?;
    crate::db::kv_set("app_settings", "settings", &content)
}

/// 一次性迁移旧 app-settings.json：解析成功后改名为 *.json.bak
fn migrate_legacy_app_settings() -> Option<AppSettings> {
    let path = get_app_settings_path();
    let content = std::fs::read_to_string(&path).ok()?;
    let settings: AppSettings = serde_json::from_str(&content).ok()?;
    let _ = std::fs::rename(&path, path.with_extension("json.bak"));
    Some(settings)
}

fn save_app_settings_inner(updates: AppSettings) -> Result<(), String> {
    let mut current = get_app_settings_inner().unwrap_or_default();

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

/// 获取自定义浏览器路径（供打开浏览器时使用）
pub fn get_browser_path() -> Option<String> {
    get_app_settings_inner()
        .ok()
        .and_then(|s| s.browser_path)
        .filter(|p| !p.is_empty())
}

/// 记录当前已切换到 CLI 的账号 id
pub fn set_current_cli_account_id_inner(account_id: &str) -> Result<(), String> {
    let mut current = get_app_settings_inner().unwrap_or_default();
    current.current_cli_account_id = Some(account_id.to_string());
    save_settings_to_file(&current)
}

/// 获取当前 CLI 账号 id
#[tauri::command]
pub async fn get_current_cli_account_id() -> Result<Option<String>, String> {
    run_blocking_io(|| Ok(get_app_settings_inner().unwrap_or_default().current_cli_account_id))
        .await
}

// ============================================================
// 账号绑定机器码功能（已废弃，保留空实现兼容旧调用）
// ============================================================

#[tauri::command]
pub async fn bind_machine_id_to_account(
    _account_id: String,
    _machine_id: String,
) -> Result<(), String> {
    // 已废弃：机器码现在存储在账号的 machine_id 字段
    Ok(())
}

#[tauri::command]
pub async fn unbind_machine_id_from_account(_account_id: String) -> Result<(), String> {
    // 已废弃：机器码现在存储在账号的 machine_id 字段
    Ok(())
}

#[tauri::command]
pub async fn get_bound_machine_id(_account_id: String) -> Result<Option<String>, String> {
    // 已废弃：机器码现在存储在账号的 machine_id 字段
    Ok(None)
}

#[tauri::command]
pub async fn get_all_bound_machine_ids() -> Result<std::collections::HashMap<String, String>, String>
{
    // 已废弃：机器码现在存储在账号的 machine_id 字段
    Ok(std::collections::HashMap::new())
}

// ============================================================
// 使用量历史记录功能
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageHistoryEntry {
    pub date: String, // YYYY-MM-DD
    pub total_quota: i32,
    pub total_used: i32,
    pub account_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageHistory {
    pub entries: Vec<UsageHistoryEntry>,
}

fn get_usage_history_path() -> PathBuf {
    get_data_dir().join("usage-history.json")
}

fn load_usage_history_from_db() -> Result<UsageHistory, String> {
    let conn = crate::db::pool().get().map_err(|e| format!("获取数据库连接失败: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT data FROM usage_history ORDER BY recorded_at")
        .map_err(|e| format!("查询历史记录失败: {e}"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("读取历史记录失败: {e}"))?;
    let mut entries = Vec::new();
    for data in rows.flatten() {
        if let Ok(e) = serde_json::from_str::<UsageHistoryEntry>(&data) {
            entries.push(e);
        }
    }
    Ok(UsageHistory { entries })
}

fn save_usage_history_to_db(history: &UsageHistory) -> Result<(), String> {
    let mut conn = crate::db::pool().get().map_err(|e| format!("获取数据库连接失败: {e}"))?;
    let tx = conn.transaction().map_err(|e| format!("开启事务失败: {e}"))?;
    tx.execute("DELETE FROM usage_history", [])
        .map_err(|e| format!("清空历史记录失败: {e}"))?;
    {
        let mut stmt = tx
            .prepare("INSERT INTO usage_history(account_id,recorded_at,data) VALUES(NULL,?1,?2)")
            .map_err(|e| format!("准备插入失败: {e}"))?;
        for e in &history.entries {
            let data = serde_json::to_string(e).map_err(|err| format!("序列化失败: {err}"))?;
            stmt.execute(rusqlite::params![e.date, data])
                .map_err(|err| format!("写入历史记录失败: {err}"))?;
        }
    }
    tx.commit().map_err(|e| format!("提交事务失败: {e}"))
}

fn migrate_legacy_usage_history() -> Option<UsageHistory> {
    let path = get_usage_history_path();
    let content = std::fs::read_to_string(&path).ok()?;
    let history: UsageHistory = serde_json::from_str(&content).ok()?;
    let _ = std::fs::rename(&path, path.with_extension("json.bak"));
    Some(history)
}

fn get_usage_history_inner() -> Result<UsageHistory, String> {
    let history = load_usage_history_from_db()?;
    if history.entries.is_empty() {
        if let Some(legacy) = migrate_legacy_usage_history() {
            let _ = save_usage_history_to_db(&legacy);
            return Ok(legacy);
        }
    }
    Ok(history)
}

fn merge_usage_history_entry(history: &mut UsageHistory, entry: UsageHistoryEntry) {
    // 如果当天已有记录，则更新；否则添加新记录
    if let Some(existing) = history.entries.iter_mut().find(|e| e.date == entry.date) {
        existing.total_quota = entry.total_quota;
        existing.total_used = entry.total_used;
        existing.account_count = entry.account_count;
    } else {
        history.entries.push(entry);
    }

    // 只保留最近 30 天的记录
    history.entries.sort_by(|a, b| a.date.cmp(&b.date));
    if history.entries.len() > 30 {
        let skip_count = history.entries.len() - 30;
        history.entries.drain(..skip_count);
    }
}

fn save_usage_history_entry_inner(entry: UsageHistoryEntry) -> Result<(), String> {
    let mut history = get_usage_history_inner().unwrap_or_default();
    merge_usage_history_entry(&mut history, entry);
    save_usage_history_to_db(&history)
}

#[tauri::command]
pub async fn get_usage_history() -> Result<UsageHistory, String> {
    run_blocking_io(get_usage_history_inner).await
}

#[tauri::command]
pub async fn save_usage_history_entry(entry: UsageHistoryEntry) -> Result<(), String> {
    run_blocking_io(move || save_usage_history_entry_inner(entry)).await
}

// ============================================================
// 自定义 Kiro 安装路径
// ============================================================

#[tauri::command]
pub async fn get_custom_kiro_path() -> Result<Option<String>, String> {
    run_blocking_io(|| {
        get_app_settings_inner()
            .map(|s| s.custom_kiro_path)
    }).await
}

#[tauri::command]
pub async fn set_custom_kiro_path(path: String) -> Result<(), String> {
    run_blocking_io(move || {
        save_app_settings_inner(AppSettings {
            custom_kiro_path: Some(path),
            ..Default::default()
        })
    }).await
}

#[tauri::command]
pub async fn clear_custom_kiro_path() -> Result<(), String> {
    run_blocking_io(|| {
        save_app_settings_inner(AppSettings {
            custom_kiro_path: Some(String::new()),
            ..Default::default()
        })
    }).await
}

// ============================================================
// 远程 MCP 服务器 OAuth 凭证存储 (~/.kirohub/mcp-oauth.json)
// 独立文件存储，便于后台刷新任务与本地反代直接读取，且天然向后兼容。
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthCred {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64, // Unix 秒；0 表示未知
    pub auth_endpoint: String,
    pub token_endpoint: String,
    pub mcp_endpoint: String, // 真实上游 MCP 地址，反代转发目标
    pub resource: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthStore {
    pub creds: std::collections::HashMap<String, McpOAuthCred>, // key = serverKey, 如 "notion"
    pub proxy_port: Option<u16>,
    #[serde(default)]
    pub proxy_secret: Option<String>, // 本地反代路径密钥，防止本机其他进程盗用 token
}

fn get_mcp_oauth_path() -> PathBuf {
    get_data_dir().join("mcp-oauth.json")
}

pub fn get_mcp_oauth_store() -> Result<McpOAuthStore, String> {
    if let Some(json) = crate::db::kv_get("mcp_oauth", "store")? {
        return serde_json::from_str(&json).map_err(|e| format!("解析 MCP OAuth 失败: {e}"));
    }
    if let Some(store) = migrate_legacy_mcp_oauth() {
        let _ = save_mcp_oauth_store(&store);
        return Ok(store);
    }
    Ok(McpOAuthStore::default())
}

pub fn save_mcp_oauth_store(store: &McpOAuthStore) -> Result<(), String> {
    let content = serde_json::to_string(store).map_err(|e| format!("序列化失败: {e}"))?;
    crate::db::kv_set("mcp_oauth", "store", &content)
}

fn migrate_legacy_mcp_oauth() -> Option<McpOAuthStore> {
    let path = get_mcp_oauth_path();
    let content = std::fs::read_to_string(&path).ok()?;
    let store: McpOAuthStore = serde_json::from_str(&content).ok()?;
    let _ = std::fs::rename(&path, path.with_extension("json.bak"));
    Some(store)
}

/// 写入/更新某个 serverKey 的凭证
pub fn upsert_mcp_oauth_cred(server_key: &str, cred: McpOAuthCred) -> Result<(), String> {
    let mut store = get_mcp_oauth_store().unwrap_or_default();
    store.creds.insert(server_key.to_string(), cred);
    save_mcp_oauth_store(&store)
}

/// 删除某个 serverKey 的凭证
pub fn remove_mcp_oauth_cred(server_key: &str) -> Result<(), String> {
    let mut store = get_mcp_oauth_store().unwrap_or_default();
    store.creds.remove(server_key);
    save_mcp_oauth_store(&store)
}

/// 确保反代端口与本地密钥已分配并持久化，返回 (port, secret)。
/// 端口固定后写盘复用，保证 mcp.json 中的 URL 跨重启稳定。
pub fn get_or_init_proxy_runtime() -> Result<(u16, String), String> {
    let mut store = get_mcp_oauth_store().unwrap_or_default();
    let mut changed = false;
    if store.proxy_port.is_none() {
        // 绑定 0 让系统分配一个空闲端口，记录后复用
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
    Ok((
        store.proxy_port.expect("port set above"),
        store.proxy_secret.expect("secret set above"),
    ))
}



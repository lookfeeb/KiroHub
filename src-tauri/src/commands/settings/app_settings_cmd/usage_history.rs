use serde::{Deserialize, Serialize};

use super::shared::{data_dir, run_blocking_io};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageHistoryEntry {
    pub date: String,
    pub total_quota: i32,
    pub total_used: i32,
    pub account_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageHistory {
    pub entries: Vec<UsageHistoryEntry>,
}

fn usage_history_path() -> std::path::PathBuf {
    data_dir().join("usage-history.json")
}

fn load_usage_history_from_db() -> Result<UsageHistory, String> {
    let conn = crate::db::connection()?;
    let mut stmt = conn
        .prepare("SELECT data FROM usage_history ORDER BY recorded_at")
        .map_err(|e| format!("查询历史记录失败: {e}"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("读取历史记录失败: {e}"))?;
    let mut entries = Vec::new();
    for data in rows.flatten() {
        if let Ok(entry) = serde_json::from_str::<UsageHistoryEntry>(&data) {
            entries.push(entry);
        }
    }
    Ok(UsageHistory { entries })
}

fn save_usage_history_to_db(history: &UsageHistory) -> Result<(), String> {
    let mut conn = crate::db::connection()?;
    let tx = conn.transaction().map_err(|e| format!("开启事务失败: {e}"))?;
    tx.execute("DELETE FROM usage_history", [])
        .map_err(|e| format!("清空历史记录失败: {e}"))?;
    {
        let mut stmt = tx
            .prepare("INSERT INTO usage_history(account_id,recorded_at,data) VALUES(NULL,?1,?2)")
            .map_err(|e| format!("准备插入失败: {e}"))?;
        for entry in &history.entries {
            let data = serde_json::to_string(entry).map_err(|e| format!("序列化失败: {e}"))?;
            stmt.execute(rusqlite::params![entry.date, data])
                .map_err(|e| format!("写入历史记录失败: {e}"))?;
        }
    }
    tx.commit().map_err(|e| format!("提交事务失败: {e}"))
}

fn migrate_legacy_usage_history() -> Result<Option<UsageHistory>, String> {
    let path = usage_history_path();
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取旧用量历史失败 ({}): {e}", path.display()))?;
    let history: UsageHistory = serde_json::from_str(&content)
        .map_err(|e| format!("解析旧用量历史失败 ({}): {e}", path.display()))?;
    std::fs::rename(&path, path.with_extension("json.bak"))
        .map_err(|e| format!("备份旧用量历史失败 ({}): {e}", path.display()))?;
    Ok(Some(history))
}

fn get_usage_history_inner() -> Result<UsageHistory, String> {
    let history = load_usage_history_from_db()?;
    if history.entries.is_empty() {
        if let Some(legacy) = migrate_legacy_usage_history()? {
            save_usage_history_to_db(&legacy)?;
            return Ok(legacy);
        }
    }
    Ok(history)
}

fn merge_usage_history_entry(history: &mut UsageHistory, entry: UsageHistoryEntry) {
    if let Some(existing) = history.entries.iter_mut().find(|e| e.date == entry.date) {
        existing.total_quota = entry.total_quota;
        existing.total_used = entry.total_used;
        existing.account_count = entry.account_count;
    } else {
        history.entries.push(entry);
    }

    history.entries.sort_by(|a, b| a.date.cmp(&b.date));
    if history.entries.len() > 30 {
        let skip_count = history.entries.len() - 30;
        history.entries.drain(..skip_count);
    }
}

fn save_usage_history_entry_inner(entry: UsageHistoryEntry) -> Result<(), String> {
    let mut history = get_usage_history_inner()?;
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

use super::*;

#[cfg(test)]
pub(crate) fn append_gateway_request_log_to_path(
    path: &Path,
    entry: &GatewayRequestLogEntry,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建请求日志目录失败: {e}"))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("打开请求日志失败: {e}"))?;
    let serialized =
        serde_json::to_string(entry).map_err(|e| format!("序列化请求日志失败: {e}"))?;
    writeln!(file, "{serialized}").map_err(|e| format!("写入请求日志失败: {e}"))
}

#[cfg(test)]
pub(crate) fn get_gateway_request_logs_from_path(
    path: &Path,
    limit: Option<usize>,
) -> Result<Vec<GatewayRequestLogEntry>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path).map_err(|e| format!("读取请求日志失败: {e}"))?;
    let reader = BufReader::new(file);
    let max_items = limit.unwrap_or(100).clamp(1, 500);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let Ok(line) = line else {
            continue;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<GatewayRequestLogEntry>(trimmed) {
            entries.push(entry);
        }
    }

    let start = entries.len().saturating_sub(max_items);
    let mut recent = entries.split_off(start);
    recent.reverse();
    Ok(recent)
}

#[cfg(test)]
pub(crate) fn clear_gateway_request_logs_at_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(path).map_err(|e| format!("清空请求日志失败: {e}"))
}

/// 懒初始化日志写入通道：首次调用时启动后台批量写任务（需在 tokio 运行时内，网关请求路径满足）。
pub(crate) fn request_log_sender() -> &'static tokio::sync::mpsc::UnboundedSender<GatewayRequestLogEntry> {
    REQUEST_LOG_TX.get_or_init(|| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<GatewayRequestLogEntry>();
        tokio::spawn(async move {
            while let Some(first) = rx.recv().await {
                let mut batch = vec![first];
                while let Ok(next) = rx.try_recv() {
                    batch.push(next);
                    if batch.len() >= 200 {
                        break;
                    }
                }
                if let Err(e) = flush_request_log_batch(&batch) {
                    eprintln!("[gateway] 写入请求日志失败: {e}");
                }
            }
        });
        tx
    })
}

/// 单事务内批量插入 gateway_request_log。
pub(crate) fn flush_request_log_batch(batch: &[GatewayRequestLogEntry]) -> Result<(), String> {
    let mut conn = crate::db::connection()?;
    let tx = conn.transaction().map_err(|e| format!("开启事务失败: {e}"))?;
    {
        let mut stmt = tx
            .prepare("INSERT INTO gateway_request_log(ts,account_id,data) VALUES(?1,?2,?3)")
            .map_err(|e| format!("准备插入失败: {e}"))?;
        for e in batch {
            let data = serde_json::to_string(e).map_err(|err| format!("序列化日志失败: {err}"))?;
            stmt.execute(rusqlite::params![e.occurred_at, e.upstream_source, data])
                .map_err(|err| format!("写入日志失败: {err}"))?;
        }
    }
    tx.commit().map_err(|e| format!("提交事务失败: {e}"))
}

/// 从 DB 读取最近 N 条（最新在前）。
pub(crate) fn get_gateway_request_logs_from_db(
    limit: Option<usize>,
) -> Result<Vec<GatewayRequestLogEntry>, String> {
    let conn = crate::db::connection()?;
    let max = limit.unwrap_or(100).clamp(1, 500) as i64;
    let mut stmt = conn
        .prepare("SELECT data FROM gateway_request_log ORDER BY id DESC LIMIT ?1")
        .map_err(|e| format!("查询请求日志失败: {e}"))?;
    let rows = stmt
        .query_map([max], |r| r.get::<_, String>(0))
        .map_err(|e| format!("读取请求日志失败: {e}"))?;
    let mut entries = Vec::new();
    for data in rows.flatten() {
        if let Ok(e) = serde_json::from_str::<GatewayRequestLogEntry>(&data) {
            entries.push(e);
        }
    }
    Ok(entries)
}

/// 清空 DB 中的请求日志。
pub(crate) fn clear_gateway_request_logs_db() -> Result<(), String> {
    let conn = crate::db::connection()?;
    conn.execute("DELETE FROM gateway_request_log", [])
        .map(|_| ())
        .map_err(|e| format!("清空请求日志失败: {e}"))
}

pub fn append_gateway_request_log(entry: &GatewayRequestLogEntry) -> Result<(), String> {
    #[cfg(test)]
    if let Some(path) = request_log_path_override() {
        return append_gateway_request_log_to_path(&path, entry);
    }
    let _ = request_log_sender().send(entry.clone());
    Ok(())
}

pub async fn get_gateway_request_logs(
    state: &tauri::State<'_, crate::state::AppState>,
    limit: Option<usize>,
) -> Result<Vec<GatewayRequestLogEntry>, String> {
    // 尝试从运行中的 gateway 的内存存储获取
    let log_store_opt = {
        let guard = state
            .gateway
            .lock()
            .map_err(|_| "获取 gateway 状态失败".to_string())?;

        guard.as_ref().map(|rt| rt.log_store.clone())
    };

    if let Some(log_store) = log_store_opt {
        // 从内存存储获取
        let logs = log_store.get_last(limit.unwrap_or(50)).await;
        return Ok(logs);
    }

    // gateway 未运行：从 SQLite 读取
    #[cfg(test)]
    if let Some(path) = request_log_path_override() {
        return get_gateway_request_logs_from_path(&path, limit);
    }
    get_gateway_request_logs_from_db(limit)
}

pub async fn clear_gateway_request_logs(
    state: &tauri::State<'_, crate::state::AppState>,
) -> Result<(), String> {
    // 清空内存存储
    let log_store_opt = {
        let guard = state
            .gateway
            .lock()
            .map_err(|_| "获取 gateway 状态失败".to_string())?;

        guard.as_ref().map(|rt| rt.log_store.clone())
    };

    if let Some(log_store) = log_store_opt {
        log_store.clear().await;
    }

    // 清空持久化日志
    #[cfg(test)]
    if let Some(path) = request_log_path_override() {
        return clear_gateway_request_logs_at_path(&path);
    }
    clear_gateway_request_logs_db()
}

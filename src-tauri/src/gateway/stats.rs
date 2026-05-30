use super::*;

/// 从 DB 全量统计（口径与 get_gateway_request_stats_from_path 一致）。
pub(crate) fn get_gateway_request_stats_from_db() -> Result<GatewayRequestStats, String> {
    let conn = crate::db::pool().get().map_err(|e| format!("获取数据库连接失败: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT data FROM gateway_request_log")
        .map_err(|e| format!("查询请求日志失败: {e}"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("读取请求日志失败: {e}"))?;
    let mut s = GatewayRequestStats {
        total: 0,
        success: 0,
        error: 0,
        streaming: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cache_read_tokens: 0,
        total_cache_creation_tokens: 0,
        requests_with_cache: 0,
        max_duration_ms: 0,
        avg_duration_ms: 0,
    };
    let mut total_duration_ms = 0u64;
    for data in rows.flatten() {
        let Ok(e) = serde_json::from_str::<GatewayRequestLogEntry>(&data) else {
            continue;
        };
        s.total += 1;
        if e.status_code < 400 {
            s.success += 1;
        } else {
            s.error += 1;
        }
        if e.stream {
            s.streaming += 1;
        }
        s.total_input_tokens += e.input_tokens.unwrap_or(0) as i64;
        s.total_output_tokens += e.output_tokens.unwrap_or(0) as i64;
        s.total_cache_read_tokens += e.cache_read_input_tokens.unwrap_or(0) as i64;
        s.total_cache_creation_tokens += e.cache_creation_input_tokens.unwrap_or(0) as i64;
        if e.cache_read_input_tokens.unwrap_or(0) > 0 || e.cache_creation_input_tokens.unwrap_or(0) > 0
        {
            s.requests_with_cache += 1;
        }
        s.max_duration_ms = s.max_duration_ms.max(e.duration_ms);
        total_duration_ms += e.duration_ms;
    }
    if s.total > 0 {
        s.avg_duration_ms = total_duration_ms / s.total as u64;
    }
    Ok(s)
}

pub async fn get_gateway_request_stats(
    state: &tauri::State<'_, crate::state::AppState>,
) -> Result<GatewayRequestStats, String> {
    // 尝试从运行中的 gateway 的内存存储获取
    let log_store_opt = {
        let guard = state
            .gateway
            .lock()
            .map_err(|_| "获取 gateway 状态失败".to_string())?;

        guard.as_ref().map(|rt| rt.log_store.clone())
    };

    if let Some(log_store) = log_store_opt {
        // 从内存存储获取统计
        let stats = log_store.get_stats().await;
        let all_logs = log_store.get_all().await;

        // 计算最大延迟
        let max_duration_ms = all_logs.iter()
            .map(|log| log.duration_ms)
            .max()
            .unwrap_or(0);

        return Ok(GatewayRequestStats {
            total: stats.total,
            success: stats.success,
            error: stats.error,
            streaming: stats.streaming,
            total_input_tokens: stats.total_input_tokens as i64,
            total_output_tokens: stats.total_output_tokens as i64,
            total_cache_read_tokens: stats.total_cache_read_tokens as i64,
            total_cache_creation_tokens: stats.total_cache_creation_tokens as i64,
            requests_with_cache: stats.requests_with_cache,
            max_duration_ms,
            avg_duration_ms: stats.avg_duration_ms,
        });
    }

    // gateway 未运行：从 SQLite 统计
    #[cfg(test)]
    if let Some(path) = request_log_path_override() {
        return get_gateway_request_stats_from_path(&path);
    }
    get_gateway_request_stats_from_db()
}

pub async fn get_gateway_model_stats(
    state: &tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<log_store::ModelStat>, String> {
    let log_store_opt = {
        let guard = state
            .gateway
            .lock()
            .map_err(|_| "获取 gateway 状态失败".to_string())?;

        guard.as_ref().map(|rt| rt.log_store.clone())
    };

    if let Some(log_store) = log_store_opt {
        return Ok(log_store.get_model_stats().await);
    }

    Ok(Vec::new())
}

pub async fn get_gateway_endpoint_stats(
    state: &tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<log_store::EndpointStat>, String> {
    let log_store_opt = {
        let guard = state
            .gateway
            .lock()
            .map_err(|_| "获取 gateway 状态失败".to_string())?;

        guard.as_ref().map(|rt| rt.log_store.clone())
    };

    if let Some(log_store) = log_store_opt {
        return Ok(log_store.get_endpoint_stats().await);
    }

    Ok(Vec::new())
}

#[cfg(test)]
pub(crate) fn get_gateway_request_stats_from_path(path: &Path) -> Result<GatewayRequestStats, String> {
    if !path.exists() {
        return Ok(GatewayRequestStats {
            total: 0,
            success: 0,
            error: 0,
            streaming: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_read_tokens: 0,
            total_cache_creation_tokens: 0,
            requests_with_cache: 0,
            max_duration_ms: 0,
            avg_duration_ms: 0,
        });
    }

    let file = fs::File::open(path).map_err(|e| format!("读取请求日志失败: {e}"))?;
    let reader = BufReader::new(file);

    let mut total = 0;
    let mut success = 0;
    let mut error = 0;
    let mut streaming = 0;
    let mut total_input_tokens: i64 = 0;
    let mut total_output_tokens: i64 = 0;
    let mut total_cache_read_tokens: i64 = 0;
    let mut total_cache_creation_tokens: i64 = 0;
    let mut requests_with_cache = 0;
    let mut max_duration_ms = 0u64;
    let mut total_duration_ms = 0u64;

    for line in reader.lines() {
        let Ok(line) = line else {
            continue;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<GatewayRequestLogEntry>(trimmed) {
            total += 1;

            if entry.status_code < 400 {
                success += 1;
            } else {
                error += 1;
            }

            if entry.stream {
                streaming += 1;
            }

            total_input_tokens += entry.input_tokens.unwrap_or(0) as i64;
            total_output_tokens += entry.output_tokens.unwrap_or(0) as i64;
            total_cache_read_tokens += entry.cache_read_input_tokens.unwrap_or(0) as i64;
            total_cache_creation_tokens += entry.cache_creation_input_tokens.unwrap_or(0) as i64;

            if entry.cache_read_input_tokens.unwrap_or(0) > 0
                || entry.cache_creation_input_tokens.unwrap_or(0) > 0 {
                requests_with_cache += 1;
            }

            max_duration_ms = max_duration_ms.max(entry.duration_ms);
            total_duration_ms += entry.duration_ms;
        }
    }

    let avg_duration_ms = if total > 0 {
        total_duration_ms / total as u64
    } else {
        0
    };

    Ok(GatewayRequestStats {
        total,
        success,
        error,
        streaming,
        total_input_tokens,
        total_output_tokens,
        total_cache_read_tokens,
        total_cache_creation_tokens,
        requests_with_cache,
        max_duration_ms,
        avg_duration_ms,
    })
}

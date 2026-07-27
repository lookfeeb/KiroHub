// 远程 MCP OAuth 令牌后台自动刷新任务
// 每 60s 扫描凭证，对 10 分钟内过期者用 refresh_token 续期，处理 refresh_token 轮换并持久化

use crate::commands::app_settings_cmd::{get_mcp_oauth_store, mcp_oauth_failure_needs_reauth};
use crate::mcp_oauth::refresh_stored_credential;
use tauri::{AppHandle, Emitter};
use tokio::time::{interval, Duration};

const LOOP_INTERVAL_SECONDS: u64 = 60;
const REFRESH_THRESHOLD_SECONDS: i64 = 600; // 提前 10 分钟刷新

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn refresh_due(app_handle: &AppHandle) -> Result<(), String> {
    let store = match get_mcp_oauth_store() {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let now = now_secs();
    let mut changed = false;

    for (key, cred) in &store.creds_by_key {
        if store
            .refresh_failures
            .get(key)
            .is_some_and(|message| mcp_oauth_failure_needs_reauth(message))
        {
            continue;
        }

        if cred.expires_at == 0 || cred.expires_at - now >= REFRESH_THRESHOLD_SECONDS {
            continue;
        }
        if cred.refresh_token.is_none() {
            continue;
        }
        match refresh_stored_credential(key, Some(cred)).await {
            Ok(_updated) => {
                changed = true;
                log::info!("MCP token refreshed: {key}");
            }
            Err(error) => {
                let msg = error.to_string();
                changed = true;
                if !mcp_oauth_failure_needs_reauth(&msg) {
                    log::error!("MCP token refresh failed [{key}]: {msg}");
                }
            }
        }
    }

    if changed {
        let _ = app_handle.emit("mcp-tokens-updated", ());
    }
    Ok(())
}

/// 启动 MCP 令牌刷新循环（供 main.rs 调用）
pub fn start_mcp_token_refresh_loop(app_handle: AppHandle) {
    log::info!("Starting MCP token refresh background task");
    tauri::async_runtime::spawn(async move {
        let mut timer = interval(Duration::from_secs(LOOP_INTERVAL_SECONDS));
        loop {
            timer.tick().await;
            if let Err(e) = refresh_due(&app_handle).await {
                log::error!("MCP token refresh loop error: {e}");
            }
        }
    });
}

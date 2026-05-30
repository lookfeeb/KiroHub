// 远程 MCP OAuth 令牌后台自动刷新任务
// 每 60s 扫描凭证，对 10 分钟内过期者用 refresh_token 续期，处理 refresh_token 轮换并持久化

use crate::commands::app_settings_cmd::{get_mcp_oauth_store, upsert_mcp_oauth_cred};
use crate::mcp_oauth::refresh_access_token;
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

    for (key, cred) in &store.creds {
        if cred.expires_at == 0 || cred.expires_at - now >= REFRESH_THRESHOLD_SECONDS {
            continue;
        }
        let Some(refresh_token) = cred.refresh_token.as_deref() else {
            continue;
        };
        match refresh_access_token(
            &cred.token_endpoint,
            &cred.client_id,
            refresh_token,
            &cred.resource,
        )
        .await
        {
            Ok((access_token, new_refresh, expires_at)) => {
                let mut updated = cred.clone();
                updated.access_token = access_token;
                updated.expires_at = expires_at;
                // refresh_token 轮换：服务端可能返回新的，否则沿用旧的
                if let Some(rt) = new_refresh {
                    updated.refresh_token = Some(rt);
                }
                upsert_mcp_oauth_cred(key, updated)?;
                changed = true;
                log::info!("MCP token refreshed: {key}");
            }
            Err(e) => log::error!("MCP token refresh failed [{key}]: {e}"),
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

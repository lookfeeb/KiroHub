// Token 自动刷新后台任务
// 参考 Kiro IDE 源码实现

use crate::commands::common::{
    apply_refreshed_account_tokens, get_usage_by_provider, is_auth_error_message, is_token_expired,
    refresh_token_by_provider, token_needs_refresh, update_account_status,
    REFRESH_LOOP_INTERVAL_SECONDS,
};
use crate::state::AppState;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::{interval, Duration};

/// Token 刷新服务
pub struct TokenRefreshService {
    app_handle: AppHandle,
}

impl TokenRefreshService {
    /// 创建新的 Token 刷新服务
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }

    /// 启动后台刷新循环
    pub fn start(self) {
        tauri::async_runtime::spawn(async move {
            let mut interval_timer = interval(Duration::from_secs(REFRESH_LOOP_INTERVAL_SECONDS));

            loop {
                interval_timer.tick().await;

                if let Err(e) = self.refresh_expiring_tokens().await {
                    log::error!("Token refresh loop error: {}", e);
                }

                if let Err(e) = self.refresh_cli_token_if_expiring().await {
                    log::error!("CLI token refresh loop error: {}", e);
                }

                if let Err(e) = self.refresh_reset_due_quotas().await {
                    log::error!("Reset-due quota refresh error: {}", e);
                }

                if let Err(e) = self.refresh_active_quotas().await {
                    log::error!("Active quota check error: {}", e);
                }
            }
        });
    }

    /// 检查并刷新即将过期的 token
    async fn refresh_expiring_tokens(&self) -> Result<(), String> {
        // 读取所有账号
        let accounts = {
            let state = self.app_handle.state::<AppState>();
            let mut store = state
                .store
                .lock()
                .map_err(|_| "Failed to acquire account store lock".to_string())?;
            store.reload();
            store.accounts.clone()
        };

        // 本轮是否有账号被刷新或失效，用于决定是否通知前端
        let mut any_changed = false;

        for account in accounts {
            // 跳过已禁用或无效的账号
            if account.status == "invalid" || account.status == "banned" {
                continue;
            }

            // 检查是否需要刷新（即将过期或已过期）
            if let Some(ref expires_at) = account.expires_at {
                if token_needs_refresh(expires_at) {
                    let email_display = account
                        .email
                        .as_deref()
                        .or(account.user_id.as_deref())
                        .unwrap_or("Unknown");

                    log::info!(
                        "Token refresh loop: token expiring soon for account {} ({}), attempting refresh",
                        email_display,
                        account.provider.as_deref().unwrap_or("Unknown")
                    );

                    // 尝试刷新
                    match refresh_token_by_provider(&account).await {
                        Ok(refresh_result) => {
                            // 更新账号信息
                            let state = self.app_handle.state::<AppState>();
                            let mut store = state
                                .store
                                .lock()
                                .map_err(|_| "Failed to acquire account store lock".to_string())?;
                            store.reload();
                            if let Some(acc) = store.accounts.iter_mut().find(|a| a.id == account.id)
                            {
                                if acc.refresh_token.as_deref() != account.refresh_token.as_deref() {
                                    log::info!(
                                        "Token refresh loop: skipped stale refresh result for {}",
                                        email_display
                                    );
                                    continue;
                                }

                                // 先提取 email_display，避免借用冲突
                                let email_display = acc
                                    .email
                                    .as_deref()
                                    .or(acc.user_id.as_deref())
                                    .unwrap_or("Unknown")
                                    .to_string();

                                // 应用刷新结果到 Account（统一的字段更新策略）
                                apply_refreshed_account_tokens(acc, &refresh_result);

                                // 保存到文件
                                if let Err(e) = store.try_save_to_file() {
                                    log::error!("Failed to save account after refresh: {}", e);
                                } else {
                                    any_changed = true;
                                    log::info!(
                                        "Token refresh loop: refresh completed successfully for {}",
                                        email_display
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            log::error!(
                                "Token refresh loop: refresh failed for {}: {}",
                                email_display,
                                e
                            );

                            // 如果是认证错误且 token 已过期，标记为 invalid
                            if is_auth_error_message(&e) && is_token_expired(expires_at) {
                                let state = self.app_handle.state::<AppState>();
                                let mut store = state
                                    .store
                                    .lock()
                                    .map_err(|_| "Failed to acquire account store lock".to_string())?;
                                store.reload();
                                if let Some(acc) = store.accounts.iter_mut().find(|a| a.id == account.id)
                                {
                                    if acc.refresh_token.as_deref() != account.refresh_token.as_deref() {
                                        log::info!(
                                            "Token refresh loop: skipped stale auth failure for {}",
                                            email_display
                                        );
                                        continue;
                                    }

                                    acc.status = "invalid".to_string();
                                    acc.enabled = false;
                                    let _ = store.try_save_to_file();
                                    any_changed = true;
                                    log::warn!(
                                        "Token refresh loop: marked account {} as invalid",
                                        email_display
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // 本轮有任何账号被刷新/失效，通知前端重新加载列表
        if any_changed {
            let _ = self.app_handle.emit("accounts-updated", ());
        }

        Ok(())
    }

    /// CLI token 过期前 3 分钟自动刷新，避免首页显示“已过期”后才处理。
    async fn refresh_cli_token_if_expiring(&self) -> Result<(), String> {
        match crate::commands::kiro_cli_cmd::refresh_default_cli_db_token_if_expiring().await {
            Ok(true) => {
                log::info!("Kiro CLI token auto-refreshed before expiry");
                let _ = self.app_handle.emit("cli-token-updated", ());
            }
            Ok(false) => {}
            Err(e) => log::warn!("Kiro CLI token auto-refresh skipped/failed: {e}"),
        }
        Ok(())
    }

    /// 重置时间到期的账号：刷新配额+token，有配额则自动启用
    async fn refresh_reset_due_quotas(&self) -> Result<(), String> {
        let accounts = {
            let state = self.app_handle.state::<AppState>();
            let mut store = state
                .store
                .lock()
                .map_err(|_| "Failed to acquire account store lock".to_string())?;
            store.reload();
            store.accounts.clone()
        };

        let now_ts = chrono::Utc::now().timestamp();
        let mut any_changed = false;

        for account in accounts {
            if account.status == "invalid" || account.status == "banned" {
                continue;
            }

            // 仅处理重置时间已到的账号（nextDateReset 为秒级时间戳）
            let reset_due = account
                .usage_data
                .as_ref()
                .and_then(|d| d.get("nextDateReset"))
                .and_then(serde_json::Value::as_i64)
                .map(|ts| now_ts >= ts)
                .unwrap_or(false);
            if !reset_due {
                continue;
            }

            let Some(provider) = account.provider.clone() else {
                continue;
            };

            // 先刷新 token，确保用有效的 access_token 拉配额
            let access_token = match refresh_token_by_provider(&account).await {
                Ok(refresh_result) => {
                    let token = refresh_result.access_token.clone();
                    let state = self.app_handle.state::<AppState>();
                    let mut store = state
                        .store
                        .lock()
                        .map_err(|_| "Failed to acquire account store lock".to_string())?;
                    store.reload();
                    if let Some(acc) = store.accounts.iter_mut().find(|a| a.id == account.id) {
                        apply_refreshed_account_tokens(acc, &refresh_result);
                        let _ = store.try_save_to_file();
                    }
                    token
                }
                Err(_) => match account.access_token.clone() {
                    Some(t) => t,
                    None => continue,
                },
            };

            // 拉取最新配额，更新状态并按配额自动启用/禁用
            if let Ok(usage) = get_usage_by_provider(&provider, &access_token).await {
                let state = self.app_handle.state::<AppState>();
                let mut store = state
                    .store
                    .lock()
                    .map_err(|_| "Failed to acquire account store lock".to_string())?;
                store.reload();
                if let Some(acc) = store.accounts.iter_mut().find(|a| a.id == account.id) {
                    acc.usage_data = Some(usage.usage_data);
                    // update_account_status 会在 capped/banned/invalid 时自动禁用
                    update_account_status(acc, usage.is_banned, usage.is_auth_error);
                    // 重置后若恢复可用（正常/超额）且当前被禁用，则自动启用
                    if matches!(acc.status.as_str(), "active" | "overage") && !acc.enabled {
                        acc.enabled = true;
                    }
                    let _ = store.try_save_to_file();
                    any_changed = true;
                }
            }
        }

        if any_changed {
            let _ = self.app_handle.emit("accounts-updated", ());
        }

        Ok(())
    }

    /// 配额巡检：对启用中的账号定期拉取配额并重算状态，
    /// 配额耗尽（capped）时自动禁用。每账号最多每 10 分钟拉一次，避免限流。
    async fn refresh_active_quotas(&self) -> Result<(), String> {
        const QUOTA_CHECK_INTERVAL_SECONDS: i64 = 600;

        let accounts = {
            let state = self.app_handle.state::<AppState>();
            let mut store = state
                .store
                .lock()
                .map_err(|_| "Failed to acquire account store lock".to_string())?;
            store.reload();
            store.accounts.clone()
        };

        let now_ts = chrono::Utc::now().timestamp();
        let mut any_changed = false;

        for account in accounts {
            // 只巡检启用中、状态有效的账号；invalid/banned 跳过
            if !account.enabled || account.status == "invalid" || account.status == "banned" {
                continue;
            }

            // 已到重置时间的交给 refresh_reset_due_quotas 处理，避免重复拉取
            let reset_due = account
                .usage_data
                .as_ref()
                .and_then(|d| d.get("nextDateReset"))
                .and_then(serde_json::Value::as_i64)
                .map(|ts| now_ts >= ts)
                .unwrap_or(false);
            if reset_due {
                continue;
            }

            // 节流：距上次巡检不足 10 分钟则跳过
            let throttled = account
                .last_quota_check_at
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|t| now_ts - t.timestamp() < QUOTA_CHECK_INTERVAL_SECONDS)
                .unwrap_or(false);
            if throttled {
                continue;
            }

            let (Some(provider), Some(access_token)) =
                (account.provider.clone(), account.access_token.clone())
            else {
                continue;
            };

            // 用现有有效 token 拉配额（token 临近过期由 refresh_expiring_tokens 负责）
            if let Ok(usage) = get_usage_by_provider(&provider, &access_token).await {
                let state = self.app_handle.state::<AppState>();
                let mut store = state
                    .store
                    .lock()
                    .map_err(|_| "Failed to acquire account store lock".to_string())?;
                store.reload();
                if let Some(acc) = store.accounts.iter_mut().find(|a| a.id == account.id) {
                    acc.usage_data = Some(usage.usage_data);
                    // update_account_status 会在 capped/banned/invalid 时自动禁用
                    update_account_status(acc, usage.is_banned, usage.is_auth_error);
                    acc.last_quota_check_at = Some(chrono::Utc::now().to_rfc3339());
                    let _ = store.try_save_to_file();
                    any_changed = true;
                }
            }
        }

        if any_changed {
            let _ = self.app_handle.emit("accounts-updated", ());
        }

        Ok(())
    }
}

/// 启动 Token 刷新循环（供 main.rs 调用）
pub fn start_token_refresh_loop(app_handle: AppHandle) {
    log::info!("Starting token refresh background task");
    let service = TokenRefreshService::new(app_handle);
    service.start();
}

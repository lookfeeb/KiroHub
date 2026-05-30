use super::*;

/// 计算剩余秒数
pub(crate) fn calculate_expires_in_seconds(expires_at: &str) -> Result<i64, String> {
    let expires = chrono::NaiveDateTime::parse_from_str(expires_at, "%Y/%m/%d %H:%M:%S")
        .map_err(|e| format!("Failed to parse expires_at: {}", e))?;
    let now = chrono::Local::now().naive_local();
    let remaining = expires.signed_duration_since(now);
    Ok(remaining.num_seconds())
}

/// 检查单个账号的 Token 状态
#[tauri::command]
pub fn check_token_status(
    state: State<AppState>,
    id: String,
) -> Result<CheckTokenStatusResponse, String> {
    let store = lock_store(&state.store, "store")?;
    let account = store
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or("Account not found")?;

    let expires_at = account
        .expires_at
        .as_ref()
        .ok_or("No expiration time")?;

    let status = if is_token_expired(expires_at) {
        "expired"
    } else if is_token_expiring_soon(expires_at) {
        "expiring_soon"
    } else if account.status == "invalid" {
        "invalid"
    } else {
        "active"
    };

    let expires_in_seconds = calculate_expires_in_seconds(expires_at)?;

    Ok(CheckTokenStatusResponse {
        status: status.to_string(),
        expires_at: expires_at.clone(),
        expires_in_seconds,
        needs_refresh: is_token_expiring_soon(expires_at),
    })
}

/// 批量检查所有账号的 Token 状态
#[tauri::command]
pub fn check_all_tokens_status(state: State<AppState>) -> Result<TokenStatusSummary, String> {
    let store = lock_store(&state.store, "store")?;
    let accounts = &store.accounts;

    let mut summary = TokenStatusSummary {
        total: accounts.len(),
        active: 0,
        expiring_soon: 0,
        expired: 0,
        invalid: 0,
        accounts_need_refresh: Vec::new(),
    };

    for account in accounts {
        if let Some(ref expires_at) = account.expires_at {
            if is_token_expired(expires_at) {
                summary.expired += 1;
            } else if is_token_expiring_soon(expires_at) {
                summary.expiring_soon += 1;
                if let Ok(expires_in_seconds) = calculate_expires_in_seconds(expires_at) {
                    summary.accounts_need_refresh.push(AccountRefreshInfo {
                        id: account.id.clone(),
                        email: account.email.clone(),
                        provider: account.provider.clone().unwrap_or_default(),
                        expires_at: expires_at.clone(),
                        expires_in_seconds,
                    });
                }
            } else if account.status == "invalid" {
                summary.invalid += 1;
            } else {
                summary.active += 1;
            }
        } else {
            summary.invalid += 1;
        }
    }

    Ok(summary)
}

/// 批量刷新即将过期的 Token
#[tauri::command]
pub async fn refresh_all_expiring_tokens(
    state: State<'_, AppState>,
    only_expiring: Option<bool>,
    max_concurrent: Option<usize>,
) -> Result<RefreshAllResponse, String> {
    use tokio::sync::Semaphore;
    use std::sync::Arc;

    let only_expiring = only_expiring.unwrap_or(true);
    let max_concurrent = max_concurrent.unwrap_or(3);

    // 获取需要刷新的账号列表
    let accounts_to_refresh: Vec<Account> = {
        let store = lock_store(&state.store, "store")?;
        store
            .accounts
            .iter()
            .filter(|acc| {
                if only_expiring {
                    acc.expires_at
                        .as_ref()
                        .map(|exp| is_token_expiring_soon(exp))
                        .unwrap_or(false)
                } else {
                    acc.refresh_token.is_some()
                }
            })
            .cloned()
            .collect()
    };

    let total = accounts_to_refresh.len();
    let mut results = Vec::new();
    let mut successful = 0;
    let mut failed = 0;

    // 使用 Semaphore 控制并发
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut tasks = Vec::new();

    for account in accounts_to_refresh {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let account_clone = account.clone();

        let task = tokio::spawn(async move {
            let result = refresh_token_by_provider(&account_clone).await;
            drop(permit);
            (account_clone, result)
        });

        tasks.push(task);
    }

    // 等待所有任务完成
    for task in tasks {
        let (account, result) = task.await.unwrap();
        let email_display = account
            .email
            .as_deref()
            .or(account.user_id.as_deref())
            .unwrap_or("Unknown")
            .to_string();

        log::debug!("Token refresh result for {}: {:?}", email_display, result);

        match result {
            Ok(refresh_result) => {
                // 更新数据库
                let mut store = lock_store(&state.store, "store")?;
                if let Some(acc) = store.accounts.iter_mut().find(|a| a.id == account.id) {
                    acc.access_token = Some(refresh_result.access_token);
                    acc.refresh_token = refresh_result.refresh_token;
                    acc.expires_at = Some(calc_expires_at(refresh_result.expires_in));

                    // IdC 账号更新额外字段
                    if let Some(id_token) = refresh_result.id_token {
                        acc.id_token = Some(id_token);
                    }
                    if let Some(sso_session_id) = refresh_result.sso_session_id {
                        acc.sso_session_id = Some(sso_session_id);
                    }
                    // Social 账号更新 profile_arn
                    if let Some(profile_arn) = refresh_result.profile_arn {
                        acc.profile_arn = Some(profile_arn);
                    }

                    save_store(&store)?;
                }

                log::info!("Batch refresh: successfully refreshed token for {}", email_display);
                successful += 1;
                results.push(RefreshResultItem {
                    id: account.id,
                    email: account.email,
                    success: true,
                    message: "Refreshed successfully".to_string(),
                });
            }
            Err(e) => {
                log::error!("Batch refresh: failed to refresh token for {}: {}", email_display, e);
                failed += 1;
                results.push(RefreshResultItem {
                    id: account.id,
                    email: account.email,
                    success: false,
                    message: e,
                });
            }
        }
    }

    Ok(RefreshAllResponse {
        total_attempted: total,
        successful,
        failed,
        skipped: 0,
        results,
    })
}

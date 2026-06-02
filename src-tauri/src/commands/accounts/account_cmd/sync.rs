use super::*;

#[tauri::command]
pub async fn sync_account(
    state: State<'_, AppState>,
    id: String,
) -> Result<SyncAccountResult, String> {
    let account = find_account_by_id(&state, &id)?;

    let provider_str = account.provider.as_deref().unwrap_or("Google");
    let access_token = account.access_token.as_ref().ok_or("No access token")?;
    let is_enterprise = provider_str == "Enterprise";

    // 如果账号缺少 machine_id，自动生成一个（所有账号都需要）
    let mut account = account.clone();
    if account.machine_id.is_none() {
        use crate::commands::machine_guid::get_machine_id;
        let machine_id = get_machine_id();
        account.machine_id = Some(machine_id);
        log::info!("Generated machine_id for account: {}", account.id);
    }

    // 先尝试用现有 token 获取配额
    let mut usage_result = if is_enterprise {
        let machine_id = account
            .machine_id
            .as_ref()
            .ok_or("Enterprise account missing machine_id")?;
        get_enterprise_usage_with_region_probe(access_token, machine_id)
            .await
            .map(|(result, _region)| result)
    } else {
        get_usage_by_provider(provider_str, access_token).await
    };

    let mut refresh_result: Option<RefreshResult> = None;
    let mut detected_region: Option<String> = None;

    // 如果是认证错误，刷新 token 后重试
    let needs_refresh = match &usage_result {
        Ok(r) => r.is_auth_error,
        Err(_) => false,
    };

    if needs_refresh {
        match refresh_token_by_provider(&account).await {
            Ok(refreshed) => {
                usage_result = if is_enterprise {
                    let machine_id = account
                        .machine_id
                        .as_ref()
                        .ok_or("Enterprise account missing machine_id")?;
                    match get_enterprise_usage_with_region_probe(&refreshed.access_token, machine_id).await {
                        Ok((result, region)) => {
                            detected_region = Some(region);
                            Ok(result)
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    get_usage_by_provider(provider_str, &refreshed.access_token).await
                };
                refresh_result = Some(refreshed);
            }
            Err(e) => {
                if e.starts_with("BANNED:") || is_auth_error_message(&e) {
                    let mut store = lock_store(&state.store, "store")?;
                    if let Some(a) = store.accounts.iter_mut().find(|a| a.id == id) {
                        a.status = if e.starts_with("BANNED:") {
                            "banned".to_string()
                        } else {
                            "invalid".to_string()
                        };
                        a.enabled = false;
                        save_store(&store)?;
                    }
                }
                return Err(e);
            }
        }
    }

    // 获取配额失败时容错处理：只更新 token，不更新 usageData
    let (usage, warning) = match usage_result {
        Ok(u) => (Some(u), None),
        Err(e) => {
            // 获取配额失败，不打印日志，直接返回错误信息
            (None, Some(format!("获取配额失败: {e}")))
        }
    };

    let mut store = lock_store(&state.store, "store")?;
    let result = if let Some(a) = store.accounts.iter_mut().find(|a| a.id == id) {
        // 如果生成了新的 machine_id，保存它（所有账号都需要）
        // 如果生成了新的 machine_id，保存它（所有账号都需要）
        if account.machine_id.is_some() && a.machine_id.is_none() {
            a.machine_id = account.machine_id.clone();
            log::info!("Saved machine_id for account: {}", a.id);
        }

        // 如果刷新了 token，更新 token 相关字段
        if let Some(ref result) = refresh_result {
            clear_available_models_cache(a);

            let email_display = a
                .email
                .as_deref()
                .or(a.user_id.as_deref())
                .unwrap_or("Unknown");

            // 刷新 Token 成功，更新账号信息
            a.access_token = Some(result.access_token.clone());
            if let Some(ref refresh_token) = result.refresh_token {
                a.refresh_token = Some(refresh_token.clone());
            }
            a.profile_arn = result.profile_arn.clone();
            a.id_token = result.id_token.clone();
            a.sso_session_id = result.sso_session_id.clone();
            a.expires_at = Some(calc_expires_at(result.expires_in));

            log::info!("Token refreshed successfully for account: {}", email_display);
        }
        // 如果探测到了新的区域，更新账户的 region 字段
        if let Some(region) = detected_region {
            a.region = Some(region);
        }

        // 只有成功获取配额时才更新 usage_data 和 status
        if let Some(usage_data) = usage {
            // 直接移动所有权，避免 clone
            a.usage_data = Some(usage_data.usage_data);
            update_account_status(a, usage_data.is_banned, usage_data.is_auth_error);

            // 从 usage_data 中提取并更新 email 和 user_id
            if let Some(user_info) = a.usage_data.as_ref().and_then(|d| d.get("userInfo")) {
                if let Some(email) = user_info.get("email").and_then(|v| v.as_str()) {
                    if !email.is_empty() {
                        a.email = Some(email.to_string());
                    }
                }
                if let Some(user_id) = user_info.get("userId").and_then(|v| v.as_str()) {
                    a.user_id = Some(user_id.to_string());
                }
            }
        } else if refresh_result.is_some() {
            // 获取配额失败，但 token 刷新成功了，说明 token 是有效的
            // 将状态设置为 active（避免显示为失效状态）
            if !matches!(a.status.as_str(), "banned" | "封禁" | "已封禁") {
                a.status = "active".to_string();
            }
        }

        // 克隆结果（这个必须 clone，因为要返回给前端）
        Some(a.clone())
    } else {
        None
    };

    // 保存文件
    save_store(&store)?;

    match result {
        Some(account) => Ok(SyncAccountResult { account, warning }),
        None => Err("Account not found after update".to_string()),
    }
}

/// 只刷新 token，不获取 usage（启动时快速刷新用）
/// 如果 token 还有 5 分钟以上有效期，跳过刷新直接返回
#[tauri::command]
pub async fn refresh_account_token(
    state: State<'_, AppState>,
    id: String,
) -> Result<Account, String> {
    let account = find_account_by_id(&state, &id)?;

    // 检查 token 是否还有 5 分钟以上有效期
    if let Some(expires_at) = &account.expires_at {
        if let Ok(exp) = chrono::NaiveDateTime::parse_from_str(expires_at, "%Y/%m/%d %H:%M:%S") {
            let now = chrono::Local::now().naive_local();
            let remaining = exp.signed_duration_since(now);
            if remaining.num_minutes() >= 5 {
                return Ok(account);
            }
        }
    }

    let refresh_result = match refresh_token_by_provider(&account).await {
        Ok(result) => result,
        Err(e) => {
            if e.starts_with("BANNED:") || is_auth_error_message(&e) {
                let mut store = lock_store(&state.store, "store")?;
                if let Some(a) = store.accounts.iter_mut().find(|a| a.id == id) {
                    a.status = if e.starts_with("BANNED:") {
                        "banned".to_string()
                    } else {
                        "invalid".to_string()
                    };
                    a.enabled = false;
                    save_store(&store)?;
                }
            }
            return Err(e);
        }
    };

    let mut store = lock_store(&state.store, "store")?;
    if let Some(a) = store.accounts.iter_mut().find(|a| a.id == id) {
        clear_available_models_cache(a);
        // 直接移动所有权，避免 clone
        a.access_token = Some(refresh_result.access_token);
        a.refresh_token = refresh_result.refresh_token;
        a.expires_at = Some(calc_expires_at(refresh_result.expires_in));
        if matches!(
            a.status.as_str(),
            "invalid" | "失效" | "已失效" | "Token已失效"
        ) {
            a.status = "active".to_string();
        }
        let result = a.clone();
        save_store(&store)?;
        return Ok(result);
    }
    Err("Account not found after update".to_string())
}

/// 从 AWS 服务端删除账号（注销账号）
/// 仅支持 Google、Github，不支持 `BuilderId` 和 `Enterprise`
#[tauri::command]
pub async fn delete_account_remote(
    state: State<'_, AppState>,
    id: String,
    delete_local: bool,
) -> Result<String, String> {
    use crate::auth::delete_account_desktop;
    use crate::commands::machine_guid::get_machine_id;

    // 获取账号信息
    let account = find_account_by_id(&state, &id)?;

    // 检查 provider
    let provider = account.provider.as_deref().unwrap_or("Google");
    if provider == "Enterprise" {
        return Err("Enterprise 账号不支持远程删除".to_string());
    }
    if provider == "BuilderId" {
        return Err("BuilderId 账号不支持远程删除".to_string());
    }

    let access_token = account
        .access_token
        .as_ref()
        .ok_or("账号缺少 access_token，请先刷新")?;

    // Google/Github 账号使用 Desktop API
    let machine_id = get_machine_id();
    delete_account_desktop(access_token, &machine_id).await?;

    // 如果需要同时删除本地记录
    if delete_local {
        let mut store = lock_store(&state.store, "store")?;
        store.delete(&id)?;
    }

    Ok(format!("账号 {} 已从服务端删除", account.get_display_id()))
}

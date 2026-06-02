use super::*;

/// 获取账号配额信息（不刷新 token，不更新数据库）
#[tauri::command]
pub async fn get_account_usage(
    access_token: String,
    provider: Option<String>,
) -> Result<serde_json::Value, String> {
    let provider_str = provider.as_deref().unwrap_or("Google");
    let usage_result = get_usage_by_provider(provider_str, &access_token).await?;
    Ok(usage_result.usage_data)
}

#[tauri::command]
pub async fn list_available_models(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
    model_provider: Option<String>,
    force_refresh: Option<bool>,
) -> Result<ListAvailableModelsResponse, String> {
    let mut account = find_account_by_id(&state, &id)?;

    if let Some(cached_response) = read_available_models_cache(
        &account,
        model_provider.as_deref(),
        force_refresh.unwrap_or(false),
    ) {
        return Ok(cached_response);
    }

    let initial_access_token = account
        .access_token
        .clone()
        .ok_or("账号缺少 access_token，请先刷新 Token")?;

    match fetch_all_available_models(&account, &initial_access_token, model_provider.as_deref())
        .await
    {
        Ok(response) => {
            let mut store = lock_store(&state.store, "store")?;
            if let Some(stored_account) = store.accounts.iter_mut().find(|item| item.id == id) {
                write_available_models_cache(stored_account, model_provider.as_deref(), &response)?;
                save_store(&store)?;
            }
            Ok(response)
        }
        Err(error) if is_auth_error_message(&error) => {
            let refresh = refresh_token_by_provider(&account).await?;
            apply_refreshed_account_tokens(&mut account, &refresh);

            {
                let mut store = lock_store(&state.store, "store")?;
                let stored_account = store
                    .accounts
                    .iter_mut()
                    .find(|item| item.id == id)
                    .ok_or("账号不存在")?;
                apply_refreshed_account_tokens(stored_account, &refresh);
                save_store(&store)?;
            }
            let response = fetch_all_available_models(
                &account,
                &refresh.access_token,
                model_provider.as_deref(),
            )
            .await?;
            {
                let mut store = lock_store(&state.store, "store")?;
                if let Some(stored_account) = store.accounts.iter_mut().find(|item| item.id == id) {
                    write_available_models_cache(
                        stored_account,
                        model_provider.as_deref(),
                        &response,
                    )?;
                    save_store(&store)?;
                }
            }
            Ok(response)
        }
        Err(error) if error.starts_with("BANNED:") => {
            // 更新账号状态为封禁
            let mut store = lock_store(&state.store, "store")?;
            if let Some(stored_account) = store.accounts.iter_mut().find(|item| item.id == id) {
                stored_account.status = "banned".to_string();
                stored_account.enabled = false;
                save_store(&store)?;
                // 通知前端刷新账号列表
                let _ = app.emit("accounts-updated", ());
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
}

/// 设置超额开关状态
#[tauri::command]
pub async fn set_overage_status(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    use crate::clients::kiro_q_client::KiroQClient;
    use crate::core::usage::OverageCapability;

    // 1. 从 state 获取账号
    let account = find_account_by_id(&state, &id)?;

    // 资格预检：必须是 OVERAGE_CAPABLE 才能开关超额
    match OverageCapability::from_usage_data(account.usage_data.as_ref()) {
        OverageCapability::Capable => {}
        OverageCapability::Incapable => {
            return Err("此账号订阅级别不支持超额（仅 Pro / Pro+ 可开启）".to_string());
        }
        OverageCapability::Unknown => {
            return Err("无法判断账号超额资格，请先点击「检测」刷新账号信息".to_string());
        }
    }

    let access_token = account.access_token.as_ref().ok_or("No access token")?.clone();

    // 2. 检查 token 是否过期，如果过期则刷新
    let final_access_token = if let Some(expires_at) = &account.expires_at {
        if token_needs_refresh(expires_at) {
            let refresh_result = refresh_token_by_provider(&account).await?;
            // 更新 store 中的 token
            let mut store = lock_store(&state.store, "store")?;
            if let Some(a) = store.accounts.iter_mut().find(|a| a.id == id) {
                apply_refreshed_account_tokens(a, &refresh_result);
                save_store(&store)?;
            }
            refresh_result.access_token
        } else {
            access_token
        }
    } else {
        access_token
    };

    // 3. 解析 Kiro 调用上下文
    let ctx = crate::commands::common::resolve_kiro_call_context(&account, "us-east-1");

    // 4. 调用 API（失败后刷新重试）
    let overage_status = if enabled { "ENABLED" } else { "DISABLED" };
    let client = KiroQClient::new()?;

    let result = client
        .set_user_preference(
            &final_access_token,
            &ctx.machine_id,
            &ctx.region,
            ctx.profile_arn.as_deref(),
            overage_status,
        )
        .await;

    // 如果首次调用失败且是认证错误，刷新 token 后重试
    if let Err(ref e) = result {
        if is_auth_error_message(e) {
            log::info!("[set_overage_status] Token 失效，尝试刷新后重试");
            let refresh_result = refresh_token_by_provider(&account).await?;

            // 更新 store 中的 token（在独立作用域内，确保锁在 await 之前释放）
            {
                let mut store = lock_store(&state.store, "store")?;
                if let Some(a) = store.accounts.iter_mut().find(|a| a.id == id) {
                    apply_refreshed_account_tokens(a, &refresh_result);
                    save_store(&store)?;
                }
            } // 锁在这里释放

            // 用新 token 重试
            client
                .set_user_preference(
                    &refresh_result.access_token,
                    &ctx.machine_id,
                    &ctx.region,
                    ctx.profile_arn.as_deref(),
                    overage_status,
                )
                .await?;
        } else {
            return Err(e.clone());
        }
    } else {
        result?;
    }

    // 5. 更新本地 usage_data 中的 overageConfiguration.overageStatus
    let mut store = lock_store(&state.store, "store")?;
    if let Some(a) = store.accounts.iter_mut().find(|a| a.id == id) {
        if let Some(ref mut usage_data) = a.usage_data {
            if let Some(overage_config) = usage_data.get_mut("overageConfiguration") {
                if let Some(obj) = overage_config.as_object_mut() {
                    obj.insert(
                        "overageStatus".to_string(),
                        serde_json::Value::String(overage_status.to_string()),
                    );
                }
            } else {
                // 如果 overageConfiguration 不存在，创建它
                if let Some(obj) = usage_data.as_object_mut() {
                    obj.insert(
                        "overageConfiguration".to_string(),
                        serde_json::json!({
                            "overageStatus": overage_status
                        }),
                    );
                }
            }
        }
        save_store(&store)?;
    }

    Ok(())
}

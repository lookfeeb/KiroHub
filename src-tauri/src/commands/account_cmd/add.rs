use super::*;

#[tauri::command]
pub async fn verify_account(
    state: State<'_, AppState>,
    params: VerifyAccountParams,
) -> Result<VerifyAccountResponse, String> {
    let VerifyAccountParams {
        access_token: _,
        refresh_token,
        provider,
        client_id,
        client_secret,
        region,
    } = params;

    let is_idc = provider == "BuilderId" || provider == "Enterprise";

    // 刷新 token
    let (new_access_token, new_refresh_token) = if is_idc {
        let (cid, csec, reg) = if client_id.is_some() && client_secret.is_some() {
            (client_id, client_secret, region)
        } else {
            let store = lock_store(&state.store, "store")?;
            store
                .accounts
                .iter()
                .find(|a| a.refresh_token.as_ref() == Some(&refresh_token))
                .map_or((None, None, None), |a| {
                    (
                        a.client_id.clone(),
                        a.client_secret.clone(),
                        a.region.clone(),
                    )
                })
        };

        let cid = cid.ok_or("IdC 账号缺少 client_id，请重新添加账号")?;
        let csec = csec.ok_or("IdC 账号缺少 client_secret，请重新添加账号")?;
        let metadata = RefreshMetadata {
            client_id: Some(cid),
            client_secret: Some(csec),
            region: reg.clone(),
            ..Default::default()
        };

        let idc_provider = IdcProvider::new(&provider, reg.as_deref().unwrap_or("us-east-1"), None);
        let auth = idc_provider.refresh_token(&refresh_token, metadata).await?;
        (auth.access_token, auth.refresh_token)
    } else {
        let auth = refresh_token_desktop(&refresh_token).await?;
        (auth.access_token, auth.refresh_token)
    };

    // 获取 usage_data（使用统一的 getUsageLimits 接口）
    let temp_account = {
        let store = lock_store(&state.store, "store")?;
        let account = store
            .accounts
            .iter()
            .find(|a| a.refresh_token.as_ref() == Some(&refresh_token))
            .ok_or("Account not found")?;

        let mut temp_account = account.clone();
        temp_account.access_token = Some(new_access_token.clone());
        temp_account
    }; // MutexGuard 在这里被释放

    let usage_result = get_usage_by_account(&temp_account, &new_access_token).await?;
    let usage_data = usage_result.usage_data.clone();

    // 更新数据库（包括状态）
    {
        let mut store = lock_store(&state.store, "store")?;
        if let Some(account) = store
            .accounts
            .iter_mut()
            .find(|a| a.refresh_token.as_ref() == Some(&refresh_token))
        {
            // 更新 token
            account.access_token = Some(new_access_token.clone()); // ✅ 这里必须 clone，因为后面还要用
            account.refresh_token = Some(new_refresh_token.clone()); // ✅ 这里必须 clone，因为后面还要用
            // 更新 usage_data 和状态（检测封禁）
            account.usage_data = Some(usage_result.usage_data);
            update_account_status(account, usage_result.is_banned, usage_result.is_auth_error);
            save_store(&store)?;
        }
    }

    Ok(VerifyAccountResponse {
        usage_data, // 直接返回，前端解析
        access_token: new_access_token,
        refresh_token: new_refresh_token,
    })
}

#[tauri::command]
pub async fn add_account_by_social(
    state: State<'_, AppState>,
    refresh_token: String,
    provider: Option<String>,
    machine_id: Option<String>,
    access_token: Option<String>,
) -> Result<AddAccountResult, String> {
    let idp = provider.as_deref().unwrap_or("Google").to_string(); // ✅ 避免不必要的 clone

    // 先尝试用传入的 access_token 获取配额
    let (final_access_token, final_refresh_token, final_profile_arn, usage_result) =
        if let Some(at) = access_token {
            match get_usage_by_provider(&idp, &at).await {
                Ok(result) if result.is_auth_error => {
                    // 401 了，刷新 token
                    let refresh_result = refresh_token_desktop(&refresh_token).await?;
                    let new_usage =
                        get_usage_by_provider(&idp, &refresh_result.access_token).await?;
                    (
                        refresh_result.access_token,
                        refresh_result.refresh_token,
                        refresh_result.profile_arn,
                        new_usage,
                    )
                }
                Ok(result) => {
                    // access_token 有效，但没有 profile_arn，需要刷新一次获取
                    let refresh_result = refresh_token_desktop(&refresh_token).await?;
                    (
                        at,
                        refresh_token.clone(),
                        refresh_result.profile_arn,
                        result,
                    )
                }
                Err(e) => return Err(e),
            }
        } else {
            // 没有 access_token，直接刷新
            let refresh_result = refresh_token_desktop(&refresh_token).await?;
            let usage_result = get_usage_by_provider(&idp, &refresh_result.access_token).await?;
            (
                refresh_result.access_token,
                refresh_result.refresh_token,
                refresh_result.profile_arn,
                usage_result,
            )
        };

    // 封禁账号直接报错
    if usage_result.is_banned {
        return Err("BANNED: 账号已被封禁".to_string());
    }

    let (new_email, user_id) = extract_user_info(&usage_result.usage_data);

    // BuilderId 账号允许使用 userId 或 email，如果都没有则用 refreshToken 作为标识
    let final_email = new_email
        .or(user_id.clone())
        .unwrap_or_else(|| format!("builderid_{}", &refresh_token[..8]));

    // 根据邮箱推断最终 provider
    let idp = provider.unwrap_or_else(|| {
        if final_email.contains("gmail") {
            "Google".to_string()
        } else if final_email.contains("github") {
            "Github".to_string()
        } else {
            "Google".to_string()
        }
    });

    let mut store = lock_store(&state.store, "store")?;
    let existing_idx = find_existing_account_idx(
        &store.accounts,
        Some(&final_email),
        &idp,
        &final_refresh_token,
        user_id.as_ref(),
    );

    let is_new = existing_idx.is_none();

    let account = if let Some(idx) = existing_idx {
        let existing = &mut store.accounts[idx];
        // 直接移动所有权，避免 clone
        existing.access_token = Some(final_access_token.clone()); // ✅ 后面还要用，必须 clone
        existing.refresh_token = Some(final_refresh_token.clone()); // ✅ 后面还要用，必须 clone
        existing.profile_arn = Some(final_profile_arn.clone()); // ✅ 保存 profile_arn
        existing.user_id = user_id;
        existing.usage_data = Some(usage_result.usage_data);
        update_account_status(existing, usage_result.is_banned, usage_result.is_auth_error);
        existing.clone() // ✅ 必须 clone，因为要返回给前端
    } else {
        let mut account = Account::new(final_email.clone(), format!("Kiro {idp} 账号"));
        account.access_token = Some(final_access_token.clone()); // ✅ 后面还要用，必须 clone
        account.refresh_token = Some(final_refresh_token.clone()); // ✅ 后面还要用，必须 clone
        account.profile_arn = Some(final_profile_arn.clone()); // ✅ 保存 profile_arn
        account.provider = Some(idp.clone());
        account.auth_method = Some("social".to_string());
        account.user_id = user_id;
        account.usage_data = Some(usage_result.usage_data);
        update_account_status(&mut account, usage_result.is_banned, usage_result.is_auth_error);
        // 使用传入的 machine_id，没有则自动生成
        account.machine_id =
            machine_id.or_else(|| Some(uuid::Uuid::new_v4().to_string().to_lowercase())); // ✅ 避免 clone
        store.accounts.insert(0, account.clone());
        account
    };

    save_store(&store)?;
    drop(store);

    let user = User {
        id: uuid::Uuid::new_v4().to_string(),
        email: account.email.clone(), // ✅ 必须 clone，因为 account 被移动了
        name: account
            .email
            .as_ref()
            .and_then(|e| e.split('@').next())
            .unwrap_or("User")
            .to_string(),
        avatar: None,
        provider: idp,
    };
    *lock_store(&state.auth.user, "auth user")? = Some(user);
    *lock_store(&state.auth.access_token, "auth access_token")? = Some(final_access_token);

    Ok(AddAccountResult { account, is_new })
}

/// 添加本地 Kiro IDE 账号
#[tauri::command]
pub async fn add_local_kiro_account(
    state: State<'_, AppState>,
) -> Result<AddAccountResult, String> {
    use crate::kiro::ide::{get_client_registration, get_kiro_local_token};

    let local_token = get_kiro_local_token()
        .await
        .ok_or("未找到本地 Kiro 账号，请先在 Kiro IDE 中登录")?;

    let refresh_token = local_token
        .refresh_token
        .ok_or("本地账号缺少 refresh_token")?;

    let auth_method = local_token.auth_method.as_deref().unwrap_or("social");
    let provider = local_token
        .provider
        .clone()
        .unwrap_or_else(|| "Google".to_string());

    // 根据 auth_method 调用对应的添加函数
    if auth_method == "IdC" {
        let hash = local_token
            .client_id_hash
            .clone()
            .ok_or("IdC 账号缺少 clientIdHash")?;
        let region = local_token
            .region
            .clone()
            .unwrap_or_else(|| "us-east-1".to_string());

        let client_reg = get_client_registration(&hash)
            .await
            .ok_or(format!("未找到客户端注册信息: {hash}.json"))?;

        // 统一调用 add_account_by_idc（展开参数）
        add_account_by_idc(
            state,
            Some(provider),                   // provider: BuilderId 或 Enterprise
            refresh_token,                    // refresh_token
            client_reg.client_id,             // client_id
            client_reg.client_secret,         // client_secret
            Some(region),                     // region
            None,                             // machine_id: 本地导入不指定，自动生成
            local_token.access_token.clone(), // access_token
            None,                             // password: 本地导入无密码
            None,                             // start_url: 本地导入无 start_url
            Some(hash),                       // client_id_hash: 直接使用 Kiro IDE 提供的
        )
        .await
    } else {
        add_account_by_social(
            state,
            refresh_token,
            Some(provider),
            None,                             // 本地导入不指定 machine_id，自动生成
            local_token.access_token.clone(), // 传入 access_token
        )
        .await
    }
}

/// 添加 IdC 账号（BuilderId 或 Enterprise）
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri IPC 命令签名需要显式参数，避免前后端调用契约破坏
pub async fn add_account_by_idc(
    state: State<'_, AppState>,
    provider: Option<String>,
    refresh_token: String,
    client_id: String,
    client_secret: String,
    region: Option<String>,
    machine_id: Option<String>,
    access_token: Option<String>,
    password: Option<String>,
    start_url: Option<String>,
    client_id_hash: Option<String>,
) -> Result<AddAccountResult, String> {
    // 从参数中获取 provider，默认为 BuilderId
    let provider_id = provider.unwrap_or_else(|| "BuilderId".to_string());

    // 验证 provider 是否合法
    if provider_id != "BuilderId" && provider_id != "Enterprise" {
        return Err(format!("不支持的 provider: {}", provider_id));
    }

    add_account_by_idc_internal(
        state,
        IdcAccountParams {
            refresh_token,
            client_id,
            client_secret,
            region,
            machine_id,
            access_token,
            password,
            provider_id,
            start_url,
            client_id_hash,
        },
    )
    .await
}

/// 内部函数：添加 `IdC` 账号（BuilderId 或 Enterprise）
pub(crate) async fn add_account_by_idc_internal(
    state: State<'_, AppState>,
    params: IdcAccountParams,
) -> Result<AddAccountResult, String> {
    let is_enterprise = params.provider_id == "Enterprise";

    // 从 clientSecret JWT 中提取 startUrl（如果未提供）
    let start_url = if params.start_url.is_some() {
        params.start_url.clone()
    } else if is_enterprise {
        extract_start_url_from_client_secret(&params.client_secret)
    } else {
        None
    };

    // BuilderId 和 Enterprise 都使用默认 region（如果未提供）
    let mut region = params.region.unwrap_or_else(|| "us-east-1".to_string());

    // 获取 machine_id（企业账号多区域探测需要）
    let machine_id = params
        .machine_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string().to_lowercase());

    // 企业账号导入时强制刷新 token（导入时的 access_token 很可能已过期）
    let (
        final_access_token,
        final_refresh_token,
        usage_result,
        expires_at,
        id_token,
        sso_session_id,
    ) = if is_enterprise || params.access_token.is_none() {
        // 企业账号或没有 access_token 时，直接刷新
        let metadata = RefreshMetadata {
            client_id: Some(params.client_id.clone()),
            client_secret: Some(params.client_secret.clone()),
            region: Some(region.clone()),
            ..Default::default()
        };
        let idc_provider = IdcProvider::new(&params.provider_id, &region, start_url.clone());
        let auth_result = idc_provider
            .refresh_token(&params.refresh_token, metadata)
            .await?;

        // 企业账号使用多区域探测
        let usage_result = if is_enterprise {
            let (result, detected_region) = get_enterprise_usage_with_region_probe(&auth_result.access_token, &machine_id).await?;
            region = detected_region;
            result
        } else {
            get_usage_by_provider(&params.provider_id, &auth_result.access_token).await?
        };

        let expires_at = calc_expires_at(auth_result.expires_in);
        (
            auth_result.access_token,
            auth_result.refresh_token,
            usage_result,
            expires_at,
            auth_result.id_token,
            auth_result.sso_session_id,
        )
    } else if let Some(at) = params.access_token {
        // BuilderId 且有 access_token 时，先尝试使用
            // BuilderId 使用原有逻辑
            match get_usage_by_provider(&params.provider_id, &at).await {
                Ok(result) if result.is_auth_error => {
                    // 401 了，刷新 token
                    let metadata = RefreshMetadata {
                        client_id: Some(params.client_id.clone()),
                        client_secret: Some(params.client_secret.clone()),
                        region: Some(region.clone()),
                        ..Default::default()
                    };
                    let idc_provider =
                        IdcProvider::new(&params.provider_id, &region, start_url.clone());
                    let auth_result = idc_provider
                        .refresh_token(&params.refresh_token, metadata)
                        .await?;
                    let new_usage =
                        get_usage_by_provider(&params.provider_id, &auth_result.access_token).await?;
                    let expires_at = calc_expires_at(auth_result.expires_in);
                    (
                        auth_result.access_token,
                        auth_result.refresh_token,
                        new_usage,
                        expires_at,
                        auth_result.id_token,
                        auth_result.sso_session_id,
                    )
                }
                Ok(result) => {
                    // access_token 有效，不需要刷新
                    (
                        at,
                        params.refresh_token.clone(),
                        result,
                        String::new(),
                        None,
                        None,
                    )
                }
                Err(e) => return Err(e),
            }
    } else {
        // 没有 access_token，直接刷新
        let metadata = RefreshMetadata {
            client_id: Some(params.client_id.clone()),
            client_secret: Some(params.client_secret.clone()),
            region: Some(region.clone()),
            ..Default::default()
        };
        let idc_provider = IdcProvider::new(&params.provider_id, &region, start_url.clone());
        let auth_result = idc_provider
            .refresh_token(&params.refresh_token, metadata)
            .await?;

        // 企业账号使用多区域探测
        let usage_result = if is_enterprise {
            let (result, detected_region) = get_enterprise_usage_with_region_probe(&auth_result.access_token, &machine_id).await?;
            region = detected_region;
            result
        } else {
            get_usage_by_provider(&params.provider_id, &auth_result.access_token).await?
        };

        let expires_at = calc_expires_at(auth_result.expires_in);
        (
            auth_result.access_token,
            auth_result.refresh_token,
            usage_result,
            expires_at,
            auth_result.id_token,
            auth_result.sso_session_id,
        )
    };

    // 封禁账号直接报错
    if usage_result.is_banned {
        return Err("BANNED: 账号已被封禁".to_string());
    }

    let (new_email, user_id) = extract_user_info(&usage_result.usage_data);

    // ========== Enterprise 和 BuilderId 分开处理 ==========

    if is_enterprise {
        // Enterprise 账号：必须有 user_id，email 可选
        let user_id = user_id.ok_or_else(|| {
            format!(
                "Enterprise 账号缺少 userId。API 返回的数据：\n{}",
                serde_json::to_string_pretty(&usage_result.usage_data).unwrap_or_default()
            )
        })?;

        // 计算 client_id_hash（可选）
        let client_id_hash = if let Some(hash) = params.client_id_hash.clone() {
            Some(hash) // 如果提供了 clientIdHash，直接使用
        } else {
            start_url.as_ref().map(|url| calculate_client_id_hash(url)) // 如果提取到了 startUrl，计算
        };

        let mut store = lock_store(&state.store, "store")?;
        let existing_idx = find_existing_account_idx(
            &store.accounts,
            new_email.as_ref(),
            &params.provider_id,
            &final_refresh_token,
            Some(&user_id),
        );

        let is_new = existing_idx.is_none();

        let account = if let Some(idx) = existing_idx {
            // 更新已存在的账号
            let existing = &mut store.accounts[idx];
            existing.access_token = Some(final_access_token);
            existing.refresh_token = Some(final_refresh_token);
            existing.email = new_email; // 更新 email（可能是 None）
            existing.user_id = Some(user_id);
            existing.provider = Some(params.provider_id.clone());
            existing.auth_method = Some("IdC".to_string()); // 确保 authMethod 正确
            if !expires_at.is_empty() {
                existing.expires_at = Some(expires_at);
            }
            existing.client_id = Some(params.client_id.clone());
            existing.client_secret = Some(params.client_secret.clone());
            existing.region = Some(region.clone());
            existing.client_id_hash = client_id_hash.clone(); // 可能是 None
            existing.start_url = start_url.clone();
            if id_token.is_some() {
                existing.id_token = id_token;
            }
            if sso_session_id.is_some() {
                existing.sso_session_id = sso_session_id;
            }
            existing.usage_data = Some(usage_result.usage_data);
            update_account_status(existing, usage_result.is_banned, usage_result.is_auth_error);
            existing.clone()
        } else {
            // 创建新的 Enterprise 账号
            let mut account =
                Account::new_enterprise(user_id.clone(), "Kiro Enterprise 账号".to_string());
            account.access_token = Some(final_access_token);
            account.refresh_token = Some(final_refresh_token);
            account.email = new_email; // 可能是 None
            if !expires_at.is_empty() {
                account.expires_at = Some(expires_at);
            }
            account.client_id = Some(params.client_id.clone());
            account.client_secret = Some(params.client_secret.clone());
            account.region = Some(region.clone());
            account.client_id_hash = client_id_hash; // 可能是 None
            account.start_url = start_url.clone();
            account.id_token = id_token;
            account.sso_session_id = sso_session_id;
            account.usage_data = Some(usage_result.usage_data);
            update_account_status(&mut account, usage_result.is_banned, usage_result.is_auth_error);
            account.machine_id = Some(
                params
                    .machine_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string().to_lowercase()),
            );
            account.password.clone_from(&params.password);
            store.accounts.insert(0, account.clone());
            account
        };

        save_store(&store)?;
        Ok(AddAccountResult { account, is_new })
    } else {
        // BuilderId 账号：允许没有 userId/email，用 refreshToken 去重

        // 计算 client_id_hash（可选）
        let client_id_hash = Some(resolve_builder_client_id_hash(
            params.client_id_hash,
            params.start_url.as_deref(),
        ));

        let mut store = lock_store(&state.store, "store")?;
        let existing_idx = find_existing_account_idx(
            &store.accounts,
            new_email.as_ref(),
            &params.provider_id,
            &final_refresh_token,
            user_id.as_ref(),
        );

        let is_new = existing_idx.is_none();

        let account = if let Some(idx) = existing_idx {
            // 更新已存在的账号
            let existing = &mut store.accounts[idx];
            existing.access_token = Some(final_access_token);
            existing.refresh_token = Some(final_refresh_token);
            existing.provider = Some(params.provider_id.clone());
            existing.auth_method = Some("IdC".to_string()); // 确保 authMethod 正确
            existing.user_id = user_id;
            if !expires_at.is_empty() {
                existing.expires_at = Some(expires_at);
            }
            existing.client_id = Some(params.client_id.clone());
            existing.client_secret = Some(params.client_secret.clone());
            existing.region = Some(region.clone());
            existing.client_id_hash = client_id_hash.clone(); // 可能是 None
            if id_token.is_some() {
                existing.id_token = id_token;
            }
            if sso_session_id.is_some() {
                existing.sso_session_id = sso_session_id;
            }
            existing.usage_data = Some(usage_result.usage_data);
            update_account_status(existing, usage_result.is_banned, usage_result.is_auth_error);
            existing.clone()
        } else {
            // 创建新的 BuilderId 账号
            // 使用 user_id 或 email 作为标识
            let display_id = new_email
                .clone()
                .or_else(|| user_id.clone())
                .unwrap_or_else(|| "BuilderId 账号".to_string());

            let mut account = Account::new(display_id.clone(), "Kiro BuilderId 账号".to_string());
            account.access_token = Some(final_access_token);
            account.refresh_token = Some(final_refresh_token);
            account.provider = Some(params.provider_id.clone());
            account.auth_method = Some("IdC".to_string());
            account.email = new_email; // 可能是 None
            account.user_id = user_id; // 可能是 None
            if !expires_at.is_empty() {
                account.expires_at = Some(expires_at);
            }
            account.client_id = Some(params.client_id.clone());
            account.client_secret = Some(params.client_secret.clone());
            account.region = Some(region.clone());
            account.client_id_hash = client_id_hash; // 可能是 None
            account.id_token = id_token;
            account.sso_session_id = sso_session_id;
            account.usage_data = Some(usage_result.usage_data);
            update_account_status(&mut account, usage_result.is_banned, usage_result.is_auth_error);
            account.machine_id = Some(
                params
                    .machine_id
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string().to_lowercase()),
            );
            account.password = params.password;
            store.accounts.insert(0, account.clone());
            account
        };

        save_store(&store)?;
        Ok(AddAccountResult { account, is_new })
    }
}

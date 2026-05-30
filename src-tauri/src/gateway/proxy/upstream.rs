use super::*;

/// 获取账号可用模型列表
///
/// 调用 Kiro Q API 的 ListAvailableModels 接口获取账号权限内的模型
pub(crate) async fn get_available_models_for_upstream(
    upstream: &UpstreamCredentials,
) -> Result<Vec<String>, String> {
    let client = KiroQClient::new()?;

    let response = client
        .list_available_models(
            &upstream.access_token,
            &get_machine_id(),
            &upstream.region,
            upstream.profile_arn.as_deref(),
            None, // model_provider
            None, // next_token
        )
        .await?;

    // 解析返回的模型列表
    let models = response
        .get("models")
        .and_then(|v| v.as_array())
        .ok_or("Invalid response: missing models array")?
        .iter()
        .filter_map(|m| m.get("modelId").and_then(|id| id.as_str()).map(String::from))
        .collect();

    Ok(models)
}


pub(crate) async fn send_generate_request<T: serde::Serialize + ?Sized>(
    http: &Client,
    upstream: &UpstreamCredentials,
    upstream_payload: &T,
) -> Result<reqwest::Response, UpstreamRequestError> {
    let upstream_url = format!(
        "{}/generateAssistantResponse",
        build_q_service_url(&upstream.region)
    );

    const MAX_RETRIES: u32 = 3;
    let mut attempt = 0;

    loop {
        attempt += 1;

        let upstream_resp = with_kiro_upstream_headers(
            http.post(&upstream_url),
            upstream,
            "application/vnd.amazon.eventstream",
            true,
            true,
            false,
        )
        .json(upstream_payload)
        .send()
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_GATEWAY,
                "api_error",
                sanitize_error(&format!("上游请求失败: {error}")),
                None,
            )
        })?;

        let status = upstream_resp.status();

        if status.is_success() {
            return Ok(upstream_resp);
        }

        let body = upstream_resp.text().await.unwrap_or_default();

        // 429 限流错误不重试，直接返回
        if status == StatusCode::TOO_MANY_REQUESTS {
            let (mapped_status, error_type, message) = map_upstream_error(status, &body);
            return Err((mapped_status, error_type, message, Some(body)));
        }

        // 403 认证错误：直接返回，不在这里重试
        // 外层会通过LoadBalancer切换账号或刷新token
        if status == StatusCode::FORBIDDEN {
            log::warn!("[网关] 上游认证失败 (403)，返回错误");
            let (mapped_status, error_type, message) = map_upstream_error(status, &body);
            return Err((mapped_status, error_type, message, Some(body)));
        }

        // 5xx 服务器错误才重试
        let should_retry = attempt < MAX_RETRIES && status.is_server_error();

        if should_retry {
            let backoff_ms = 1000 * 2u64.pow(attempt - 1);
            log::warn!(
                "上游请求失败 (状态: {}, 尝试: {}/{}), {}ms 后重试",
                status,
                attempt,
                MAX_RETRIES,
                backoff_ms
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
            continue;
        }

        let (mapped_status, error_type, message) = map_upstream_error(status, &body);
        return Err((mapped_status, error_type, message, Some(body)));
    }
}


pub(crate) fn with_kiro_upstream_headers(
    builder: reqwest::RequestBuilder,
    upstream: &UpstreamCredentials,
    accept: &str,
    include_opt_out: bool,
    include_agent_mode: bool,
    include_profile_arn_header: bool,
) -> reqwest::RequestBuilder {
    let invocation_id = uuid::Uuid::new_v4().to_string();

    let mut builder = builder
        .header("Authorization", format!("Bearer {}", upstream.access_token))
        .header("Content-Type", "application/json")
        .header("Accept", accept)
        .header("host", format!("q.{}.amazonaws.com", upstream.region))
        .header(header::USER_AGENT, upstream.user_agent.clone())
        .header("x-amz-user-agent", upstream.user_agent.clone())
        .header("amz-sdk-invocation-id", invocation_id)
        .header("amz-sdk-request", "attempt=1; max=3");

    if include_opt_out && upstream.send_opt_out {
        builder = builder.header("x-amzn-codewhisperer-optout", "true");
    }
    if include_agent_mode {
        builder = builder.header("x-amzn-kiro-agent-mode", DEFAULT_AGENT_MODE);
    }
    if include_profile_arn_header {
        if let Some(profile_arn) = upstream
            .profile_arn
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            builder = builder.header("x-amzn-kiro-profile-arn", profile_arn);
        }
    }
    if should_add_redirect_for_internal(upstream.provider.as_deref()) {
        builder = builder.header("redirect-for-internal", "true");
    }

    builder
}


pub(crate) async fn resolve_upstream_credentials(
    config: &GatewayConfig,
    state: &RouterState,
) -> Result<UpstreamCredentials, String> {
    match config.account_mode.as_str() {
        "single" | "group" | "pool" => resolve_managed_account_credentials(config, state).await,
        "local" => Err("反代不再支持 local 模式，请改用 single/group/pool 账号池模式".to_string()),
        _ => Err("accountMode 必须是 single/group/pool".to_string()),
    }
}


pub(crate) async fn resolve_managed_account_credentials(
    config: &GatewayConfig,
    state: &RouterState,
) -> Result<UpstreamCredentials, String> {
    let mut store = AccountStore { accounts: Vec::new() };
    store.reload_async().await;

    // 自愈机制：检查是否所有账号都因 "TooManyFailures" 被禁用
    let all_disabled_by_failures = match config.account_mode.as_str() {
        "single" => {
            store
                .accounts
                .iter()
                .filter(|account| config.account_id.as_deref() == Some(account.id.as_str()))
                .all(|account| {
                    account.disabled_reason.as_deref() == Some("TooManyFailures")
                })
        }
        "group" => {
            let group_accounts: Vec<_> = store
                .accounts
                .iter()
                .filter(|account| config.group_id.as_deref() == account.group_id.as_deref())
                .collect();

            !group_accounts.is_empty()
                && group_accounts.iter().all(|account| {
                    account.disabled_reason.as_deref() == Some("TooManyFailures")
                })
        }
        "pool" => {
            // 候选集按配置(pool=所有账号)判断，不能先用 is_available() 过滤，
            // 否则全部账号都因 TooManyFailures 被禁用时候选集为空，永远无法自愈。
            !store.accounts.is_empty()
                && store.accounts.iter().all(|account| {
                    account.disabled_reason.as_deref() == Some("TooManyFailures")
                })
        }
        _ => false,
    };

    if all_disabled_by_failures {
        let mut healed = Vec::new();
        for account in store.accounts.iter_mut() {
            if account.disabled_reason.as_deref() == Some("TooManyFailures") {
                account.failure_count = 0;
                account.status = "active".to_string();
                account.disabled_reason = None;
                healed.push(account.clone());
            }
        }
        for acc in &healed {
            let _ = store.update_one(acc);
        }
    }

    let accounts = match config.account_mode.as_str() {
        "single" => store
            .accounts
            .iter()
            .filter(|account| config.account_id.as_deref() == Some(account.id.as_str()))
            .cloned()
            .collect::<Vec<_>>(),
        "group" => store
            .accounts
            .iter()
            .filter(|account| {
                config.group_id.as_deref() == account.group_id.as_deref() && account.is_available() && account.enabled
            })
            .cloned()
            .collect::<Vec<_>>(),
        "pool" => store
            .accounts
            .iter()
            .filter(|account| account.is_available() && account.enabled)
            .cloned()
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    if accounts.is_empty() {
        return Err("未找到符合反代配置的可用账号".to_string());
    }

    // 使用 LoadBalancer 选择账号
    let selected_account = state.load_balancer.select_account(&accounts).await;

    let Some(account) = selected_account else {
        return Err("LoadBalancer 未能选择可用账号".to_string());
    };

    let request_start = Instant::now();

    // 检查 token 是否需要刷新（只有快过期时才刷新）
    let need_refresh = match &account.expires_at {
        Some(expires_at) => is_token_expiring_soon(expires_at),
        None => true, // 没有过期时间，强制刷新
    };

    // 如果 token 没过期且有 access_token，直接使用
    if !need_refresh {
        if let Some(access_token) = &account.access_token {
            if !access_token.is_empty() {
                let ctx = crate::commands::common::resolve_kiro_call_context(
                    &account,
                    &state.config.region,
                );
                return Ok(UpstreamCredentials {
                    access_token: access_token.clone(),
                    profile_arn: ctx.profile_arn,
                    provider: account.provider.clone(),
                    region: ctx.region,
                    account_id: account.id.clone(),
                    source_label: format_managed_upstream_source(&state.config, &account),
                    user_agent: build_kiro_custom_user_agent(&ctx.machine_id),
                    auth_method: account.auth_method.clone(),
                    send_opt_out: should_send_codewhisperer_optout(),
                });
            }
        }
    }

    match refresh_token_by_provider(&account).await {
        Ok(refresh) => {
            let usage_result =
                crate::commands::common::get_usage_by_account(&account, &refresh.access_token)
                    .await;
            let mut usage_data = None;
            let mut is_banned = false;
            let mut is_auth_error = false;

            if let Ok(usage) = usage_result {
                usage_data = Some(usage.usage_data);
                is_banned = usage.is_banned;
                is_auth_error = usage.is_auth_error;
            }

            // 失败追踪：如果账号被封禁或认证失败，累加失败计数
            let should_increment_failure = is_banned || is_auth_error;

            persist_account_refresh(
                &account,
                &refresh,
                usage_data.clone(),
                is_banned,
                is_auth_error,
                should_increment_failure,
            );

            if is_banned || is_auth_error {
                // 记录失败
                state.load_balancer.record_failure(&account.id).await;
                return Err(format!("账号 {} 已不可用", account.label));
            }

            if let Some(usage_data) = &usage_data {
                if usage_exceeds_threshold(usage_data, config.threshold) {
                    // 配额超阈值，直接禁用账号
                    state.load_balancer.record_failure(&account.id).await;
                    disable_account_by_id(&account.id, "配额已满");
                    return Err(format!(
                        "账号 {} 配额已满，已自动禁用",
                        account.label
                    ));
                }
            }

            // 记录成功
            let response_time_ms = request_start.elapsed().as_millis() as u64;
            state.load_balancer.record_success(&account.id, response_time_ms).await;

            let machine_id = account
                .machine_id
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(get_machine_id);
            let profile_arn = match account.provider.as_deref() {
                Some("Enterprise") => None,
                provider => refresh.profile_arn.or_else(|| account.profile_arn.clone())
                    .or_else(|| Some(resolve_default_profile_arn(provider).to_string())),
            };
            let region = resolve_kiro_upstream_region(
                profile_arn.as_deref(),
                account.region.as_deref(),
                &config.region,
            );

            Ok(UpstreamCredentials {
                access_token: refresh.access_token,
                profile_arn,
                provider: account.provider.clone(),
                region,
                account_id: account.id.clone(),
                source_label: format_managed_upstream_source(config, &account),
                user_agent: build_kiro_custom_user_agent(&machine_id),
                auth_method: account.auth_method.clone(),
                send_opt_out: should_send_codewhisperer_optout(),
            })
        }
        Err(error) => {
            // 记录失败
            state.load_balancer.record_failure(&account.id).await;

            Err(format!(
                "刷新账号 {} 失败: {}",
                account.label,
                sanitize_error(&error)
            ))
        }
    }
}
/// 根据账号 provider 返回默认的 profileArn
/// BuilderId 账号和 Social 账号（Github/Google）使用不同的 profileArn


pub(crate) fn format_managed_upstream_source(config: &GatewayConfig, account: &Account) -> String {
    // 只使用 email 或 user_id，都没有则返回 "unknown"
    let account_label = if let Some(email) = account
        .email
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        email.trim().to_string()
    } else if let Some(user_id) = account
        .user_id
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        user_id.trim().to_string()
    } else {
        "unknown".to_string()
    };

    match config.account_mode.as_str() {
        "single" => format!("single:{account_label}"),
        "group" => format!(
            "group:{}:{account_label}",
            config.group_id.as_deref().unwrap_or("unknown")
        ),
        "pool" => format!("pool:{account_label}"),
        _ => account_label,
    }
}


/// 禁用指定账号（配额满时自动调用）
pub(crate) fn disable_account_by_id(account_id: &str, reason: &str) {
    let mut store = AccountStore::new();
    if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
        account.enabled = false;
        account.disabled_reason = Some(reason.to_string());
        let updated = account.clone();
        // 仅写回该账号行，避免整表覆盖clobber其它账号的并发更新
        let _ = store.update_one(&updated);
        log::info!("[网关] 账号 {} 已自动禁用: {}", account_id, reason);
    }
}


pub(crate) fn persist_account_refresh(
    account: &Account,
    refresh: &RefreshResult,
    usage_data: Option<Value>,
    is_banned: bool,
    is_auth_error: bool,
    should_increment_failure: bool,
) {
    let mut store = AccountStore::new();
    if let Some(target) = store
        .accounts
        .iter_mut()
        .find(|candidate| candidate.id == account.id)
    {
        // 应用 token 字段更新（Option 字段仅在新值存在时覆盖，避免清空已有值）
        crate::commands::common::apply_refreshed_account_tokens(target, &refresh);
        if let Some(data) = usage_data {
            target.usage_data = Some(data);
        }
        update_account_status(target, is_banned, is_auth_error);

        // 失败追踪逻辑
        if should_increment_failure {
            target.failure_count += 1;
            target.last_failure_at = Some(Local::now().to_rfc3339());

            // 如果失败次数达到阈值，自动禁用账号
            if target.failure_count >= MAX_FAILURES_PER_ACCOUNT {
                target.status = "disabled".to_string();
                target.disabled_reason = Some("TooManyFailures".to_string());
                log::warn!(
                    "[Gateway] 账号 {} 失败次数达到 {}，自动禁用",
                    target.label, MAX_FAILURES_PER_ACCOUNT
                );
            }
        } else {
            // 请求成功，重置失败计数并累加成功计数
            target.failure_count = 0;
            target.success_count += 1;
            target.last_failure_at = None;

            // 如果之前因为失败过多被禁用，现在恢复
            if target.disabled_reason.as_deref() == Some("TooManyFailures") {
                target.disabled_reason = None;
                if target.status == "disabled" {
                    target.status = "active".to_string();
                }
            }
        }

        let updated = target.clone();
        let _ = store.update_one(&updated);
    }
}


pub(crate) fn usage_exceeds_threshold(usage_data: &Value, threshold: i32) -> bool {
    crate::core::usage::usage_exceeds_threshold(Some(usage_data), f64::from(threshold))
}


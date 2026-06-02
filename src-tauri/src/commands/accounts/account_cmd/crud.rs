use super::*;

#[tauri::command]
pub fn get_accounts(state: State<AppState>) -> Vec<Account> {
    match lock_store(&state.store, "store") {
        Ok(mut store) => {
            // 每次获取前重新从文件加载，确保数据最新
            store.reload();
            store.get_all()
        }
        Err(err) => {
            eprintln!("[account_cmd] {err}");
            Vec::new()
        }
    }
}

#[tauri::command]
pub fn delete_account(state: State<AppState>, id: &str) -> bool {
    match lock_store(&state.store, "store") {
        Ok(mut store) => store.delete(id).unwrap_or_else(|err| {
            eprintln!("[account_cmd] {err}");
            false
        }),
        Err(err) => {
            eprintln!("[account_cmd] {err}");
            false
        }
    }
}

#[tauri::command]
pub fn delete_accounts(state: State<AppState>, ids: Vec<String>) -> usize {
    match lock_store(&state.store, "store") {
        Ok(mut store) => store.delete_many(&ids).unwrap_or_else(|err| {
            eprintln!("[account_cmd] {err}");
            0
        }),
        Err(err) => {
            eprintln!("[account_cmd] {err}");
            0
        }
    }
}

#[tauri::command]
pub fn import_accounts(state: State<AppState>, json: &str) -> Result<usize, String> {
    let mut store = lock_store(&state.store, "store")?;
    store.import_from_json(json)
}

#[tauri::command]
pub fn export_accounts(state: State<AppState>, ids: Option<Vec<String>>) -> String {
    let store = match lock_store(&state.store, "store") {
        Ok(store) => store,
        Err(err) => {
            eprintln!("[account_cmd] {err}");
            return "[]".to_string();
        }
    };

    // 修复账号数据
    let fix_account = |mut account: Account| -> Account {
        // 1. 修复 provider 为 null
        if account.provider.is_none() && account.auth_method.as_deref() == Some("IdC") {
            // IdC 账号但 provider 为 null，根据 start_url 或 client_secret 判断
            if let Some(ref start_url) = account.start_url {
                if start_url.contains("awsapps.com") {
                    account.provider = Some("Enterprise".to_string());
                } else {
                    account.provider = Some("BuilderId".to_string());
                }
            } else if let Some(ref client_secret) = account.client_secret {
                if client_secret.contains("initiateLoginUri") {
                    account.provider = Some("Enterprise".to_string());
                } else {
                    account.provider = Some("BuilderId".to_string());
                }
            } else {
                // 默认 BuilderId
                account.provider = Some("BuilderId".to_string());
            }
        } else if account.provider.is_none() && account.auth_method.as_deref() == Some("social") {
            // Social 账号但 provider 为 null，根据邮箱判断
            if let Some(ref email) = account.email {
                if email.contains("gmail") {
                    account.provider = Some("Google".to_string());
                } else if email.contains("github") {
                    account.provider = Some("Github".to_string());
                } else {
                    account.provider = Some("Google".to_string());
                }
            } else {
                account.provider = Some("Google".to_string());
            }
        }

        // 2. 修复 authMethod 为 null
        if account.auth_method.is_none() {
            if account.client_id.is_some() && account.client_secret.is_some() {
                account.auth_method = Some("IdC".to_string());
            } else {
                account.auth_method = Some("social".to_string());
            }
        }

        account
    };

    match ids {
        Some(id_list) if !id_list.is_empty() => {
            // 导出选中的账号
            let selected: Vec<Account> = store
                .accounts
                .iter()
                .filter(|a| id_list.contains(&a.id))
                .cloned()
                .map(fix_account)
                .collect();
            serde_json::to_string_pretty(&selected).unwrap_or_else(|_| "[]".to_string())
        }
        _ => {
            // 没有选中任何账号，返回空数组
            "[]".to_string()
        }
    }
}

/// 更新账号信息（支持修改 label、token、SSO Client ID/Secret、machineId）
#[tauri::command]
pub fn update_account(
    state: State<AppState>,
    params: UpdateAccountParams,
) -> Result<Account, String> {
    let mut store = lock_store(&state.store, "store")?;

    // 先找到索引，避免借用冲突
    let idx = store.accounts.iter().position(|a| a.id == params.id);

    if let Some(idx) = idx {
        if let Some(l) = params.label {
            store.accounts[idx].label = l;
        }
        if let Some(status) = params.status {
            store.accounts[idx].status = status;
        }
        if let Some(at) = params.access_token {
            store.accounts[idx].access_token = Some(at);
        }
        if let Some(rt) = params.refresh_token {
            store.accounts[idx].refresh_token = Some(rt);
        }
        // BuilderId SSO 字段
        if let Some(cid) = params.client_id {
            store.accounts[idx].client_id = Some(cid);
        }
        if let Some(csec) = params.client_secret {
            store.accounts[idx].client_secret = Some(csec);
        }
        // 机器码
        if let Some(mid) = params.machine_id {
            store.accounts[idx].machine_id = Some(mid);
        }
        // 启用/禁用
        if let Some(enabled) = params.enabled {
            store.accounts[idx].enabled = enabled;
        }
        let result = store.accounts[idx].clone();
        save_store(&store)?;
        Ok(result)
    } else {
        Err("账号不存在".to_string())
    }
}

/// 获取可用账号列表（用于自动换号）
#[tauri::command]
pub fn get_available_accounts(state: State<AppState>) -> Vec<Account> {
    match lock_store(&state.store, "store") {
        Ok(store) => store
            .get_available_accounts()
            .into_iter()
            .cloned()
            .collect(),
        Err(err) => {
            eprintln!("[account_cmd] {err}");
            Vec::new()
        }
    }
}

/// 按分组筛选账号
#[tauri::command]
pub fn get_accounts_by_group(state: State<AppState>, group_id: String) -> Vec<Account> {
    match lock_store(&state.store, "store") {
        Ok(store) => store
            .get_accounts_by_group(&group_id)
            .into_iter()
            .cloned()
            .collect(),
        Err(err) => {
            eprintln!("[account_cmd] {err}");
            Vec::new()
        }
    }
}

/// 按标签筛选账号
#[tauri::command]
pub fn get_accounts_by_tag(state: State<AppState>, tag_id: String) -> Vec<Account> {
    match lock_store(&state.store, "store") {
        Ok(store) => store
            .get_accounts_by_tag(&tag_id)
            .into_iter()
            .cloned()
            .collect(),
        Err(err) => {
            eprintln!("[account_cmd] {err}");
            Vec::new()
        }
    }
}

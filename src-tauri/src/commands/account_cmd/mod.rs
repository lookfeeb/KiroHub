// 账号相关命令 - 直接存储原始 usage_data

#![allow(clippy::needless_pass_by_value)] // Tauri 命令需要按值传递 State
#![allow(clippy::too_many_lines)] // 命令文件包含多个函数

mod types;
mod crud;
mod add;
mod sync;
mod usage;
mod token_status;
pub(crate) use types::*;
pub(crate) use crud::*;
pub(crate) use add::*;
pub(crate) use sync::*;
pub(crate) use usage::*;
pub(crate) use token_status::*;

use crate::core::account::Account;
use crate::auth::{refresh_token_desktop, User};
use crate::commands::account_models::{
    clear_available_models_cache, fetch_all_available_models, read_available_models_cache,
    write_available_models_cache, ListAvailableModelsResponse,
};
use crate::commands::common::{
    calc_expires_at, extract_user_info, find_account_by_id,
    find_existing_account_idx, get_enterprise_usage_with_region_probe, get_usage_by_account,
    get_usage_by_provider, is_auth_error_message, is_token_expired, is_token_expiring_soon,
    lock_store, refresh_token_by_provider, save_store, token_needs_refresh, update_account_status, RefreshResult,
};
use crate::auth::providers::{AuthProvider, IdcProvider, RefreshMetadata};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::{Emitter, State};


// ===== 辅助函数 =====

/// 从 clientSecret JWT 中提取 startUrl
fn extract_start_url_from_client_secret(client_secret: &str) -> Option<String> {
    use base64::{engine::general_purpose, Engine as _};

    // JWT 格式：header.payload.signature
    let parts: Vec<&str> = client_secret.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    // Base64 解码 payload
    let payload = parts[1];
    let decoded = general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
    let payload_str = String::from_utf8(decoded).ok()?;

    // 解析 JSON
    let payload_json: serde_json::Value = serde_json::from_str(&payload_str).ok()?;
    let serialized_str = payload_json.get("serialized")?.as_str()?;
    let serialized: serde_json::Value = serde_json::from_str(serialized_str).ok()?;

    // 提取 initiateLoginUri
    serialized
        .get("initiateLoginUri")?
        .as_str()
        .map(|s| s.to_string())
}

/// 根据 startUrl 计算 clientIdHash（与 Kiro IDE 源码一致）
fn calculate_client_id_hash(start_url: &str) -> String {
    let input = format!(r#"{{"startUrl":"{start_url}"}}"#);
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

fn resolve_builder_client_id_hash(
    client_id_hash: Option<String>,
    start_url: Option<&str>,
) -> String {
    client_id_hash.unwrap_or_else(|| {
        calculate_client_id_hash(start_url.unwrap_or("https://view.awsapps.com/start"))
    })
}

// ===== 数据结构 =====


/// account_cmd 的扩展：在通用 token 应用之上额外做缓存清理 / status 重置
fn apply_refreshed_account_tokens(account: &mut Account, refresh: &RefreshResult) {
    clear_available_models_cache(account);
    crate::commands::common::apply_refreshed_account_tokens(account, refresh);
    account.status = "active".to_string();
}


// ============================================================
// 筛选查询命令
// ============================================================


// ============================================================
// 配额查询接口
// ============================================================


#[cfg(test)]
mod tests {
    use super::resolve_builder_client_id_hash;

    #[test]
    fn resolve_builder_client_id_hash_prefers_explicit_hash() {
        let resolved = resolve_builder_client_id_hash(
            Some("provided-hash".to_string()),
            Some("https://example.awsapps.com/start"),
        );

        assert_eq!(resolved, "provided-hash");
    }

    #[test]
    fn resolve_builder_client_id_hash_uses_start_url_when_hash_missing() {
        let start_url = "https://example.awsapps.com/start";
        let resolved = resolve_builder_client_id_hash(None, Some(start_url));

        assert_eq!(resolved, super::calculate_client_id_hash(start_url));
    }

    #[test]
    fn resolve_builder_client_id_hash_falls_back_to_default_start_url() {
        let resolved = resolve_builder_client_id_hash(None, None);

        assert_eq!(
            resolved,
            super::calculate_client_id_hash("https://view.awsapps.com/start")
        );
    }
}

// ============================================================
// Token 状态检查接口（参考 Kiro IDE 源码）
// ============================================================



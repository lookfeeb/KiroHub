use super::*;

#[derive(Serialize)]
pub struct SyncAccountResult {
    pub account: Account,
    pub warning: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAccountParams {
    pub id: String,
    pub label: Option<String>,
    pub status: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub machine_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyAccountResponse {
    #[serde(rename = "usageData")]
    pub usage_data: serde_json::Value, // 直接返回原始数据，前端解析
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
}

/// 添加账号的返回结果（包含是否新增）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddAccountResult {
    pub account: Account,
    #[serde(rename = "isNew")]
    pub is_new: bool, // true = 新增，false = 更新
}

/// `verify_account` 命令参数
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyAccountParams {
    #[allow(dead_code)]
    pub access_token: String,
    pub refresh_token: String,
    pub provider: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub region: Option<String>,
}

/// `IdC` 账号添加参数
pub(crate) struct IdcAccountParams {
    pub(crate) refresh_token: String,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
    pub(crate) region: Option<String>,
    pub(crate) machine_id: Option<String>,
    pub(crate) access_token: Option<String>,
    pub(crate) password: Option<String>,
    pub(crate) provider_id: String,
    pub(crate) start_url: Option<String>,
    pub(crate) client_id_hash: Option<String>,
}

/// Token 状态检查响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckTokenStatusResponse {
    pub status: String, // "active" | "expiring_soon" | "expired" | "invalid"
    pub expires_at: String,
    pub expires_in_seconds: i64,
    pub needs_refresh: bool,
}

/// Token 状态汇总
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatusSummary {
    pub total: usize,
    pub active: usize,
    pub expiring_soon: usize,
    pub expired: usize,
    pub invalid: usize,
    pub accounts_need_refresh: Vec<AccountRefreshInfo>,
}

/// 需要刷新的账号信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRefreshInfo {
    pub id: String,
    pub email: Option<String>,
    pub provider: String,
    pub expires_at: String,
    pub expires_in_seconds: i64,
}

/// 批量刷新响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshAllResponse {
    pub total_attempted: usize,
    pub successful: usize,
    pub failed: usize,
    pub skipped: usize,
    pub results: Vec<RefreshResultItem>,
}

/// 单个刷新结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResultItem {
    pub id: String,
    pub email: Option<String>,
    pub success: bool,
    pub message: String,
}

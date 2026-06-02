// Kiro Web Portal 客户端 - CBOR API
// 直接返回 JSON Value，不依赖复杂的结构体

use crate::clients::http_client::build_http_client;
use serde::Serialize;

const KIRO_WEB_PORTAL: &str = "https://app.kiro.dev";

pub fn cbor_encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(value).map_err(|e| format!("CBOR encode error: {e}"))
}

#[allow(dead_code)]
pub fn cbor_decode<T: for<'de> serde::Deserialize<'de>>(data: &[u8]) -> Result<T, String> {
    serde_cbor::from_slice(data).map_err(|e| format!("CBOR decode error: {e}"))
}

// ============================================================
// 请求结构体
// ============================================================

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct GetUserUsageAndLimitsRequest {
    #[serde(rename = "isEmailRequired")]
    is_email_required: bool,
    origin: String,
}

// ============================================================
// KiroPortalClient
// ============================================================

#[allow(dead_code)]
pub struct KiroPortalClient {
    client: reqwest::Client,
    endpoint: String,
}

impl KiroPortalClient {
    #[allow(dead_code)]
    pub fn new() -> Result<Self, String> {
        let client = build_http_client()?;

        Ok(Self {
            client,
            endpoint: KIRO_WEB_PORTAL.to_string(),
        })
    }

    /// 获取用户配额和用量信息（直接返回 JSON Value）
    #[allow(dead_code)]
    pub async fn get_user_usage_and_limits(
        &self,
        access_token: &str,
        idp: &str,
    ) -> Result<serde_json::Value, String> {
        let url = format!(
            "{}/service/KiroWebPortalService/operation/GetUserUsageAndLimits",
            self.endpoint
        );

        let request = GetUserUsageAndLimitsRequest {
            is_email_required: true,
            origin: "KIRO_IDE".to_string(),
        };

        let body = cbor_encode(&request)?;
        let cookie = format!("Idp={idp}; AccessToken={access_token}");

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/cbor")
            .header("Accept", "application/cbor")
            .header("smithy-protocol", "rpc-v2-cbor")
            .header("authorization", format!("Bearer {access_token}"))
            .header("Cookie", cookie)
            .body(body)
            .send()
            .await
            .map_err(|e| format!("请求失败（可能是并发请求过多被限流）: {}，请稍后重试", e))?;

        let status = response.status();
        let status_code = status.as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read response: {e}"))?;

        if !status.is_success() {
            let error_msg = if let Ok(error) = cbor_decode::<serde_json::Value>(&bytes) {
                serde_json::to_string_pretty(&error)
                    .unwrap_or_else(|err| format!("<错误响应序列化失败: {err}>"))
            } else {
                String::from_utf8_lossy(&bytes).to_string()
            };

            // 429 → 请求过多，被限流
            if status_code == 429 {
                return Err(format!("HTTP {}: 请求过多，已被限流，请稍后重试", status_code));
            }

            // 401 → token 过期，需要刷新（不打印日志，这是正常流程）
            if status_code == 401 {
                return Err(format!("AUTH_ERROR: HTTP {} - {}", status_code, error_msg));
            }

            // 423 Locked + AccountSuspendedException → 封禁（不打印日志）
            if status_code == 423 {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&error_msg) {
                    let type_field = parsed.get("__type").and_then(|t| t.as_str()).unwrap_or("");
                    if type_field.contains("AccountSuspendedException") {
                        let message = parsed
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("账号已被暂停");
                        return Err(format!("BANNED: {message}"));
                    }
                }
            }

            // 403 处理
            if status_code == 403 {
                let error_msg_lower = error_msg.to_lowercase();
                // 解析 JSON 检查 reason 字段
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&error_msg) {
                    let reason = parsed.get("reason").and_then(|r| r.as_str()).unwrap_or("");
                    let message = parsed
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("账号已被封禁");

                    // 403 + reason 为 TemporarilySuspended → 封禁
                    if reason == "TemporarilySuspended" {
                        return Err(format!("BANNED: {message}"));
                    }
                }
                // 检查消息中是否包含 "suspended" 关键词
                if error_msg_lower.contains("suspended") {
                    return Err(format!("BANNED: {error_msg}"));
                }
                // 403 + 其他情况 → token 无效，需要刷新
                return Err(format!("AUTH_ERROR: HTTP {} - {}", status_code, error_msg));
            }

            return Err(format!(
                "GetUserUsageAndLimits failed - HTTP {}: {}",
                status_code, error_msg
            ));
        }

        // 直接解码为 JSON Value，不经过结构体
        let decoded = cbor_decode::<serde_json::Value>(&bytes)?;
        
        // 如果解码成功但数据为 null，添加调试信息
        if decoded.is_null() {
            return Err(format!(
                "API 返回成功（HTTP {}）但数据为 null。原始响应：{:?}",
                status_code,
                String::from_utf8_lossy(&bytes)
            ));
        }
        
        Ok(decoded)
    }
}

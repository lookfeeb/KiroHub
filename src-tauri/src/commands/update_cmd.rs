// 更新检查命令 - 支持代理

#![allow(clippy::needless_pass_by_value)] // Tauri 命令需要按值传递参数

use reqwest::Proxy;
use serde::{Deserialize, Serialize};

const UPDATE_URL_DEFAULT: &str =
    "https://github.com/lookfeeb/KiroHub/releases/latest/download/latest.json";
#[cfg(target_os = "linux")]
const UPDATE_URL_DEB: &str =
    "https://github.com/lookfeeb/KiroHub/releases/latest/download/latest-deb.json";
#[cfg(target_os = "linux")]
const UPDATE_URL_RPM: &str =
    "https://github.com/lookfeeb/KiroHub/releases/latest/download/latest-rpm.json";

/// 检测 Linux 安装方式
#[cfg(target_os = "linux")]
fn detect_linux_install_type() -> &'static str {
    if let Ok(exe_path) = std::env::current_exe() {
        let path_str = exe_path.to_string_lossy();
        // deb/rpm 安装通常在 /usr/bin 或 /opt
        if path_str.starts_with("/usr/") || path_str.starts_with("/opt/") {
            // 检查是 deb 还是 rpm 系统
            if std::path::Path::new("/etc/debian_version").exists() {
                return "deb";
            }
            if std::path::Path::new("/etc/redhat-release").exists()
                || std::path::Path::new("/etc/fedora-release").exists()
            {
                return "rpm";
            }
            // 默认 deb
            return "deb";
        }
        if path_str.contains(".mount_") || path_str.contains("AppImage") {
            return "appimage";
        }
    }
    "appimage"
}

/// 获取更新 URL
fn get_update_url() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        match detect_linux_install_type() {
            "deb" => return UPDATE_URL_DEB,
            "rpm" => return UPDATE_URL_RPM,
            _ => {}
        }
    }
    UPDATE_URL_DEFAULT
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub pub_date: String,
    pub platforms: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub notes: Option<String>,
    pub download_url: Option<String>,
}

fn extract_http_proxy_from_json(json: &serde_json::Value) -> Option<String> {
    json.get("http.proxy")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(std::string::ToString::to_string)
}

/// 获取 Kiro IDE 设置中的代理
fn get_proxy_from_kiro_settings() -> Option<String> {
    #[cfg(target_os = "windows")]
    let path = std::env::var("APPDATA").ok().map(|appdata| {
        std::path::PathBuf::from(appdata)
            .join("Kiro")
            .join("User")
            .join("settings.json")
    });

    #[cfg(target_os = "macos")]
    let path = std::env::var("HOME").ok().map(|home| {
        std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Kiro")
            .join("User")
            .join("settings.json")
    });

    #[cfg(target_os = "linux")]
    let path = std::env::var("HOME").ok().map(|home| {
        std::path::PathBuf::from(home)
            .join(".config")
            .join("Kiro")
            .join("User")
            .join("settings.json")
    });

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let path: Option<std::path::PathBuf> = None;

    path.and_then(|p| {
        if p.exists() {
            std::fs::read_to_string(&p).ok()
        } else {
            None
        }
    })
    .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
    .and_then(|json| extract_http_proxy_from_json(&json))
}

/// 构建 HTTP 客户端（支持代理）
fn build_http_client() -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30));

    // 尝试从 Kiro 设置获取代理
    if let Some(proxy_url) = get_proxy_from_kiro_settings() {
        let proxy = Proxy::all(&proxy_url).map_err(|e| format!("代理配置错误: {e}"))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

/// 获取当前平台的下载 URL
fn get_download_url_for_platform(
    platforms: &serde_json::Value,
    platform_key: &str,
) -> Option<String> {
    platforms
        .get(platform_key)
        .and_then(|platform| platform.get("url"))
        .and_then(serde_json::Value::as_str)
        .map(std::string::ToString::to_string)
}

fn get_platform_download_url(platforms: &serde_json::Value) -> Option<String> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    let platform_key = "windows-x86_64-nsis";

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    let platform_key = "darwin-x86_64";

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let platform_key = "darwin-aarch64";

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let platform_key = "linux-x86_64";

    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )))]
    let platform_key = "";

    get_download_url_for_platform(platforms, platform_key)
}

#[tauri::command]
pub async fn check_update() -> Result<UpdateCheckResult, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    let client = build_http_client()?;
    let update_url = get_update_url();

    let response = client
        .get(update_url)
        .send()
        .await
        .map_err(|e| format!("请求更新信息失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("服务器返回错误: {}", response.status()));
    }

    let update_info: UpdateInfo = response
        .json()
        .await
        .map_err(|e| format!("解析更新信息失败: {e}"))?;

    // 比较版本号
    let has_update = compare_versions(&current_version, &update_info.version);

    let download_url = if has_update {
        get_platform_download_url(&update_info.platforms)
    } else {
        None
    };

    Ok(UpdateCheckResult {
        has_update,
        current_version,
        latest_version: Some(update_info.version),
        notes: Some(update_info.notes),
        download_url,
    })
}

/// 比较版本号，返回 true 表示有新版本
fn parse_version_parts(version: &str) -> Vec<u32> {
    version
        .trim_start_matches('v')
        .split('.')
        .filter_map(|segment| segment.parse().ok())
        .collect()
}

fn compare_versions(current: &str, latest: &str) -> bool {
    let current_parts = parse_version_parts(current);
    let latest_parts = parse_version_parts(latest);

    for i in 0..std::cmp::max(current_parts.len(), latest_parts.len()) {
        let c = current_parts.get(i).copied().unwrap_or(0);
        let l = latest_parts.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        } else if l < c {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        compare_versions, extract_http_proxy_from_json, get_download_url_for_platform,
        parse_version_parts,
    };

    #[test]
    fn parse_version_parts_ignores_prefix_and_invalid_segments() {
        assert_eq!(parse_version_parts("v1.2.3"), vec![1, 2, 3]);
        assert_eq!(parse_version_parts("1.2.beta.4"), vec![1, 2, 4]);
    }

    #[test]
    fn compare_versions_treats_missing_segments_as_zero() {
        assert!(compare_versions("1.2.3", "1.2.4"));
        assert!(!compare_versions("1.2.3", "v1.2.3"));
        assert!(!compare_versions("1.2.3", "1.2.3.0"));
        assert!(compare_versions("1.2", "1.2.0.1"));
    }

    #[test]
    fn extract_http_proxy_from_json_reads_non_empty_proxy_only() {
        let json = serde_json::json!({
            "http.proxy": "http://127.0.0.1:7890"
        });
        let empty = serde_json::json!({
            "http.proxy": ""
        });

        assert_eq!(
            extract_http_proxy_from_json(&json),
            Some("http://127.0.0.1:7890".to_string())
        );
        assert_eq!(extract_http_proxy_from_json(&empty), None);
        assert_eq!(extract_http_proxy_from_json(&serde_json::json!({})), None);
    }

    #[test]
    fn get_download_url_for_platform_reads_nested_url_only() {
        let platforms = serde_json::json!({
            "windows-x86_64-nsis": {
                "url": "https://example.com/app.exe"
            },
            "linux-x86_64": {}
        });

        assert_eq!(
            get_download_url_for_platform(&platforms, "windows-x86_64-nsis"),
            Some("https://example.com/app.exe".to_string())
        );
        assert_eq!(
            get_download_url_for_platform(&platforms, "linux-x86_64"),
            None
        );
        assert_eq!(get_download_url_for_platform(&platforms, "missing"), None);
    }
}

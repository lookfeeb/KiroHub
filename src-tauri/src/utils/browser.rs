// 浏览器打开工具

use crate::commands::app_settings_cmd::get_browser_path;
use serde::Serialize;

/// 打开浏览器访问指定 URL
/// 如果用户配置了自定义浏览器路径，则使用自定义浏览器
/// 否则使用系统默认浏览器
pub fn open_browser(url: &str) -> Result<(), String> {
    if let Some(browser_path) = get_browser_path() {
        open_with_custom_browser(&browser_path, url)
    } else {
        open_with_default_browser(url)
    }
}

/// 打开浏览器但强制去除无痕/隐私模式参数（用于 MCP OAuth 授权，需复用已登录会话）。
/// 仅过滤已知无痕 flag，其余自定义参数保留；未配置自定义浏览器时回退系统默认浏览器。
pub fn open_browser_keep_session(url: &str) -> Result<(), String> {
    let Some(browser_path) = get_browser_path() else {
        return open_with_default_browser(url);
    };
    let (exe_path, mut args) = parse_browser_command(&browser_path)?;
    args.retain(|a| {
        !matches!(
            a.as_str(),
            "--incognito" | "--inprivate" | "-private-window" | "-private" | "--private"
        )
    });
    args.push(url.to_string());
    std::process::Command::new(&exe_path)
        .args(&args)
        .spawn()
        .map_err(|e| format!("打开自定义浏览器失败: {e} (路径: {exe_path})"))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedBrowser {
    pub name: String,
    pub path: String,
    pub incognito_arg: String,
}

/// 检测系统中安装的浏览器
#[cfg(target_os = "windows")]
pub fn detect_browsers() -> Vec<DetectedBrowser> {
    use std::path::Path;

    let browsers = vec![
        (
            "Chrome",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            "--incognito",
        ),
        (
            "Chrome (x86)",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            "--incognito",
        ),
        (
            "Edge",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            "--inprivate",
        ),
        (
            "Edge",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
            "--inprivate",
        ),
        (
            "Firefox",
            r"C:\Program Files\Mozilla Firefox\firefox.exe",
            "-private-window",
        ),
        (
            "Firefox (x86)",
            r"C:\Program Files (x86)\Mozilla Firefox\firefox.exe",
            "-private-window",
        ),
        (
            "Brave",
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
            "--incognito",
        ),
        (
            "Brave (x86)",
            r"C:\Program Files (x86)\BraveSoftware\Brave-Browser\Application\brave.exe",
            "--incognito",
        ),
    ];

    let mut detected = Vec::new();
    for (name, path, incognito_arg) in browsers {
        if Path::new(path).exists() {
            detected.push(DetectedBrowser {
                name: name.to_string(),
                path: path.to_string(),
                incognito_arg: incognito_arg.to_string(),
            });
        }
    }
    detected
}

#[cfg(target_os = "macos")]
pub fn detect_browsers() -> Vec<DetectedBrowser> {
    use std::path::Path;

    let browsers = vec![
        (
            "Chrome",
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "--incognito",
        ),
        (
            "Firefox",
            "/Applications/Firefox.app/Contents/MacOS/firefox",
            "-private-window",
        ),
        (
            "Safari",
            "/Applications/Safari.app/Contents/MacOS/Safari",
            "",
        ),
        (
            "Edge",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "--inprivate",
        ),
        (
            "Brave",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "--incognito",
        ),
    ];

    let mut detected = Vec::new();
    for (name, path, incognito_arg) in browsers {
        if Path::new(path).exists() {
            detected.push(DetectedBrowser {
                name: name.to_string(),
                path: path.to_string(),
                incognito_arg: incognito_arg.to_string(),
            });
        }
    }
    detected
}

#[cfg(target_os = "linux")]
pub fn detect_browsers() -> Vec<DetectedBrowser> {
    use std::path::Path;

    let browsers = vec![
        ("Chrome", "/usr/bin/google-chrome", "--incognito"),
        ("Chrome", "/usr/bin/google-chrome-stable", "--incognito"),
        ("Chromium", "/usr/bin/chromium", "--incognito"),
        ("Chromium", "/usr/bin/chromium-browser", "--incognito"),
        ("Firefox", "/usr/bin/firefox", "-private-window"),
        ("Edge", "/usr/bin/microsoft-edge", "--inprivate"),
        ("Edge", "/usr/bin/microsoft-edge-stable", "--inprivate"),
        ("Brave", "/usr/bin/brave-browser", "--incognito"),
        ("Brave", "/usr/bin/brave", "--incognito"),
    ];

    let mut detected = Vec::new();
    for (name, path, incognito_arg) in browsers {
        if Path::new(path).exists() {
            detected.push(DetectedBrowser {
                name: name.to_string(),
                path: path.to_string(),
                incognito_arg: incognito_arg.to_string(),
            });
        }
    }
    detected
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn detect_browsers() -> Vec<DetectedBrowser> {
    Vec::new()
}

/// 使用自定义浏览器打开 URL
/// `browser_path` 格式: "路径" 参数1 参数2... 或 路径 参数1 参数2...
/// 例如: "C:\Program Files\Google\Chrome\Application\chrome.exe" --incognito
fn open_with_custom_browser(browser_path: &str, url: &str) -> Result<(), String> {
    let (exe_path, mut args) = parse_browser_command(browser_path)?;
    args.push(url.to_string());

    std::process::Command::new(&exe_path)
        .args(&args)
        .spawn()
        .map_err(|e| format!("打开自定义浏览器失败: {e} (路径: {exe_path})"))?;

    Ok(())
}

fn parse_browser_command(browser_path: &str) -> Result<(String, Vec<String>), String> {
    let browser_path = browser_path.trim();
    if browser_path.is_empty() {
        return Err("浏览器路径为空".to_string());
    }

    if let Some(stripped) = browser_path.strip_prefix('"') {
        if let Some(end_quote) = stripped.find('"') {
            let path = stripped[..end_quote].to_string();
            let remaining = stripped[end_quote + 1..].trim();
            let args = if remaining.is_empty() {
                Vec::new()
            } else {
                remaining.split_whitespace().map(str::to_string).collect()
            };
            return Ok((path, args));
        }

        return Ok((browser_path.trim_matches('"').to_string(), Vec::new()));
    }

    let parts: Vec<&str> = browser_path.split_whitespace().collect();
    if parts.is_empty() {
        return Err("浏览器路径为空".to_string());
    }

    let arg_start = parts
        .iter()
        .position(|part| part.starts_with('-'))
        .unwrap_or(parts.len());
    let exe_path = parts[..arg_start].join(" ");
    let args = parts[arg_start..]
        .iter()
        .map(|part| (*part).to_string())
        .collect();

    Ok((exe_path, args))
}

/// 使用系统默认浏览器打开 URL
fn open_with_default_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|e| format!("打开浏览器失败: {e}"))?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        open::that(url).map_err(|e| format!("打开浏览器失败: {e}"))?;
    }

    Ok(())
}

// ===== Tauri Command =====

#[tauri::command]
pub async fn detect_installed_browsers() -> Vec<DetectedBrowser> {
    detect_browsers()
}

#[cfg(test)]
mod tests {
    use super::parse_browser_command;

    #[test]
    fn parse_browser_command_keeps_unquoted_windows_path_with_spaces() {
        let (path, args) =
            parse_browser_command(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
                .expect("path should parse");

        assert_eq!(
            path,
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        );
        assert!(args.is_empty());
    }

    #[test]
    fn parse_browser_command_splits_flags_after_unquoted_path() {
        let (path, args) = parse_browser_command(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe --incognito --profile-directory=Default",
        )
        .expect("path with args should parse");

        assert_eq!(
            path,
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        );
        assert_eq!(args, vec!["--incognito", "--profile-directory=Default"]);
    }

    #[test]
    fn parse_browser_command_supports_quoted_path_and_flags() {
        let (path, args) = parse_browser_command(
            r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --incognito"#,
        )
        .expect("quoted path should parse");

        assert_eq!(
            path,
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        );
        assert_eq!(args, vec!["--incognito"]);
    }
}

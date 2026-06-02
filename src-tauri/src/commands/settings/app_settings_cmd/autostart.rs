use super::shared::run_blocking_io;

const AUTOSTART_APP_NAME: &str = "KiroHub";

#[cfg(windows)]
fn windows_autostart_key() -> Result<winreg::RegKey, String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            KEY_READ | KEY_SET_VALUE,
        )
        .map_err(|e| format!("打开开机自启注册表失败: {e}"))?;
    Ok(key)
}

#[cfg(windows)]
fn current_exe_autostart_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("获取当前程序路径失败: {e}"))?;
    Ok(format!("\"{}\"", exe.display()))
}

#[cfg(windows)]
fn get_autostart_enabled_inner() -> Result<bool, String> {
    let key = windows_autostart_key()?;
    Ok(key.get_value::<String, _>(AUTOSTART_APP_NAME).is_ok())
}

#[cfg(windows)]
fn set_autostart_enabled_inner(enabled: bool) -> Result<(), String> {
    let key = windows_autostart_key()?;
    if enabled {
        key.set_value(AUTOSTART_APP_NAME, &current_exe_autostart_command()?)
            .map_err(|e| format!("写入开机自启失败: {e}"))?;
    } else {
        match key.delete_value(AUTOSTART_APP_NAME) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("移除开机自启失败: {e}")),
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn get_autostart_enabled_inner() -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(windows))]
fn set_autostart_enabled_inner(_enabled: bool) -> Result<(), String> {
    Err("当前系统暂不支持在这里设置开机自启".to_string())
}

#[tauri::command]
pub async fn get_autostart_enabled() -> Result<bool, String> {
    run_blocking_io(get_autostart_enabled_inner).await
}

#[tauri::command]
pub async fn set_autostart_enabled(enabled: bool) -> Result<bool, String> {
    run_blocking_io(move || {
        set_autostart_enabled_inner(enabled)?;
        get_autostart_enabled_inner()
    })
    .await
}

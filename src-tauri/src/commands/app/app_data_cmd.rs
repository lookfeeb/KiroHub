use tauri::AppHandle;

/// 备份应用数据库到 `<data_dir>/.kirohub/backups/kirohub-<时间戳>.db`
///
/// 使用 `VACUUM INTO` 生成 WAL 一致性单文件快照，返回备份文件路径。
#[tauri::command]
pub fn backup_app_database() -> Result<String, String> {
    let backups_dir = dirs::data_dir()
        .ok_or_else(|| "无法获取数据目录".to_string())?
        .join(".kirohub")
        .join("backups");
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = backups_dir.join(format!("kirohub-{ts}.db"));
    crate::db::backup_to(&dest)?;
    Ok(dest.to_string_lossy().to_string())
}

/// 获取应用数据目录路径
#[tauri::command]
pub fn get_app_data_dir(_app: AppHandle) -> Result<String, String> {
    // Windows: C:\Users\{username}\AppData\Roaming\.kirohub
    // macOS: ~/Library/Application Support/.kirohub
    // Linux: ~/.local/share/.kirohub
    let app_data_dir = dirs::data_dir()
        .ok_or_else(|| "Failed to get data directory".to_string())?
        .join(".kirohub");
    
    Ok(app_data_dir.to_string_lossy().to_string())
}

/// 使用系统文件管理器打开应用数据目录
#[tauri::command]
pub fn open_app_data_dir(_app: AppHandle) -> Result<(), String> {
    // Windows: C:\Users\{username}\AppData\Roaming\.kirohub
    // macOS: ~/Library/Application Support/.kirohub
    // Linux: ~/.local/share/.kirohub
    let app_data_dir = dirs::data_dir()
        .ok_or_else(|| "Failed to get data directory".to_string())?
        .join(".kirohub");
    
    // 确保目录存在
    if !app_data_dir.exists() {
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| format!("Failed to create app data dir: {}", e))?;
    }
    
    // 使用系统默认文件管理器打开目录
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(app_data_dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }
    
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(app_data_dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }
    
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(app_data_dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }
    
    Ok(())
}

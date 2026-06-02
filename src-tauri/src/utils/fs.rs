use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn atomic_write(path: &Path, content: &str, label: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 {label} 目录失败: {e}"))?;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp_path = path.with_extension(format!(
        "{}.{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("file"),
        timestamp
    ));

    fs::write(&tmp_path, content).map_err(|e| format!("写入 {label} 临时文件失败: {e}"))?;
    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("替换 {label} 失败: {e}")
    })
}

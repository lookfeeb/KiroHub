// macOS 平台机器码实现
// Kiro IDE 在 macOS 上会读取以下位置的机器码：
// 1. ~/Library/Application Support/Kiro/machineid - 主要机器码文件
// 2. ~/Library/Application Support/Kiro/User/globalStorage/storage.json - 遥测相关 ID

use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

use super::types::SystemMachineInfo;
use super::utils::*;
use crate::utils::fs::atomic_write;

/// 获取 Kiro IDE 数据目录
fn get_kiro_data_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Kiro")
    })
}

/// 获取 Kiro IDE 的 machineid 文件路径
fn get_kiro_machineid_path() -> Option<PathBuf> {
    get_kiro_data_dir().map(|p| p.join("machineid"))
}

/// 获取 storage.json 路径
fn get_storage_json_path() -> Option<PathBuf> {
    get_kiro_data_dir().map(|p| p.join("User").join("globalStorage").join("storage.json"))
}

/// 读取硬件 UUID
fn read_hardware_uuid() -> Result<String, String> {
    let output = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .map_err(|e| format!("执行 ioreg 失败: {}", e))?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|l| l.contains("IOPlatformUUID"))
        .and_then(|l| l.split('"').nth(3).map(|s| s.to_lowercase()))
        .ok_or_else(|| "无法获取 IOPlatformUUID".to_string())
}

/// 计算 SHA256 哈希（用于 telemetry.machineId）
fn sha256_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 写入所有 Kiro IDE 机器码相关文件
fn write_all_machine_ids(machine_id: &str) -> Result<(), String> {
    // 1. 写入 machineid 文件
    let machineid_path = get_kiro_machineid_path().ok_or("无法获取 Kiro machineid 路径")?;

    if let Some(parent) = machineid_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    atomic_write(&machineid_path, machine_id, "Kiro machineid")?;

    // 2. 更新 storage.json 中的遥测 ID
    if let Some(storage_path) = get_storage_json_path() {
        if storage_path.exists() {
            update_storage_json(&storage_path, machine_id)?;
        }
    }

    Ok(())
}

/// 更新 storage.json 中的机器码相关字段
fn update_storage_json(path: &PathBuf, machine_id: &str) -> Result<(), String> {
    let content = fs::read_to_string(path).map_err(|e| format!("读取 storage.json 失败: {}", e))?;

    let mut json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("解析 storage.json 失败: {}", e))?;

    if let Some(obj) = json.as_object_mut() {
        // 更新 telemetry.machineId（SHA256 哈希值）
        obj.insert(
            "telemetry.machineId".to_string(),
            serde_json::Value::String(sha256_hash(machine_id)),
        );

        // 更新 telemetry.devDeviceId（新的 UUID）
        obj.insert(
            "telemetry.devDeviceId".to_string(),
            serde_json::Value::String(Uuid::new_v4().to_string().to_lowercase()),
        );

        // 更新 telemetry.sqmId（新的 UUID，带大括号）
        obj.insert(
            "telemetry.sqmId".to_string(),
            serde_json::Value::String(format!("{{{}}}", Uuid::new_v4().to_string().to_uppercase())),
        );
    }

    let new_content = serde_json::to_string_pretty(&json)
        .map_err(|e| format!("序列化 storage.json 失败: {}", e))?;

    atomic_write(path, &new_content, "storage.json")?;

    Ok(())
}

pub fn get_system_machine_guid_inner() -> Result<SystemMachineInfo, String> {
    // 优先读取 Kiro IDE 的 machineid 文件
    let machine_guid = if let Some(path) = get_kiro_machineid_path() {
        if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| is_valid_machine_id(s))
        } else {
            None
        }
    } else {
        None
    }
    .map_or_else(|| read_hardware_uuid(), Ok)?;

    Ok(SystemMachineInfo {
        machine_guid: Some(machine_guid),
        os_type: "macos".to_string(),
        can_modify: true,
        requires_admin: false,
    })
}

pub fn reset_machine_guid_inner() -> Result<String, String> {
    let new_guid = Uuid::new_v4().to_string().to_lowercase();
    write_all_machine_ids(&new_guid)?;
    Ok(new_guid)
}

pub fn set_custom_machine_guid_inner(new_guid: String) -> Result<String, String> {
    if !is_valid_machine_id(&new_guid) {
        return Err("无效的机器码格式".to_string());
    }
    let formatted = new_guid.to_lowercase();
    write_all_machine_ids(&formatted)?;
    Ok(formatted)
}

pub fn clear_override_inner() -> Result<(), String> {
    // 删除 Kiro IDE 的 machineid 文件，恢复使用硬件 UUID
    if let Some(path) = get_kiro_machineid_path() {
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("删除 Kiro machineid 文件失败: {}", e))?;
        }
    }

    // 同时更新 storage.json，使用硬件 UUID
    if let Some(storage_path) = get_storage_json_path() {
        if storage_path.exists() {
            let hardware_uuid = read_hardware_uuid()?;
            update_storage_json(&storage_path, &hardware_uuid)?;
        }
    }

    Ok(())
}

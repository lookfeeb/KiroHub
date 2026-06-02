// Linux 平台机器码实现

use std::io::Write;
use std::process::{Command, Stdio};
use uuid::Uuid;

use super::types::SystemMachineInfo;
use super::utils::*;

const MACHINE_ID_PATHS: [&str; 2] = ["/etc/machine-id", "/var/lib/dbus/machine-id"];

fn read_machine_id() -> Result<String, String> {
    for path in &MACHINE_ID_PATHS {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Ok(content.trim().to_string());
        }
    }
    Err("无法获取 Linux 机器码".to_string())
}

fn write_with_pkexec(raw_id: &str) -> Result<(), String> {
    let mut child = Command::new("pkexec")
        .args(["tee", "/etc/machine-id"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("执行 pkexec 失败: {}", e))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(format!("{}\n", raw_id).as_bytes()).ok();
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("等待 pkexec 失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("dismissed") || stderr.contains("Not authorized") {
            return Err("用户取消了授权".to_string());
        }
        return Err(format!("写入失败: {}", stderr));
    }
    Ok(())
}

pub fn get_system_machine_guid_inner() -> Result<SystemMachineInfo, String> {
    Ok(SystemMachineInfo {
        machine_guid: Some(format_as_uuid(&read_machine_id()?)),
        os_type: "linux".to_string(),
        can_modify: true,
        requires_admin: true,
    })
}

pub fn reset_machine_guid_inner() -> Result<String, String> {
    let new_guid = Uuid::new_v4().to_string().to_lowercase();
    write_with_pkexec(&new_guid.replace("-", ""))?;
    Ok(new_guid)
}

pub fn set_custom_machine_guid_inner(new_guid: String) -> Result<String, String> {
    if !is_valid_machine_id(&new_guid) {
        return Err("无效的机器码格式".to_string());
    }
    let raw_id = new_guid.replace("-", "").to_lowercase();
    write_with_pkexec(&raw_id)?;
    Ok(format_as_uuid(&raw_id))
}

pub fn clear_override_inner() -> Result<(), String> {
    Ok(())
}

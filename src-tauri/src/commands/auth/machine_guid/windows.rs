// Windows 平台机器码实现

use uuid::Uuid;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_SET_VALUE};
use winreg::RegKey;

use super::types::SystemMachineInfo;
use super::utils::is_valid_machine_id;

fn read_registry() -> Result<String, String> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Cryptography")
        .map_err(|e| format!("无法打开注册表: {e}"))?
        .get_value("MachineGuid")
        .map_err(|e| format!("无法读取 MachineGuid: {e}"))
}

fn write_registry(value: &str) -> Result<(), String> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags("SOFTWARE\\Microsoft\\Cryptography", KEY_SET_VALUE)
        .map_err(|e| format!("无法打开注册表（需要管理员权限）: {e}"))?
        .set_value("MachineGuid", &value)
        .map_err(|e| format!("写入注册表失败（需要管理员权限）: {e}"))
}

pub fn get_system_machine_guid_inner() -> Result<SystemMachineInfo, String> {
    Ok(SystemMachineInfo {
        machine_guid: Some(read_registry()?),
        os_type: "windows".to_string(),
        can_modify: true,
        requires_admin: true,
    })
}

pub fn reset_machine_guid_inner() -> Result<String, String> {
    let new_guid = Uuid::new_v4().to_string().to_uppercase();
    write_registry(&new_guid)?;
    Ok(new_guid)
}

pub fn set_custom_machine_guid_inner(new_guid: String) -> Result<String, String> {
    if !is_valid_machine_id(&new_guid) {
        return Err("无效的机器码格式".to_string());
    }
    let formatted = new_guid.to_uppercase();
    write_registry(&formatted)?;
    Ok(formatted)
}

#[allow(clippy::unnecessary_wraps)] // 保持与其他平台的接口一致性
pub fn clear_override_inner() -> Result<(), String> {
    Ok(())
}

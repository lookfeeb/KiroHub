// 机器码工具函数

use uuid::Uuid;

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub fn get_macos_override_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_default()
        .join(".kirohub")
        .join("machine-id-override")
}

pub fn generate_random_machine_id() -> String {
    Uuid::new_v4().to_string().to_lowercase()
}

pub fn get_machine_id() -> String {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        super::platform::get_system_machine_guid_inner()
            .ok()
            .and_then(|i| i.machine_guid)
            .unwrap_or_else(generate_random_machine_id)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        generate_random_machine_id()
    }
}

pub fn is_valid_machine_id(id: &str) -> bool {
    let lower = id.to_lowercase();
    is_hex32(&lower) || is_uuid_like(&lower)
}

fn is_hex32(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_uuid_like(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }

    value.bytes().enumerate().all(|(idx, byte)| {
        if matches!(idx, 8 | 13 | 18 | 23) {
            byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    })
}

#[cfg(target_os = "linux")]
pub fn format_as_uuid(hex: &str) -> String {
    let c = hex.replace("-", "").to_lowercase();
    if c.len() != 32 {
        return c;
    }
    format!(
        "{}-{}-{}-{}-{}",
        &c[0..8],
        &c[8..12],
        &c[12..16],
        &c[16..20],
        &c[20..32]
    )
}


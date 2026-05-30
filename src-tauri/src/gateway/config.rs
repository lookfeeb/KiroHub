use super::*;

/// 根据模型映射规则解析实际模型名
pub fn resolve_model_mapping(config: &GatewayConfig, requested_model: &str) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ROUND_ROBIN: AtomicUsize = AtomicUsize::new(0);

    for rule in &config.model_mappings {
        if !rule.enabled {
            continue;
        }
        if rule.source_model != requested_model {
            continue;
        }
        if rule.target_models.is_empty() {
            continue;
        }

        match rule.rule_type.as_str() {
            "replace" | "alias" => {
                return rule.target_models[0].clone();
            }
            "loadbalance" => {
                if rule.weights.is_empty() || rule.weights.len() != rule.target_models.len() {
                    // 无权重或权重数量不匹配，简单轮询
                    let idx = ROUND_ROBIN.fetch_add(1, Ordering::Relaxed) % rule.target_models.len();
                    return rule.target_models[idx].clone();
                }
                // 加权轮询
                let total_weight: u32 = rule.weights.iter().sum();
                if total_weight == 0 {
                    return rule.target_models[0].clone();
                }
                let tick = ROUND_ROBIN.fetch_add(1, Ordering::Relaxed) as u32 % total_weight;
                let mut cumulative = 0u32;
                for (i, &w) in rule.weights.iter().enumerate() {
                    cumulative += w;
                    if tick < cumulative {
                        return rule.target_models[i].clone();
                    }
                }
                return rule.target_models[0].clone();
            }
            _ => {}
        }
    }

    requested_model.to_string()
}

pub(crate) fn build_bind_addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    let normalized = host.trim();
    if normalized.is_empty() {
        return Err("监听地址不能为空".to_string());
    }

    if normalized.eq_ignore_ascii_case("localhost") {
        return Ok(SocketAddr::from(([127, 0, 0, 1], port)));
    }

    let bind_target = if normalized.contains(':') {
        format!("[{normalized}]:{port}")
    } else {
        format!("{normalized}:{port}")
    };

    bind_target
        .parse::<SocketAddr>()
        .map_err(|e| format!("监听地址无效: {e}"))
}

pub(crate) fn effective_client_api_keys(config: &GatewayConfig) -> Vec<String> {
    let mut keys = Vec::new();

    if let Some(key) = config
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| !value.starts_with("#disabled#")) // 过滤禁用的 Key
    {
        keys.push(key.to_string());
    }

    for key in config
        .client_api_keys
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .filter(|item| !item.starts_with("#disabled#")) // 过滤禁用的 Key
    {
        if !keys.iter().any(|existing| existing == key) {
            keys.push(key.to_string());
        }
    }

    keys
}

pub(crate) fn ensure_config_valid(config: &GatewayConfig) -> Result<(), String> {
    build_bind_addr(&config.host, config.port)?;
    if config.port == 0 {
        return Err("端口必须大于 0".to_string());
    }

    let region = config.region.trim();
    if region.is_empty() {
        return Err("region 不能为空".to_string());
    }
    if !is_supported_kiro_region(region) {
        return Err(format!("region 不受支持: {region}"));
    }
    match config.account_mode.as_str() {
        "single"
            if config
                .account_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty() =>
        {
            return Err("single 模式必须选择账号".to_string());
        }
        "group"
            if config
                .group_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty() =>
        {
            return Err("group 模式必须选择分组".to_string());
        }
        "single" | "group" | "pool" => {}
        "local" => {
            return Err("反代不再支持 local 模式，请改用 single/group/pool 账号池模式".to_string());
        }
        _ => return Err("accountMode 必须是 single/group/pool".to_string()),
    }
    if !matches!(
        config.log_level.as_str(),
        "debug" | "info" | "warn" | "error"
    ) {
        return Err("logLevel 必须是 debug/info/warn/error".to_string());
    }
    if effective_client_api_keys(config).is_empty() {
        return Err("必须配置客户端 API Key".to_string());
    }
    if !config.local_only && config.allowed_ips.is_empty() {
        return Err("允许远程访问时必须至少配置一个白名单来源 IP".to_string());
    }
    for entry in &config.allowed_ips {
        if !is_valid_allowlist_entry(entry) {
            return Err(format!("白名单条目无效: {entry}"));
        }
    }
    Ok(())
}

pub(crate) fn is_valid_allowlist_entry(entry: &str) -> bool {
    let trimmed = entry.trim();
    !trimmed.is_empty()
        && (trimmed.parse::<IpAddr>().is_ok() || trimmed.parse::<ipnet::IpNet>().is_ok())
}

pub(crate) fn normalize_config(config: &GatewayConfig) -> GatewayConfig {
    let mut normalized = config.clone();
    normalized.host = normalized.host.trim().to_string();
    normalized.access_token = normalized
        .access_token
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    normalized.region = normalized.region.trim().to_string();
    normalized.account_mode = normalized.account_mode.trim().to_string();
    normalized.strategy = normalized.strategy.trim().to_string();
    normalized.log_level = normalized.log_level.trim().to_ascii_lowercase();
    normalized.client_api_keys = effective_client_api_keys(&normalized);
    normalized.access_token = normalized.client_api_keys.first().cloned();
    normalized.allowed_ips = normalized
        .allowed_ips
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .fold(Vec::new(), |mut acc, item| {
            if !acc.contains(&item) {
                acc.push(item);
            }
            acc
        });
    normalized
}

pub(crate) fn config_path() -> Result<PathBuf, String> {
    Ok(ensure_gateway_data_dir()?.join(CONFIG_FILE))
}

pub fn load_gateway_config() -> Result<GatewayConfig, String> {
    if let Some(json) = crate::db::kv_get("gateway_config", "config")? {
        let cfg = serde_json::from_str::<GatewayConfig>(&json)
            .map_err(|e| format!("解析配置失败: {e}"))?;
        return Ok(normalize_config(&cfg));
    }
    // 首次：迁移旧 gateway-config.json（若有）
    if let Some(cfg) = migrate_legacy_gateway_config() {
        let _ = save_gateway_config(&cfg);
        return Ok(normalize_config(&cfg));
    }
    Ok(GatewayConfig::default())
}

pub(crate) fn migrate_legacy_gateway_config() -> Option<GatewayConfig> {
    let path = config_path().ok()?;
    let content = fs::read_to_string(&path).ok()?;
    let cfg = serde_json::from_str::<GatewayConfig>(&content).ok()?;
    let _ = fs::rename(&path, path.with_extension("json.bak"));
    Some(cfg)
}

pub fn get_gateway_config() -> Result<GatewayConfig, String> {
    load_gateway_config()
}

pub fn save_gateway_config(config: &GatewayConfig) -> Result<(), String> {
    let normalized = normalize_config(config);
    ensure_config_valid(&normalized)?;
    let content =
        serde_json::to_string(&normalized).map_err(|e| format!("序列化配置失败: {e}"))?;
    crate::db::kv_set("gateway_config", "config", &content)
}

/// 启动时确保至少存在一个客户端 API Key：没有则自动生成一个并写回配置，已有则跳过
pub fn ensure_client_api_key_exists() -> Result<(), String> {
    let mut config = load_gateway_config()?;
    let has_key = config
        .client_api_keys
        .iter()
        .map(|key| key.trim())
        .any(|key| !key.is_empty() && !key.starts_with("#disabled#"));
    if has_key {
        return Ok(());
    }
    config
        .client_api_keys
        .push(format!("sk-{}", uuid::Uuid::new_v4().simple()));
    save_gateway_config(&config)
}

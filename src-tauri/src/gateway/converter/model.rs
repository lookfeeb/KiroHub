use super::*;

pub fn get_internal_model_id(external_model: &str) -> Result<String, String> {
    let normalized = normalize_external_model_alias(external_model);

    // 1. 特殊别名（简写 / latest / 特殊值）
    let model_id = match normalized.as_str() {
        "auto" | "default" => return Ok("auto".to_string()),
        "opus" | "opus-4-7" => return Ok("claude-opus-4.7".to_string()),
        "sonnet" | "sonnet-4-6" => return Ok("claude-sonnet-4.6".to_string()),
        "haiku" | "haiku-4-5" => return Ok("claude-haiku-4.5".to_string()),
        "claude-sonnet-latest" => return Ok("claude-sonnet-4.5".to_string()),
        // Claude 3.x 旧版兜底到 Claude 4.x
        "claude-3-5-sonnet" | "claude-3-5-sonnet-latest" | "claude-3-opus" | "claude-3-opus-latest" => {
            return Ok("claude-sonnet-4.5".to_string());
        }
        "claude-3-sonnet" => return Ok("claude-sonnet-4".to_string()),
        "claude-3-haiku" | "claude-3-5-haiku" => return Ok("claude-haiku-4.5".to_string()),
        // OpenAI GPT 兼容映射（默认全部映射到 sonnet-4.5；用户可以在前端「模型映射」配置里覆盖）
        "gpt-4" | "gpt-4o" | "gpt-4-turbo" | "gpt-3.5-turbo" | "gpt-4o-mini" => {
            return Ok("claude-sonnet-4.5".to_string());
        }
        // 开源模型别名
        "deepseek-3-2" | "deepseek-3.2" | "deepseek" => return Ok("deepseek-3.2".to_string()),
        "minimax-m2-5" | "minimax-m2.5" | "minimax" => return Ok("minimax-m2.5".to_string()),
        "minimax-m2-1" | "minimax-m2.1" => return Ok("minimax-m2.1".to_string()),
        "glm-5" | "glm5" => return Ok("glm-5".to_string()),
        "qwen3-coder-next" | "qwen3-coder" | "qwen3" | "qwen" => return Ok("qwen3-coder-next".to_string()),
        _ => &normalized,
    };

    // 2. 正则归一化：Anthropic 公开格式 → Kiro 内部格式
    //    claude-{family}-{major}-{minor}[-thinking][-日期] → claude-{family}-{major}.{minor}
    let normalized_model = normalize_claude_model_format(model_id);

    // 3. 兜底：如果归一化后仍然不像 Kiro 支持的格式，映射到默认 sonnet-4.5 避免直接 400
    //    向前兼容：claude-{sonnet|haiku|opus}-* 格式透传，假定 Kiro 后续新发布的版本格式不变
    if is_kiro_supported_model_format(&normalized_model) {
        Ok(normalized_model)
    } else {
        log::warn!(
            "[模型映射] 未知模型 \"{}\" → 兜底到 claude-sonnet-4.5",
            external_model
        );
        Ok("claude-sonnet-4.5".to_string())
    }
}


/// 判断模型 ID 是否符合 Kiro API 接受的格式
/// - claude-{sonnet|haiku|opus}-{version} （包括 4.5 / 4.6 / 4.7 + 未来新版本）
/// - 开源模型：deepseek-3.2 / minimax-m2.5 / minimax-m2.1 / glm-5 / qwen3-coder-next
/// - 特殊值：auto
pub(crate) fn is_kiro_supported_model_format(model: &str) -> bool {
    if model == "auto" {
        return true;
    }
    if model.starts_with("claude-sonnet-")
        || model.starts_with("claude-haiku-")
        || model.starts_with("claude-opus-")
    {
        return true;
    }
    matches!(
        model,
        "deepseek-3.2" | "minimax-m2.5" | "minimax-m2.1" | "glm-5" | "qwen3-coder-next"
    )
}


/// 将 Anthropic 公开模型名归一化为 Kiro 内部格式
///
/// 规则：
/// - 去掉日期后缀 -20xxxxxx（8位数字）
/// - 版本号横杠转点号：claude-{family}-{major}-{minor} → claude-{family}-{major}.{minor}
/// - 保留 -thinking 后缀（Kiro 通过模型 ID 区分是否启用思考）
/// - 已经是点号格式的直接返回
pub(crate) fn normalize_claude_model_format(model: &str) -> String {
    let mut s = model.to_string();

    // 去掉 -thinking 后缀（thinking 通过系统提示注入启用，Kiro API 不接受带 -thinking 的模型 ID）
    if let Some(stripped) = s.strip_suffix("-thinking") {
        s = stripped.to_string();
    }

    // 去掉日期后缀（-20xxxxxx，8位数字）
    if s.len() > 9 {
        let tail = &s[s.len() - 9..];
        if tail.starts_with('-') && tail[1..].chars().all(|c| c.is_ascii_digit()) && tail[1..].starts_with("20") {
            s.truncate(s.len() - 9);
        }
    }

    // 版本号横杠转点号：claude-{family}-{major}-{minor} → claude-{family}-{major}.{minor}
    // 匹配模式：末尾是 -{digit}-{digit} 的情况
    if let Some(last_dash) = s.rfind('-') {
        let after_last = &s[last_dash + 1..];
        if after_last.len() == 1 && after_last.chars().all(|c| c.is_ascii_digit()) {
            // 检查倒数第二个 dash 后面是否也是单个数字
            let prefix = &s[..last_dash];
            if let Some(second_last_dash) = prefix.rfind('-') {
                let between = &prefix[second_last_dash + 1..];
                if between.len() == 1 && between.chars().all(|c| c.is_ascii_digit()) {
                    // claude-opus-4-7 → claude-opus-4.7
                    let base = &s[..second_last_dash + 1 + between.len()];
                    return format!("{}.{}", base, after_last);
                }
            }
        }
    }

    s
}


/// 带降级的模型映射函数
///
/// 根据账号可用模型列表（来自 ListAvailableModels API），自动将不可用的模型降级
///
/// ## 降级策略（基于 Kiro 订阅限制）
///
/// Free 用户可用模型：sonnet-4.5, sonnet-4, haiku-4.5, 开源模型
/// Free 用户不可用：所有 Opus 系列、Sonnet 4.6
///
/// 降级链：
/// - Opus 4.7 → Opus 4.6 → Opus 4.5 → Sonnet 4.5
/// - Sonnet 4.6 → Sonnet 4.5
pub fn get_internal_model_id_with_fallback(
    external_model: &str,
    available_models: &[String],
) -> Result<String, String> {
    let mapped_model = get_internal_model_id(external_model)?;

    // 检查是否在可用列表中
    if available_models.contains(&mapped_model) {
        return Ok(mapped_model);
    }

    // 降级策略：逐级降级直到找到可用模型
    let fallback = if mapped_model.contains("opus-4.7") {
        // Opus 4.7 → Opus 4.6 → Opus 4.5 → Sonnet 4.5
        if available_models.iter().any(|m| m.contains("opus-4.6")) {
            "claude-opus-4.6"
        } else if available_models.iter().any(|m| m.contains("opus-4.5")) {
            "claude-opus-4.5"
        } else {
            // Free 用户：Opus 全系列不可用，降级到 Sonnet 4.5
            "claude-sonnet-4.5"
        }
    } else if mapped_model.contains("opus-4.6") {
        // Opus 4.6 → Opus 4.5 → Sonnet 4.5
        if available_models.iter().any(|m| m.contains("opus-4.5")) {
            "claude-opus-4.5"
        } else {
            "claude-sonnet-4.5"
        }
    } else if mapped_model.contains("opus-4.5") {
        // Opus 4.5 → Sonnet 4.5（Free 用户场景）
        "claude-sonnet-4.5"
    } else if mapped_model.contains("sonnet-4.6") {
        // Sonnet 4.6 → Sonnet 4.5
        "claude-sonnet-4.5"
    } else {
        // 其他模型不降级，返回原模型（可能会在后续请求中失败）
        return Ok(mapped_model);
    };

    log::warn!(
        "[Gateway] 模型 {} 不在可用列表中，降级到 {}",
        mapped_model,
        fallback
    );

    Ok(fallback.to_string())
}


pub(crate) fn normalize_external_model_alias(external_model: &str) -> String {
    external_model.trim().to_ascii_lowercase()
}


/// 检测模型名是否包含 "thinking" 后缀，若包含则覆写 thinking 配置
///
/// 根据 Anthropic 官方文档 (https://platform.claude.com/docs/en/docs/about-claude/models):
///
/// **Adaptive Thinking** (type: "adaptive"):
/// - Claude Opus 4.7
/// - Claude Sonnet 4.6
///
/// **Extended Thinking** (type: "enabled"):
/// - Claude Haiku 4.5
/// - Claude Sonnet 4.5
/// - Claude Opus 4.5
///
/// budget_tokens 固定为 20000
pub(crate) fn override_thinking_from_model_name(request: &mut NormalizedRequest) {
    let model_lower = request.model.to_lowercase();
    if !model_lower.contains("thinking") {
        return;
    }

    // 判断是否支持 Adaptive Thinking
    let supports_adaptive =
        // Claude Opus 4.7
        (model_lower.contains("opus") && (model_lower.contains("4-7") || model_lower.contains("4.7")))
        ||
        // Claude Sonnet 4.6
        (model_lower.contains("sonnet") && (model_lower.contains("4-6") || model_lower.contains("4.6")));

    let thinking_type = if supports_adaptive {
        "adaptive"
    } else {
        "enabled"
    };

    log::info!(
        "[Gateway] 模型名 {} 包含 thinking 后缀，覆写 thinking 配置为 {}",
        request.model,
        thinking_type
    );

    use crate::gateway::models::Thinking;
    request.thinking = Some(Thinking {
        thinking_type: thinking_type.to_string(),
        budget_tokens: 20000,
    });
}


pub fn get_available_models() -> Vec<ModelInfo> {
    // 最后更新：2026-05-10
    // 数据来源：Kiro ListAvailableModels API 实际返回
    // API 返回的 modelId：auto, claude-opus-4.7, claude-opus-4.6, claude-sonnet-4.6,
    //   claude-opus-4.5, claude-sonnet-4.5, claude-sonnet-4, claude-haiku-4.5,
    //   deepseek-3.2, minimax-m2.5, minimax-m2.1, glm-5, qwen3-coder-next
    [
        // 自动选择
        "auto",
        // Claude 4.7 系列（目前仅 Opus 4.7）
        "claude-opus-4.7",
        "claude-opus-4.7-thinking",
        // Kiro API 暂未返回 Sonnet/Haiku 4.7，等上游支持后再加入。
        // "claude-sonnet-4.7",
        // "claude-sonnet-4.7-thinking",
        // "claude-haiku-4.7",
        // "claude-haiku-4.7-thinking",
        // Claude 4.6 系列（Opus 和 Sonnet）
        "claude-opus-4.6",
        "claude-opus-4.6-thinking",
        "claude-sonnet-4.6",
        "claude-sonnet-4.6-thinking",
        // Claude 4.5 系列
        "claude-opus-4.5",
        "claude-opus-4.5-thinking",
        "claude-sonnet-4.5",
        "claude-sonnet-4.5-thinking",
        "claude-haiku-4.5",
        "claude-haiku-4.5-thinking",
        // Claude 4 系列
        "claude-sonnet-4",
        "claude-sonnet-4-thinking",
        // 开源模型
        "deepseek-3.2",
        "minimax-m2.5",
        "minimax-m2.1",
        "glm-5",
        "qwen3-coder-next",
    ]
    .into_iter()
    .map(|id| ModelInfo {
        id: id.to_string(),
        object: "model".to_string(),
        created: 1_700_000_000,
        owned_by: "anthropic".to_string(),
    })
    .collect()
}

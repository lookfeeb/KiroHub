// ===== 详情 / 删除 / 路径 =====

/// 安全定位文件：拼接后 canonicalize 并校验在 root 内
fn safe_path(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.contains("..") {
        return None;
    }
    let target = root.join(rel);
    let croot = root.canonicalize().ok()?;
    let ctarget = target.canonicalize().ok()?;
    ctarget.starts_with(&croot).then_some(ctarget)
}

fn locate(hash: &str, session_id: &str) -> Option<(&'static SourceDef, PathBuf)> {
    let d = def_for(hash)?;
    let root = (d.root)()?;
    Some((d, safe_path(&root, session_id)?))
}

pub fn load_session(hash: &str, session_id: &str) -> anyhow::Result<IdeSession> {
    let (d, path) = locate(hash, session_id).ok_or_else(|| anyhow::anyhow!("非法的会话路径"))?;
    if matches!(d.layout, Layout::AntigravityIde) {
        let root = (d.root)().ok_or_else(|| anyhow::anyhow!("无法定位 Antigravity IDE 目录"))?;
        return load_antigravity_ide_session(d, &root, &path, session_id, hash);
    }

    if matches!(d.layout, Layout::Antigravity) {
        let bytes = read_bytes_capped(&path).ok_or_else(|| anyhow::anyhow!("无法读取会话文件"))?;
        let mut parsed = antigravity_strings_to_parsed(&bytes);
        if parsed.cwd.is_empty() {
            parsed.cwd = hash.strip_prefix(d.prefix).unwrap_or_default().to_string();
        }
        let mut history = Vec::new();
        if !parsed.cwd.is_empty() {
            history.push(history_item(
                "system",
                format!("工作目录：{}", parsed.cwd),
                0,
            ));
        }
        for (i, (role, text)) in parsed.blocks.iter().enumerate() {
            history.push(history_item(role, text.clone(), i + 1));
        }
        return Ok(IdeSession {
            session_id: session_id.to_string(),
            title: parsed.title,
            session_type: d.source.to_string(),
            workspace_directory: parsed.cwd,
            history,
            conversation_summary: None,
        });
    }

    let content_path = match d.layout {
        Layout::File { .. } => path,
        Layout::Antigravity | Layout::AntigravityIde => {
            return Err(anyhow::anyhow!("不支持的会话布局: {:?}", d.layout));
        }
    };
    let content = read_capped(&content_path).ok_or_else(|| anyhow::anyhow!("无法读取会话文件"))?;
    let parsed = (d.parse)(&content);
    let mut history = Vec::new();
    if !parsed.cwd.is_empty() {
        history.push(history_item(
            "system",
            format!("工作目录：{}", parsed.cwd),
            0,
        ));
    }
    for (i, (role, text)) in parsed.blocks.iter().enumerate() {
        history.push(history_item(role, text.clone(), i + 1));
    }
    Ok(IdeSession {
        session_id: session_id.to_string(),
        title: parsed.title,
        session_type: d.source.to_string(),
        workspace_directory: parsed.cwd,
        history,
        conversation_summary: None,
    })
}

fn history_item(role: &str, text: String, idx: usize) -> HistoryItem {
    HistoryItem {
        message: Message {
            role: role.to_string(),
            content: vec![ContentItem {
                content_type: "text".to_string(),
                text,
            }],
            is_hidden: false,
            id: format!("ext-{idx}"),
        },
        context_items: Vec::new(),
        editor_state: serde_json::Value::Null,
        prompt_logs: Vec::new(),
    }
}

pub fn file_path(hash: &str, session_id: &str) -> anyhow::Result<String> {
    let (d, path) = locate(hash, session_id).ok_or_else(|| anyhow::anyhow!("非法的会话路径"))?;
    let p = match d.layout {
        Layout::File { .. } => path,
        Layout::Antigravity => path,
        Layout::AntigravityIde => path,
    };
    // 去掉 Windows canonicalize 产生的 \\?\ 扩展长度前缀
    let s = p.to_string_lossy().to_string();
    Ok(s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s))
}

pub fn delete_session(hash: &str, session_id: &str) -> anyhow::Result<()> {
    let (d, path) = locate(hash, session_id).ok_or_else(|| anyhow::anyhow!("非法的会话路径"))?;
    let root = (d.root)().ok_or_else(|| anyhow::anyhow!("无法定位会话目录"))?;
    if d.source == "codex" {
        let result = delete_codex_session(&root, &path);
        invalidate_cache();
        return result;
    }
    if matches!(d.layout, Layout::Antigravity | Layout::AntigravityIde) {
        let result = delete_antigravity_session(d, &root, session_id);
        invalidate_cache();
        return result;
    }
    match d.layout {
        Layout::File { .. } => fs::remove_file(&path)?,
        Layout::Antigravity | Layout::AntigravityIde => unreachable!(),
    }
    invalidate_cache();
    Ok(())
}

/// 删除整个 workspace（该 cwd 下全部会话）
pub fn delete_workspace(hash: &str) -> anyhow::Result<()> {
    let d = def_for(hash).ok_or_else(|| anyhow::anyhow!("未知的会话来源"))?;
    let root = (d.root)().ok_or_else(|| anyhow::anyhow!("无法定位会话目录"))?;
    let workspace = hash.strip_prefix(d.prefix).unwrap_or_default();
    if d.source == "codex" {
        let result = delete_codex_workspace(&root, workspace);
        invalidate_cache();
        return result;
    }
    if matches!(d.layout, Layout::Antigravity | Layout::AntigravityIde) {
        let result = delete_antigravity_workspace(d, &root, workspace);
        invalidate_cache();
        return result;
    }

    let keys: Vec<String> = list_sessions(hash)
        .into_iter()
        .map(|s| s.session_id)
        .collect();
    let mut errors = Vec::new();
    for k in keys {
        if let Err(error) = delete_session(hash, &k) {
            errors.push(format!("{k}: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(errors.join("；")))
    }
}

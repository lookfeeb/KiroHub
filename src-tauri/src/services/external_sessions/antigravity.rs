fn antigravity_summary_index(root: &Path) -> HashMap<String, (String, String)> {
    let Some(bytes) = read_bytes_capped(&root.join("agyhub_summaries_proto.pb")) else {
        return HashMap::new();
    };
    let strings = printable_strings(&bytes);
    let mut out = HashMap::new();

    for (i, item) in strings.iter().enumerate() {
        let Some(id) = extract_uuid(item) else { continue };
        if out.contains_key(&id) {
            continue;
        }

        let title = strings
            .iter()
            .skip(i + 1)
            .take(4)
            .map(|s| clean_proto_text(s))
            .find(|s| {
                !s.is_empty()
                    && extract_uuid(s).is_none()
                    && !s.starts_with("file://")
                    && !s.starts_with("https://")
                    && s != "master"
            })
            .unwrap_or_else(|| "Antigravity 会话".to_string());

        let cwd = strings
            .iter()
            .skip(i + 1)
            .take(24)
            .find(|s| s.contains("file:///"))
            .map(|s| {
                let start = s.find("file:///").unwrap_or(0);
                clean_file_uri(&s[start..])
            })
            .unwrap_or_default();

        out.insert(id, (truncate(&title, MAX_TITLE_CHARS), cwd));
    }

    out
}

fn antigravity_strings_to_parsed(bytes: &[u8]) -> Parsed {
    let strings = printable_strings(bytes);
    let mut p = Parsed { cwd: String::new(), title: String::new(), created: None, updated: None, blocks: Vec::new() };
    for s in strings {
        let text = clean_proto_text(&s);
        if p.cwd.is_empty() && text.contains("file:///") {
            let start = text.find("file:///").unwrap_or(0);
            p.cwd = clean_file_uri(&text[start..]);
            continue;
        }
        if !is_readable_proto_text(&text) {
            continue;
        }
        push_block(&mut p.blocks, "artifact", text);
        if p.blocks.len() >= 24 {
            break;
        }
    }
    p.title = title_from_any(&p.blocks);
    p
}

fn antigravity_ide_state_db() -> Option<PathBuf> {
    dirs::data_dir().map(|d| {
        d.join("Antigravity IDE")
            .join("User")
            .join("globalStorage")
            .join("state.vscdb")
    })
}

fn antigravity_ide_index() -> HashMap<String, AntigravityIdeIndexEntry> {
    let Some(db) = antigravity_ide_state_db().filter(|p| p.exists()) else {
        return HashMap::new();
    };
    let Ok(conn) = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return HashMap::new();
    };
    let Ok(value) = conn.query_row(
        "select value from ItemTable where key = ?1",
        ["antigravityUnifiedStateSync.trajectorySummaries"],
        |row| row.get::<_, String>(0),
    ) else {
        return HashMap::new();
    };
    let Ok(bytes) = general_purpose::STANDARD.decode(value) else {
        return HashMap::new();
    };

    let strings = printable_strings(&bytes);
    let mut out = HashMap::new();
    for (i, item) in strings.iter().enumerate() {
        let Some(id) = extract_uuid(item) else { continue };
        if out.contains_key(&id) {
            continue;
        }
        let window: Vec<String> = strings.iter().skip(i + 1).take(12).cloned().collect();
        let mut entry = window
            .iter()
            .find_map(|s| antigravity_ide_entry_from_base64(s))
            .unwrap_or_default();
        if entry.title.is_empty() {
            entry.title = window
                .iter()
                .map(|s| clean_proto_text(s))
                .find(|s| is_antigravity_ide_title_candidate(s))
                .unwrap_or_default();
        }
        if entry.cwd.is_empty() {
            entry.cwd = strings
            .iter()
            .skip(i + 1)
            .take(24)
                .find_map(|s| clean_file_uri_at(s))
            .unwrap_or_default();
        }
        out.insert(id, entry);
    }
    out
}

fn looks_base64_blob(value: &str) -> bool {
    value.len() > 120 && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
}

fn antigravity_ide_entry_from_base64(value: &str) -> Option<AntigravityIdeIndexEntry> {
    if !looks_base64_blob(value) {
        return None;
    }
    let bytes = general_purpose::STANDARD.decode(value).ok()?;
    let strings = printable_strings(&bytes);
    let title = strings
        .iter()
        .map(|s| clean_proto_text(s))
        .find(|s| is_antigravity_ide_title_candidate(s))
        .unwrap_or_default();
    let cwd = strings.iter().find_map(|s| clean_file_uri_at(s)).unwrap_or_default();
    if title.is_empty() && cwd.is_empty() {
        return None;
    }
    Some(AntigravityIdeIndexEntry { title, cwd })
}

fn is_antigravity_ide_title_candidate(value: &str) -> bool {
    let s = value.trim();
    !s.is_empty()
        && s.chars().count() <= MAX_TITLE_CHARS
        && extract_uuid(s).is_none()
        && !s.starts_with("file://")
        && !s.starts_with("http")
        && !looks_base64_blob(s)
        && !s.contains('<')
        && !s.contains('>')
        && !s.contains("\\")
}

fn read_artifact_meta(path: &Path) -> Option<AntigravityArtifactMeta> {
    let content = read_capped(path)?;
    serde_json::from_str(&content).ok()
}

fn antigravity_ide_summary(root: &Path, id: &str, dir: &Path, index: &HashMap<String, AntigravityIdeIndexEntry>) -> SessionSummary {
    let task = dir.join("task.md");
    let plan = dir.join("implementation_plan.md");
    let walkthrough = dir.join("walkthrough.md");
    let transcript = dir.join(".system_generated").join("logs").join("transcript.jsonl");
    let conv = root.join("conversations").join(format!("{id}.pb"));

    let task_meta = read_artifact_meta(&dir.join("task.md.metadata.json"));
    let plan_meta = read_artifact_meta(&dir.join("implementation_plan.md.metadata.json"));
    let walkthrough_meta = read_artifact_meta(&dir.join("walkthrough.md.metadata.json"));

    let title = index
        .get(id)
        .map(|e| e.title.trim())
        .filter(|s| !s.is_empty())
        .map(|s| truncate(s, MAX_TITLE_CHARS))
        .or_else(|| task_meta.as_ref().map(|m| m.summary.trim()).filter(|s| !s.is_empty()).map(|s| truncate(s, MAX_TITLE_CHARS)))
        .or_else(|| first_markdown_heading(&task))
        .or_else(|| first_markdown_heading(&plan))
        .or_else(|| first_markdown_heading(&walkthrough))
        .unwrap_or_else(|| "Antigravity IDE 会话".to_string());

    let cwd = index
        .get(id)
        .map(|e| e.cwd.trim())
        .filter(|s| !s.is_empty())
        .map(|s| normalize_workspace_path(s.to_string()))
        .or_else(|| antigravity_ide_workspace_from_dir(dir))
        .unwrap_or_else(|| "Antigravity IDE".to_string());

    let file_size = [task.as_path(), plan.as_path(), walkthrough.as_path(), transcript.as_path(), conv.as_path()]
        .iter()
        .map(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();
    let modified_at = [
        task_meta.as_ref().and_then(|m| m.updated_at.as_deref()).and_then(iso_secs),
        plan_meta.as_ref().and_then(|m| m.updated_at.as_deref()).and_then(iso_secs),
        walkthrough_meta.as_ref().and_then(|m| m.updated_at.as_deref()).and_then(iso_secs),
        metadata_secs(dir, false),
        metadata_secs(&conv, false),
    ]
    .into_iter()
    .flatten()
    .max();

    SessionSummary {
        session_id: format!("brain/{id}"),
        title,
        session_type: "antigravity-ide".to_string(),
        workspace_directory: cwd.clone(),
        workspace_hash: format!("antigravity-ide:{cwd}"),
        message_count: antigravity_ide_message_count(dir, &conv),
        file_size,
        created_at: metadata_secs(dir, true).or_else(|| metadata_secs(&conv, true)),
        modified_at,
        source: "antigravity-ide".to_string(),
    }
}

fn first_markdown_heading(path: &Path) -> Option<String> {
    let content = read_capped(path)?;
    content
        .lines()
        .find_map(|line| {
            let text = line.trim().trim_start_matches('#').trim();
            (!text.is_empty()).then(|| truncate(text, MAX_TITLE_CHARS))
        })
}

fn antigravity_ide_workspace_from_dir(dir: &Path) -> Option<String> {
    for name in ["task.md", "implementation_plan.md", "walkthrough.md"] {
        let content = read_capped(&dir.join(name)).unwrap_or_default();
        if let Some(cwd) = clean_file_uri_at(&content) {
            return Some(cwd);
        }
    }
    None
}

fn antigravity_ide_message_count(dir: &Path, conv: &Path) -> usize {
    let mut count = 0;
    for name in ["task.md", "implementation_plan.md", "walkthrough.md"] {
        if fs::metadata(dir.join(name)).map(|m| m.len()).unwrap_or(0) > 0 {
            count += 1;
        }
    }
    let transcript = dir.join(".system_generated").join("logs").join("transcript.jsonl");
    if fs::metadata(&transcript).map(|m| m.len()).unwrap_or(0) > 0 {
        count += read_capped(&transcript).map(|c| c.lines().count()).unwrap_or(1);
    }
    if fs::metadata(conv).map(|m| m.len()).unwrap_or(0) > 0 {
        count += 1;
    }
    count
}

fn collect_antigravity_ide_source(d: &SourceDef, root: &Path) -> Vec<SessionSummary> {
    let index = antigravity_ide_index();
    let brain = root.join("brain");
    let mut ids = HashSet::new();
    let mut out = Vec::new();

    if let Ok(entries) = fs::read_dir(&brain) {
        for e in entries.flatten() {
            let dir = e.path();
            if !dir.is_dir() {
                continue;
            }
            let Some(id) = e.file_name().to_str().and_then(extract_uuid) else { continue };
            if ids.insert(id.clone()) {
                out.push(antigravity_ide_summary(root, &id, &dir, &index));
            }
        }
    }

    let conv_dir = root.join("conversations");
    if let Ok(entries) = fs::read_dir(&conv_dir) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("pb") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()).and_then(extract_uuid) else { continue };
            if ids.contains(&id) {
                continue;
            }
            let entry = index.get(&id).cloned().unwrap_or_default();
            let cwd = if entry.cwd.is_empty() { "Antigravity IDE".to_string() } else { normalize_workspace_path(entry.cwd) };
            out.push(SessionSummary {
                session_id: format!("conversations/{id}.pb"),
                title: if entry.title.is_empty() { "Antigravity IDE 会话".to_string() } else { truncate(&entry.title, MAX_TITLE_CHARS) },
                session_type: d.source.to_string(),
                workspace_directory: cwd.clone(),
                workspace_hash: format!("{}{}", d.prefix, cwd),
                message_count: 1,
                file_size: fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                created_at: metadata_secs(&path, true),
                modified_at: metadata_secs(&path, false),
                source: d.source.to_string(),
            });
        }
    }

    out
}

fn load_antigravity_ide_session(d: &SourceDef, root: &Path, path: &Path, session_id: &str, hash: &str) -> anyhow::Result<IdeSession> {
    let mut history = Vec::new();
    let mut title = "Antigravity IDE 会话".to_string();
    let mut cwd = hash.strip_prefix(d.prefix).unwrap_or_default().to_string();

    if session_id.starts_with("brain/") {
        let dir = path;
        if let Some(t) = first_markdown_heading(&dir.join("task.md"))
            .or_else(|| first_markdown_heading(&dir.join("implementation_plan.md")))
            .or_else(|| first_markdown_heading(&dir.join("walkthrough.md")))
        {
            title = t;
        }
        if cwd.is_empty() {
            cwd = antigravity_ide_workspace_from_dir(dir).unwrap_or_default();
        }
        if !cwd.is_empty() {
            history.push(history_item("system", format!("工作目录：{}", cwd), 0));
        }
        push_antigravity_ide_file(&mut history, dir, "task.md", "task.md");
        push_antigravity_ide_file(&mut history, dir, "implementation_plan.md", "implementation_plan.md");
        push_antigravity_ide_file(&mut history, dir, "walkthrough.md", "walkthrough.md");
        push_antigravity_ide_file(&mut history, dir, ".system_generated/logs/transcript.jsonl", "transcript.jsonl");

        if let Some(id) = session_id.strip_prefix("brain/") {
            let conv = root.join("conversations").join(format!("{id}.pb"));
            if let Some(bytes) = read_bytes_capped(&conv) {
                let parsed = antigravity_strings_to_parsed(&bytes);
                let readable = parsed
                    .blocks
                    .into_iter()
                    .map(|(_, text)| text)
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");
                if !readable.trim().is_empty() {
                    let idx = history.len();
                    history.push(history_item("artifact", format!("## conversation.pb\n\n{readable}"), idx));
                }
            }
        }
    } else {
        let bytes = read_bytes_capped(path).ok_or_else(|| anyhow::anyhow!("无法读取会话文件"))?;
        let parsed = antigravity_strings_to_parsed(&bytes);
        title = if parsed.title == "未命名会话" { title } else { parsed.title };
        if cwd.is_empty() {
            cwd = parsed.cwd;
        }
        for (role, text) in parsed.blocks {
            let idx = history.len();
            history.push(history_item(&role, text, idx));
        }
        if history.is_empty() {
            history.push(history_item("assistant", "这个 .pb 文件没有解析出可安全展示的文本内容。".to_string(), 0));
        }
    }

    Ok(IdeSession {
        session_id: session_id.to_string(),
        title,
        session_type: d.source.to_string(),
        workspace_directory: cwd,
        history,
        conversation_summary: None,
    })
}

fn push_antigravity_ide_file(history: &mut Vec<HistoryItem>, dir: &Path, rel: &str, label: &str) {
    let path = rel.split('/').fold(dir.to_path_buf(), |p, part| p.join(part));
    let Some(content) = read_capped(&path) else { return };
    if content.trim().is_empty() {
        return;
    }
    let idx = history.len();
    history.push(history_item("artifact", format!("## {label}\n\n{content}"), idx));
}

fn parse_antigravity(content: &str) -> Parsed {
    let mut p = Parsed { cwd: String::new(), title: String::new(), created: None, updated: None, blocks: Vec::new() };
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if let Some(ts) = v.get("created_at").and_then(|x| x.as_str()).and_then(iso_secs) {
            if p.created.is_none() {
                p.created = Some(ts);
            }
            p.updated = Some(ts);
        }
        let role = if v.get("source").and_then(|x| x.as_str()) == Some("USER_EXPLICIT") { "user" } else { "assistant" };
        let raw = v.get("content").and_then(|x| x.as_str()).unwrap_or("");
        let text = match (raw.find("<USER_REQUEST>"), raw.find("</USER_REQUEST>")) {
            (Some(a), Some(b)) if b > a => raw[a + "<USER_REQUEST>".len()..b].to_string(),
            _ => raw.to_string(),
        };
        push_block(&mut p.blocks, role, text);
    }
    p.title = title_from(&p.blocks);
    p
}

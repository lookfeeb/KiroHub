// ===== 列表 =====

/// 收集所有外部来源的 workspace 标识（codex:/claude: 按 cwd，antigravity 单组）
pub fn list_workspaces() -> Vec<String> {
    let mut out = Vec::new();
    for s in collect_all() {
        if !out.contains(&s.workspace_hash) {
            out.push(s.workspace_hash.clone());
        }
    }
    out
}

/// 列出某 workspace 下的会话
pub fn list_sessions(hash: &str) -> Vec<SessionSummary> {
    collect_all().into_iter().filter(|s| s.workspace_hash == hash).collect()
}

pub fn list_all_sessions() -> Vec<SessionSummary> {
    collect_all()
}

/// 扫描三个来源，产出全部会话摘要
fn collect_all() -> Vec<SessionSummary> {
    if let Ok(cache) = CACHE.lock() {
        if let Some((t, v)) = cache.as_ref() {
            if t.elapsed() < CACHE_TTL {
                return v.clone();
            }
        }
    }
    let fresh = scan_all();
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some((Instant::now(), fresh.clone()));
    }
    fresh
}

fn scan_all() -> Vec<SessionSummary> {
    // 每个来源各起一个线程并发扫描，随注册表自动伸缩
    let mut out: Vec<SessionSummary> = std::thread::scope(|s| {
        let handles: Vec<_> = SOURCES.iter().map(|d| s.spawn(move || collect_source(d))).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    out.sort_by(|a, b| b.modified_at.unwrap_or(0).cmp(&a.modified_at.unwrap_or(0)));
    out
}

/// 按来源定义扫描其根目录，产出会话摘要
fn collect_source(d: &SourceDef) -> Vec<SessionSummary> {
    let Some(root) = (d.root)().filter(|r| r.is_dir()) else { return Vec::new() };
    let mut out = Vec::new();
    match d.layout {
        Layout::File { depth } => {
            let mut files = Vec::new();
            collect_files(&root, "jsonl", depth, &mut files);
            for f in files {
                let Some(p) = (d.scan)(&f) else { continue };
                let Some(key) = rel_key(&root, &f) else { continue };
                out.push(summary_from_scan(d, p, key, &f));
            }
        }
        Layout::Antigravity => {
            let index = antigravity_summary_index(&root);
            let conv_dir = root.join("conversations");
            let Ok(entries) = fs::read_dir(&conv_dir) else { return out };
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some("pb") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
                let metadata = match e.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let (title, cwd) = index
                    .get(stem)
                    .cloned()
                    .unwrap_or_else(|| ("Antigravity 会话".to_string(), "Antigravity".to_string()));
                out.push(SessionSummary {
                    session_id: format!("conversations/{stem}.pb"),
                    title,
                    session_type: d.source.to_string(),
                    workspace_directory: cwd.clone(),
                    workspace_hash: format!("{}{cwd}", d.prefix),
                    message_count: 0,
                    file_size: metadata.len(),
                    created_at: metadata_secs(&path, true),
                    modified_at: metadata_secs(&path, false),
                    source: d.source.to_string(),
                });
            }
        }
        Layout::AntigravityIde => {
            out.extend(collect_antigravity_ide_source(d, &root));
        }
    }
    out
}

fn rel_key(root: &Path, file: &Path) -> Option<String> {
    file.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/"))
}

fn summary_from_scan(d: &SourceDef, p: ParsedSummary, session_id: String, file: &Path) -> SessionSummary {
    SessionSummary {
        session_id,
        title: p.title,
        session_type: d.source.to_string(),
        workspace_directory: p.cwd.clone(),
        workspace_hash: format!("{}{}", d.prefix, p.cwd),
        message_count: p.message_count,
        file_size: fs::metadata(file).map(|m| m.len()).unwrap_or(0),
        created_at: p.created.or_else(|| metadata_secs(file, true)),
        modified_at: p.updated.or_else(|| metadata_secs(file, false)),
        source: d.source.to_string(),
    }
}

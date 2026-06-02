// ===== Codex =====

fn parse_codex(content: &str) -> Parsed {
    let mut p = Parsed { cwd: String::new(), title: String::new(), created: None, updated: None, blocks: Vec::new() };
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if let Some(ts) = v.get("timestamp").and_then(|x| x.as_str()).and_then(iso_secs) {
            p.updated = Some(ts);
            if p.created.is_none() {
                p.created = Some(ts);
            }
        }
        let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let payload = v.get("payload");
        let pty = payload.and_then(|x| x.get("type")).and_then(|x| x.as_str()).unwrap_or("");
        match (ty, pty) {
            ("session_meta", _) => {
                if let Some(cwd) = payload.and_then(|x| x.get("cwd")).and_then(|x| x.as_str()) {
                    p.cwd = cwd.to_string();
                }
            }
            ("event_msg", "user_message") => {
                if let Some(m) = payload.and_then(|x| x.get("message")).and_then(|x| x.as_str()) {
                    push_block(&mut p.blocks, "user", m.to_string());
                }
            }
            ("response_item", "message") => {
                if payload.and_then(|x| x.get("role")).and_then(|x| x.as_str()) == Some("assistant") {
                    let text = payload
                        .and_then(|x| x.get("content"))
                        .and_then(|c| c.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter(|it| matches!(it.get("type").and_then(|x| x.as_str()), Some("output_text" | "input_text")))
                                .filter_map(|it| it.get("text").and_then(|x| x.as_str()))
                                .collect::<Vec<_>>()
                                .join("")
                        })
                        .unwrap_or_default();
                    push_block(&mut p.blocks, "assistant", text);
                }
            }
            _ => {}
        }
    }
    p.title = title_from(&p.blocks);
    p
}

fn scan_codex_summary(path: &Path) -> Option<ParsedSummary> {
    let size = fs::metadata(path).map(|m| m.len()).ok()?;
    if size == 0 || size > MAX_FILE_SIZE {
        return None;
    }

    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut cwd = String::new();
    let mut first_user = String::new();
    let mut created = None;
    let mut updated = None;
    let mut message_count = 0usize;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        if let Some(ts) = v.get("timestamp").and_then(|x| x.as_str()).and_then(iso_secs) {
            updated = Some(ts);
            if created.is_none() {
                created = Some(ts);
            }
        }

        let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let payload = v.get("payload");
        let pty = payload.and_then(|x| x.get("type")).and_then(|x| x.as_str()).unwrap_or("");
        match (ty, pty) {
            ("session_meta", _) if cwd.is_empty() => {
                if let Some(value) = payload.and_then(|x| x.get("cwd")).and_then(|x| x.as_str()) {
                    cwd = value.to_string();
                }
            }
            ("event_msg", "user_message") => {
                if let Some(m) = payload.and_then(|x| x.get("message")).and_then(|x| x.as_str()) {
                    message_count += 1;
                    if first_user.is_empty() {
                        first_user = m.to_string();
                    }
                }
            }
            ("response_item", "message") => {
                if payload.and_then(|x| x.get("role")).and_then(|x| x.as_str()) == Some("assistant") {
                    message_count += 1;
                }
            }
            _ => {}
        }
    }

    if cwd.is_empty() && message_count == 0 {
        return None;
    }

    Some(ParsedSummary {
        cwd,
        title: title_from_user_text(&first_user),
        created,
        updated,
        message_count,
    })
}

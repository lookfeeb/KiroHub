// ===== Codex =====

fn parse_codex(content: &str) -> Parsed {
    let mut p = Parsed {
        cwd: String::new(),
        title: String::new(),
        created: None,
        updated: None,
        blocks: Vec::new(),
    };
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(ts) = v
            .get("timestamp")
            .and_then(|x| x.as_str())
            .and_then(iso_secs)
        {
            p.updated = Some(ts);
            if p.created.is_none() {
                p.created = Some(ts);
            }
        }
        let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let payload = v.get("payload");
        let pty = payload
            .and_then(|x| x.get("type"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        match (ty, pty) {
            ("session_meta", _) => {
                if let Some(cwd) = payload.and_then(|x| x.get("cwd")).and_then(|x| x.as_str()) {
                    p.cwd = cwd.to_string();
                }
            }
            ("event_msg", "user_message") => {
                if let Some(m) = payload
                    .and_then(|x| x.get("message"))
                    .and_then(|x| x.as_str())
                {
                    push_block(&mut p.blocks, "user", m.to_string());
                }
            }
            ("response_item", "message") => {
                if payload.and_then(|x| x.get("role")).and_then(|x| x.as_str()) == Some("assistant")
                {
                    let text = payload
                        .and_then(|x| x.get("content"))
                        .and_then(|c| c.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter(|it| {
                                    matches!(
                                        it.get("type").and_then(|x| x.as_str()),
                                        Some("output_text" | "input_text")
                                    )
                                })
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
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if let Some(ts) = v
            .get("timestamp")
            .and_then(|x| x.as_str())
            .and_then(iso_secs)
        {
            updated = Some(ts);
            if created.is_none() {
                created = Some(ts);
            }
        }

        let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let payload = v.get("payload");
        let pty = payload
            .and_then(|x| x.get("type"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        match (ty, pty) {
            ("session_meta", _) if cwd.is_empty() => {
                if let Some(value) = payload.and_then(|x| x.get("cwd")).and_then(|x| x.as_str()) {
                    cwd = value.to_string();
                }
            }
            ("event_msg", "user_message") => {
                if let Some(m) = payload
                    .and_then(|x| x.get("message"))
                    .and_then(|x| x.as_str())
                {
                    message_count += 1;
                    if first_user.is_empty() {
                        first_user = m.to_string();
                    }
                }
            }
            ("response_item", "message") => {
                if payload.and_then(|x| x.get("role")).and_then(|x| x.as_str()) == Some("assistant")
                {
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

fn normalize_codex_workspace(value: &str) -> String {
    let value = value
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .unwrap_or_else(|| value.strip_prefix(r"\\?\").unwrap_or(value).to_string());
    value
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn codex_session_identity(path: &Path) -> anyhow::Result<(Option<String>, Option<String>)> {
    let file = fs::File::open(path)?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|v| v.as_str()) != Some("session_meta") {
            continue;
        }
        let payload = value.get("payload");
        let id = payload
            .and_then(|v| v.get("id").or_else(|| v.get("session_id")))
            .and_then(|v| v.as_str())
            .and_then(extract_uuid);
        let cwd = payload
            .and_then(|v| v.get("cwd"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        return Ok((id, cwd));
    }
    Ok((None, None))
}

fn codex_id_from_filename(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(extract_uuid)
}

fn codex_id_from_path(path: &Path) -> anyhow::Result<Option<String>> {
    Ok(codex_session_identity(path)?
        .0
        .or_else(|| codex_id_from_filename(path)))
}

fn codex_database_paths(codex_home: &Path, prefix: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(codex_home)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".sqlite"))
        {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn collect_codex_files_strict(
    dir: &Path,
    ext: &str,
    depth: usize,
    out: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    if depth == 0 {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_codex_files_strict(&path, ext, depth - 1, out)?;
        } else if file_type.is_file()
            && path.extension().and_then(|value| value.to_str()) == Some(ext)
        {
            out.push(path);
        }
    }
    Ok(())
}

fn sqlite_table_names(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "select name from sqlite_master where type = 'table' and name not like 'sqlite_%'",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn sqlite_table_columns(conn: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let quoted = table.replace('"', "\"\"");
    let mut stmt = conn.prepare(&format!("pragma table_info(\"{quoted}\")"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn codex_state_rows(db_path: &Path) -> anyhow::Result<Vec<(String, String, String)>> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_secs(3))?;
    if !sqlite_table_names(&conn)?
        .iter()
        .any(|name| name == "threads")
    {
        return Ok(Vec::new());
    }
    let columns = sqlite_table_columns(&conn, "threads")?;
    if !["id", "rollout_path", "cwd"]
        .iter()
        .all(|required| columns.iter().any(|column| column == required))
    {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare("select id, rollout_path, cwd from threads")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn safe_codex_rollout_path(root: &Path, raw: &str) -> Option<PathBuf> {
    let raw = raw
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .unwrap_or_else(|| raw.strip_prefix(r"\\?\").unwrap_or(raw).to_string());
    if raw.trim().is_empty() {
        return None;
    }
    let candidate = PathBuf::from(&raw);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    if !candidate.is_file() {
        return None;
    }
    let canonical_root = root.canonicalize().ok()?;
    let canonical_candidate = candidate.canonicalize().ok()?;
    canonical_candidate
        .starts_with(canonical_root)
        .then_some(canonical_candidate)
}

fn rewrite_jsonl_without_ids(
    path: &Path,
    id_field: &str,
    ids: &HashSet<String>,
) -> anyhow::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            return Err(anyhow::anyhow!(
                "Codex 索引路径不是文件: {}",
                path.display()
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    let content = fs::read_to_string(path)?;
    let mut filtered = String::with_capacity(content.len());
    let mut changed = false;

    for line in content.split_inclusive('\n') {
        let json = line.trim_end_matches(['\r', '\n']);
        let remove = serde_json::from_str::<serde_json::Value>(json)
            .ok()
            .and_then(|value| {
                value
                    .get(id_field)
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
            .is_some_and(|id| ids.contains(&id));
        if remove {
            changed = true;
        } else {
            filtered.push_str(line);
        }
    }

    if changed {
        let mut file = OpenOptions::new().write(true).truncate(true).open(path)?;
        file.write_all(filtered.as_bytes())?;
        file.sync_all()?;
    }
    Ok(())
}

fn delete_thread_rows_from_db(db_path: &Path, ids: &HashSet<String>) -> anyhow::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.busy_timeout(Duration::from_secs(3))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let mut table_columns = sqlite_table_names(&conn)?
        .into_iter()
        .map(|table| {
            let columns = sqlite_table_columns(&conn, &table)?;
            Ok((table, columns))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    // 先删关联表，再删 threads 主表；兼容未来新增的非级联外键。
    table_columns.sort_by(|left, right| {
        (left.0 == "threads", &left.0).cmp(&(right.0 == "threads", &right.0))
    });
    let mut sorted_ids = ids.iter().collect::<Vec<_>>();
    sorted_ids.sort();

    let transaction = conn.transaction()?;
    for (table, columns) in table_columns {
        let id_columns = columns
            .iter()
            .filter(|column| {
                column.as_str() == "thread_id"
                    || column.as_str() == "parent_thread_id"
                    || column.as_str() == "child_thread_id"
                    || (table == "threads" && column.as_str() == "id")
            })
            .cloned()
            .collect::<Vec<_>>();
        if id_columns.is_empty() {
            continue;
        }
        let table = table.replace('"', "\"\"");
        for column in id_columns {
            let column = column.replace('"', "\"\"");
            // 避免大型 logs 数据库按每个 ID 重复全表扫描。
            for chunk in sorted_ids.chunks(500) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!("delete from \"{table}\" where \"{column}\" in ({placeholders})");
                transaction.execute(&sql, rusqlite::params_from_iter(chunk.iter().copied()))?;
            }
        }
    }
    transaction.commit()?;
    let _ = conn.execute_batch("pragma wal_checkpoint(truncate)");
    Ok(())
}

fn cleanup_empty_codex_dirs(root: &Path, path: &Path) {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir == root || !dir.starts_with(root) {
            break;
        }
        let empty = fs::read_dir(dir)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !empty || fs::remove_dir(dir).is_err() {
            break;
        }
        current = dir.parent();
    }
}

fn delete_codex_targets(
    root: &Path,
    ids: &HashSet<String>,
    paths: &HashSet<PathBuf>,
) -> anyhow::Result<()> {
    let codex_home = root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("无法定位 Codex 数据目录"))?;
    let mut metadata_errors = Vec::new();

    for prefix in ["state_", "logs_", "goals_", "memories_"] {
        match codex_database_paths(codex_home, prefix) {
            Ok(paths) => {
                for db_path in paths {
                    if let Err(error) = delete_thread_rows_from_db(&db_path, ids) {
                        metadata_errors.push(format!("清理 {} 失败: {error}", db_path.display()));
                    }
                }
            }
            Err(error) => metadata_errors.push(format!("枚举 Codex {prefix} 数据库失败: {error}")),
        }
    }

    for (name, field) in [
        ("session_index.jsonl", "id"),
        ("session_index.jsonl.bak", "id"),
        ("history.jsonl", "session_id"),
    ] {
        let path = codex_home.join(name);
        if let Err(error) = rewrite_jsonl_without_ids(&path, field, ids) {
            metadata_errors.push(format!("清理 {} 失败: {error}", path.display()));
        }
    }

    // 元数据清理失败时保留正文文件，确保用户关闭 Codex 后还能从 KiroHub 重试。
    if !metadata_errors.is_empty() {
        return Err(anyhow::anyhow!(metadata_errors.join("；")));
    }

    let canonical_root = root.canonicalize()?;
    let mut file_errors = Vec::new();
    for path in paths {
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => {
                file_errors.push(format!("Codex 会话路径不是文件: {}", path.display()));
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                file_errors.push(format!("读取 {} 失败: {error}", path.display()));
                continue;
            }
        }
        let canonical = match path.canonicalize() {
            Ok(path) if path.starts_with(&canonical_root) && path.is_file() => path,
            Ok(_) => {
                file_errors.push(format!(
                    "拒绝删除 sessions 目录之外的路径: {}",
                    path.display()
                ));
                continue;
            }
            Err(error) => {
                file_errors.push(format!("校验 {} 失败: {error}", path.display()));
                continue;
            }
        };
        match fs::remove_file(&canonical) {
            Ok(()) => cleanup_empty_codex_dirs(root, path),
            Err(error) => file_errors.push(format!("删除 {} 失败: {error}", path.display())),
        }
    }

    if file_errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(file_errors.join("；")))
    }
}

fn delete_codex_session(root: &Path, path: &Path) -> anyhow::Result<()> {
    let id = codex_id_from_path(path)?
        .ok_or_else(|| anyhow::anyhow!("无法识别 Codex 会话 ID，未执行不完整删除"))?;
    let ids = HashSet::from([id]);
    let paths = HashSet::from([path.to_path_buf()]);
    delete_codex_targets(root, &ids, &paths)
}

fn delete_codex_workspace(root: &Path, workspace: &str) -> anyhow::Result<()> {
    let target = normalize_codex_workspace(workspace);
    if target.is_empty() {
        return Err(anyhow::anyhow!("Codex 工作区路径为空，已拒绝删除"));
    }
    let codex_home = root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("无法定位 Codex 数据目录"))?;
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    let mut unidentified_paths = Vec::new();

    let mut files = Vec::new();
    collect_codex_files_strict(root, "jsonl", 8, &mut files)?;
    for path in files {
        let (id, cwd) = codex_session_identity(&path)?;
        if cwd
            .as_deref()
            .is_some_and(|cwd| normalize_codex_workspace(cwd) == target)
        {
            match id.or_else(|| codex_id_from_filename(&path)) {
                Some(id) => {
                    ids.insert(id);
                    paths.insert(path);
                }
                None => unidentified_paths.push(path),
            }
        }
    }

    if !unidentified_paths.is_empty() {
        let paths = unidentified_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("、");
        return Err(anyhow::anyhow!(
            "以下 Codex 会话无法识别 ID，未执行不完整删除: {paths}"
        ));
    }

    for db_path in codex_database_paths(codex_home, "state_")? {
        for (id, rollout_path, cwd) in codex_state_rows(&db_path)? {
            if normalize_codex_workspace(&cwd) != target {
                continue;
            }
            ids.insert(id);
            if let Some(path) = safe_codex_rollout_path(root, &rollout_path) {
                paths.insert(path);
            }
        }
    }

    delete_codex_targets(root, &ids, &paths)?;

    let mut remaining_files = Vec::new();
    collect_codex_files_strict(root, "jsonl", 8, &mut remaining_files)?;
    let mut file_remains = false;
    for path in &remaining_files {
        if codex_session_identity(path)?
            .1
            .as_deref()
            .is_some_and(|cwd| normalize_codex_workspace(cwd) == target)
        {
            file_remains = true;
            break;
        }
    }
    let state_remains = codex_database_paths(codex_home, "state_")?
        .iter()
        .map(|path| codex_state_rows(path))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .any(|(_, _, cwd)| normalize_codex_workspace(&cwd) == target);
    if file_remains || state_remains {
        return Err(anyhow::anyhow!(
            "Codex 工作区仍有残留记录，请关闭 Codex 后重试"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod codex_delete_tests {
    use super::*;

    fn test_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("kirohub-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn rewrites_codex_indexes_by_session_id() {
        let dir = test_dir("codex-index");
        let path = dir.join("session_index.jsonl");
        fs::write(&path, "{\"id\":\"keep\"}\n{\"id\":\"remove\"}\nnot-json\n").unwrap();
        rewrite_jsonl_without_ids(&path, "id", &HashSet::from(["remove".to_string()])).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"id\":\"keep\"}\nnot-json\n"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn normalizes_extended_windows_workspace_paths() {
        assert_eq!(
            normalize_codex_workspace(r"\\?\C:\Workspace\Target\"),
            r"c:\workspace\target"
        );
        assert_eq!(
            normalize_codex_workspace(r"\\?\UNC\Server\Share\Project"),
            r"\\server\share\project"
        );
    }

    #[test]
    fn rejects_empty_codex_workspace() {
        let root = test_dir("codex-empty-workspace");
        assert!(delete_codex_workspace(&root, "").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_unidentified_codex_rollout_for_retry() {
        let home = test_dir("codex-unidentified");
        let root = home.join("sessions");
        fs::create_dir_all(&root).unwrap();
        let rollout = root.join("rollout-without-id.jsonl");
        fs::write(
            &rollout,
            r#"{"type":"session_meta","payload":{"cwd":"C:\\Workspace\\Target"}}
"#,
        )
        .unwrap();

        assert!(delete_codex_workspace(&root, "C:\\Workspace\\Target").is_err());
        assert!(rollout.exists());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn removes_thread_rows_from_related_tables() {
        let dir = test_dir("codex-db");
        let path = dir.join("state_5.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "create table threads (id text primary key, rollout_path text, cwd text);\
             create table thread_spawn_edges (parent_thread_id text, child_thread_id text);\
             create table logs (thread_id text, body text);\
             insert into threads values ('remove', 'a', 'c'), ('keep', 'b', 'd');\
             insert into thread_spawn_edges values ('remove', 'keep'), ('keep', 'remove');\
             insert into logs values ('remove', 'x'), ('keep', 'y');",
        )
        .unwrap();
        drop(conn);

        delete_thread_rows_from_db(&path, &HashSet::from(["remove".to_string()])).unwrap();
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            conn.query_row(
                "select count(*) from threads where id = 'remove'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row("select count(*) from thread_spawn_edges", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "select count(*) from logs where thread_id = 'remove'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        drop(conn);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn deletes_workspace_files_and_orphan_codex_threads() {
        let home = test_dir("codex-workspace");
        let root = home.join("sessions");
        let day = root.join("2026").join("07").join("27");
        fs::create_dir_all(&day).unwrap();
        let removed_id = "11111111-1111-4111-8111-111111111111";
        let orphan_id = "22222222-2222-4222-8222-222222222222";
        let keep_id = "33333333-3333-4333-8333-333333333333";
        let rollout = day.join(format!("rollout-{removed_id}.jsonl"));
        fs::write(
            &rollout,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{removed_id}\",\"cwd\":\"C:\\\\Workspace\\\\Target\"}}}}\n"
            ),
        )
        .unwrap();
        fs::write(
            home.join("session_index.jsonl"),
            format!(
                "{{\"id\":\"{removed_id}\"}}\n{{\"id\":\"{orphan_id}\"}}\n{{\"id\":\"{keep_id}\"}}\n"
            ),
        )
        .unwrap();
        fs::write(
            home.join("history.jsonl"),
            format!(
                "{{\"session_id\":\"{removed_id}\",\"text\":\"remove\"}}\n{{\"session_id\":\"{keep_id}\",\"text\":\"keep\"}}\n"
            ),
        )
        .unwrap();

        let db = home.join("state_5.sqlite");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "create table threads (id text primary key, rollout_path text, cwd text);",
        )
        .unwrap();
        for (id, cwd) in [
            (removed_id, "C:\\Workspace\\Target"),
            (orphan_id, "C:\\Workspace\\Target"),
            (keep_id, "C:\\Workspace\\Other"),
        ] {
            conn.execute(
                "insert into threads (id, rollout_path, cwd) values (?1, '', ?2)",
                rusqlite::params![id, cwd],
            )
            .unwrap();
        }
        drop(conn);

        delete_codex_workspace(&root, "C:\\Workspace\\Target").unwrap();
        assert!(!rollout.exists());
        let index = fs::read_to_string(home.join("session_index.jsonl")).unwrap();
        assert!(index.contains(keep_id));
        assert!(!index.contains(removed_id));
        let history = fs::read_to_string(home.join("history.jsonl")).unwrap();
        assert!(!history.contains(removed_id));
        let conn = Connection::open(&db).unwrap();
        assert_eq!(
            conn.query_row(
                "select count(*) from threads where cwd = ?1",
                ["C:\\Workspace\\Target"],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "select count(*) from threads where id = ?1",
                [keep_id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        drop(conn);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn keeps_rollout_file_when_codex_metadata_cleanup_fails() {
        let home = test_dir("codex-retry");
        let root = home.join("sessions");
        fs::create_dir_all(&root).unwrap();
        let id = "11111111-1111-4111-8111-111111111111";
        let rollout = root.join(format!("rollout-{id}.jsonl"));
        fs::write(
            &rollout,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"C:\\\\Workspace\\\\Target\"}}}}\n"
            ),
        )
        .unwrap();
        // 用同名目录模拟索引被占用/不可写，元数据阶段必须失败且保留正文供重试。
        fs::create_dir(home.join("session_index.jsonl")).unwrap();

        assert!(delete_codex_session(&root, &rollout).is_err());
        assert!(rollout.exists());
        fs::remove_dir_all(home).unwrap();
    }
}

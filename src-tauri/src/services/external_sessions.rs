// 外部 AI CLI 历史会话解析：Codex / Claude / Antigravity
// 统一映射到既有 SessionSummary / IdeSession 模型，作为新的 source 接入会话管理。
// 详见 session-history-parsing.md。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::models::ide_session::{ContentItem, HistoryItem, IdeSession, Message, SessionSummary};

const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 72;
const CACHE_TTL: Duration = Duration::from_secs(5);

/// 进程内短期缓存：避免一次刷新中 list_workspaces + N×list_sessions 重复全量扫描
static CACHE: Mutex<Option<(Instant, Vec<SessionSummary>)>> = Mutex::new(None);

/// 失效缓存（刷新按钮强制重扫）
pub fn invalidate_cache() {
    *CACHE.lock().unwrap() = None;
}

/// 会话在磁盘上的组织方式
#[derive(Clone, Copy)]
enum Layout {
    /// 递归扫描 root 下的 *.jsonl，每文件一会话；session_id = 相对路径
    File { depth: usize },
    /// root 下每个子目录一会话；日志位于子目录内 `log` 相对路径（按段拼接）
    Dir { log: &'static [&'static str] },
}

/// 一个外部 CLI 历史来源的定义。
/// 新增其它 CLI：只需在 SOURCES 增加一项 + 写一个 `parse_xxx` 函数即可，无需改动分发逻辑。
struct SourceDef {
    prefix: &'static str,
    source: &'static str,
    root: fn() -> Option<PathBuf>,
    layout: Layout,
    parse: fn(&str) -> Parsed,
}

static SOURCES: &[SourceDef] = &[
    SourceDef { prefix: "codex:", source: "codex", root: codex_root, layout: Layout::File { depth: 8 }, parse: parse_codex },
    SourceDef { prefix: "claude:", source: "claude", root: claude_root, layout: Layout::File { depth: 4 }, parse: parse_claude },
    SourceDef {
        prefix: "antigravity:",
        source: "antigravity",
        root: antigravity_root,
        layout: Layout::Dir { log: &[".system_generated", "logs", "overview.txt"] },
        parse: parse_antigravity,
    },
];

fn def_for(hash: &str) -> Option<&'static SourceDef> {
    SOURCES.iter().find(|d| hash.starts_with(d.prefix))
}

/// 本模块是否负责该 workspace_hash
pub fn handles(hash: &str) -> bool {
    def_for(hash).is_some()
}

/// 按段拼接子目录内的日志路径
fn conv_log(dir: &Path, log: &[&str]) -> PathBuf {
    log.iter().fold(dir.to_path_buf(), |p, c| p.join(c))
}

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}
fn codex_root() -> Option<PathBuf> {
    home().map(|h| h.join(".codex").join("sessions"))
}
fn claude_root() -> Option<PathBuf> {
    home().map(|h| h.join(".claude").join("projects"))
}
fn antigravity_root() -> Option<PathBuf> {
    home().map(|h| h.join(".gemini").join("antigravity").join("brain"))
}

/// 解析后的中间结构
struct Parsed {
    cwd: String,
    title: String,
    created: Option<i64>,
    updated: Option<i64>,
    blocks: Vec<(String, String)>, // (role, text)
}

fn iso_secs(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
}

fn truncate(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() > n {
        t.chars().take(n).collect::<String>() + "…"
    } else {
        t.to_string()
    }
}

/// 从 user 文本提炼标题：跳过标题/标签/代码块/文件路径/上下文样板，
/// 优先取 "My request" 标记之后的首个有意义行。
fn title_from(blocks: &[(String, String)]) -> String {
    blocks
        .iter()
        .filter(|(r, _)| r == "user")
        .find_map(|(_, t)| meaningful_line(t))
        .map(|l| truncate(&l, MAX_TITLE_CHARS))
        .unwrap_or_else(|| "未命名会话".to_string())
}

/// 一行是否“有意义”（可作标题）
fn is_meaningful(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') || t.starts_with('<') || t.starts_with("```") || t.starts_with("//") {
        return false;
    }
    let lower = t.to_lowercase();
    if lower.starts_with("context") || lower.contains("context from") || lower.starts_with("system") {
        return false;
    }
    // 纯文件路径行（盘符或以 / 开头且不含空格）
    if (t.contains(":\\") || t.starts_with('/')) && !t.contains(' ') {
        return false;
    }
    true
}

fn meaningful_line(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    // 优先：My request 标记之后的首个有意义行
    for (i, l) in lines.iter().enumerate() {
        let lt = l.trim();
        if lt.starts_with("## My request") || lt.starts_with("# My request") || lt.contains("My request for") {
            if let Some(n) = lines.iter().skip(i + 1).find(|n| is_meaningful(n)) {
                return Some(n.trim().to_string());
            }
        }
    }
    lines.iter().find(|l| is_meaningful(l)).map(|l| l.trim().to_string())
}

fn mtime_secs(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

fn read_capped(path: &Path) -> Option<String> {
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size == 0 || size > MAX_FILE_SIZE {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// 递归收集指定扩展名文件（限定深度）
fn collect_files(dir: &Path, ext: &str, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_files(&p, ext, depth - 1, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some(ext) {
            out.push(p);
        }
    }
}

fn push_block(blocks: &mut Vec<(String, String)>, role: &str, text: String) {
    if !text.trim().is_empty() {
        blocks.push((role.to_string(), text));
    }
}

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

// ===== Claude =====

fn parse_claude(content: &str) -> Parsed {
    let mut p = Parsed { cwd: String::new(), title: String::new(), created: None, updated: None, blocks: Vec::new() };
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if let Some(ts) = v.get("timestamp").and_then(|x| x.as_str()).and_then(iso_secs) {
            p.updated = Some(ts);
            if p.created.is_none() {
                p.created = Some(ts);
            }
        }
        if p.cwd.is_empty() {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                p.cwd = c.to_string();
            }
        }
        let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let msg = v.get("message");
        let content_val = msg.and_then(|m| m.get("content"));
        match ty {
            "user" => match content_val {
                Some(serde_json::Value::String(s)) => push_block(&mut p.blocks, "user", s.clone()),
                Some(serde_json::Value::Array(arr)) => {
                    for it in arr {
                        match it.get("type").and_then(|x| x.as_str()) {
                            Some("text") => push_block(&mut p.blocks, "user", it.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string()),
                            Some("tool_result") => {
                                let txt = claude_tool_result_text(it.get("content"));
                                let role = if it.get("is_error").and_then(|x| x.as_bool()).unwrap_or(false) { "error" } else { "tool_result" };
                                push_block(&mut p.blocks, role, txt);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            "assistant" => {
                if let Some(arr) = content_val.and_then(|c| c.as_array()) {
                    for it in arr {
                        match it.get("type").and_then(|x| x.as_str()) {
                            Some("text") => push_block(&mut p.blocks, "assistant", it.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string()),
                            Some("thinking") => push_block(&mut p.blocks, "thinking", it.get("thinking").and_then(|x| x.as_str()).unwrap_or("").to_string()),
                            Some("tool_use") => {
                                let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("?");
                                let input = it.get("input").map(|i| serde_json::to_string_pretty(i).unwrap_or_default()).unwrap_or_default();
                                push_block(&mut p.blocks, "tool_use", format!("工具调用：{name}\n{input}"));
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    p.title = title_from(&p.blocks);
    p
}

fn claude_tool_result_text(content: Option<&serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter(|it| it.get("type").and_then(|x| x.as_str()) == Some("text"))
            .filter_map(|it| it.get("text").and_then(|x| x.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

// ===== Antigravity =====

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

/// 扫描三个来源，产出全部会话摘要
fn collect_all() -> Vec<SessionSummary> {
    if let Some((t, v)) = CACHE.lock().unwrap().as_ref() {
        if t.elapsed() < CACHE_TTL {
            return v.clone();
        }
    }
    let fresh = scan_all();
    *CACHE.lock().unwrap() = Some((Instant::now(), fresh.clone()));
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
                let Some(content) = read_capped(&f) else { continue };
                let p = (d.parse)(&content);
                if p.blocks.is_empty() {
                    continue;
                }
                let Some(key) = rel_key(&root, &f) else { continue };
                out.push(summary(d, &p, key, &f));
            }
        }
        Layout::Dir { log } => {
            let Ok(entries) = fs::read_dir(&root) else { return out };
            for e in entries.flatten() {
                let dir = e.path();
                if !dir.is_dir() {
                    continue;
                }
                let conv_id = e.file_name().to_string_lossy().to_string();
                let overview = conv_log(&dir, log);
                let Some(content) = read_capped(&overview) else { continue };
                let size = content.len() as u64;
                let p = (d.parse)(&content);
                if p.blocks.is_empty() {
                    continue;
                }
                let mut s = summary(d, &p, conv_id, &overview);
                s.file_size = size;
                out.push(s);
            }
        }
    }
    out
}

fn rel_key(root: &Path, file: &Path) -> Option<String> {
    file.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/"))
}

fn summary(d: &SourceDef, p: &Parsed, session_id: String, file: &Path) -> SessionSummary {
    let cwd = p.cwd.clone();
    SessionSummary {
        session_id,
        title: p.title.clone(),
        session_type: d.source.to_string(),
        workspace_directory: cwd.clone(),
        workspace_hash: format!("{}{cwd}", d.prefix),
        message_count: p.blocks.len(),
        file_size: fs::metadata(file).map(|m| m.len()).unwrap_or(0),
        created_at: p.created,
        modified_at: p.updated.or_else(|| mtime_secs(file)),
        source: d.source.to_string(),
    }
}

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
    let content_path = match d.layout {
        Layout::Dir { log } => conv_log(&path, log),
        Layout::File { .. } => path,
    };
    let content = read_capped(&content_path).ok_or_else(|| anyhow::anyhow!("无法读取会话文件"))?;
    let parsed = (d.parse)(&content);
    let mut history = Vec::new();
    if !parsed.cwd.is_empty() {
        history.push(history_item("system", format!("工作目录：{}", parsed.cwd), 0));
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
            content: vec![ContentItem { content_type: "text".to_string(), text }],
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
        Layout::Dir { log } => conv_log(&path, log),
        Layout::File { .. } => path,
    };
    // 去掉 Windows canonicalize 产生的 \\?\ 扩展长度前缀
    let s = p.to_string_lossy().to_string();
    Ok(s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s))
}

pub fn delete_session(hash: &str, session_id: &str) -> anyhow::Result<()> {
    let (d, path) = locate(hash, session_id).ok_or_else(|| anyhow::anyhow!("非法的会话路径"))?;
    match d.layout {
        Layout::Dir { .. } => fs::remove_dir_all(&path)?,
        Layout::File { .. } => fs::remove_file(&path)?,
    }
    invalidate_cache();
    Ok(())
}

/// 删除整个 workspace（该 cwd 下全部会话）
pub fn delete_workspace(hash: &str) -> anyhow::Result<()> {
    let keys: Vec<String> = list_sessions(hash).into_iter().map(|s| s.session_id).collect();
    for k in keys {
        let _ = delete_session(hash, &k);
    }
    Ok(())
}

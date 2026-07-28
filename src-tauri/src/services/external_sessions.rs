// 外部 AI 历史会话解析：Codex / Claude / Antigravity / Gemini CLI
// 统一映射到既有 SessionSummary / IdeSession 模型，作为新的 source 接入会话管理。
// 详见 session-history-parsing.md。

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::models::ide_session::{ContentItem, HistoryItem, IdeSession, Message, SessionSummary};
use base64::{engine::general_purpose, Engine as _};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

const MAX_TITLE_CHARS: usize = 72;
const CACHE_TTL: Duration = Duration::from_secs(5);

/// 进程内短期缓存：避免一次刷新中 list_workspaces + N×list_sessions 重复全量扫描
static CACHE: Mutex<Option<(Instant, Vec<SessionSummary>)>> = Mutex::new(None);

/// 失效缓存（刷新按钮强制重扫）
pub fn invalidate_cache() {
    if let Ok(mut cache) = CACHE.lock() {
        *cache = None;
    }
}

/// 会话在磁盘上的组织方式
#[derive(Debug, Clone, Copy)]
enum Layout {
    /// 递归扫描 root 下的 *.jsonl，每文件一会话；session_id = 相对路径
    File { depth: usize },
    /// Antigravity: root/conversations/*.pb，摘要在 root/agyhub_summaries_proto.pb
    Antigravity,
    /// Antigravity IDE: root/brain/<uuid> 聚合 task / plan / walkthrough / transcript
    AntigravityIde,
    /// Gemini CLI: ~/.gemini/tmp/<project>/chats/*.jsonl
    Gemini,
}

/// 一个外部 CLI 历史来源的定义。
/// 新增其它 CLI：只需在 SOURCES 增加一项 + 写解析函数即可，无需改动分发逻辑。
struct SourceDef {
    prefix: &'static str,
    source: &'static str,
    root: fn() -> Option<PathBuf>,
    layout: Layout,
    parse: fn(&str) -> Parsed,
    scan: fn(&Path) -> Option<ParsedSummary>,
}

static SOURCES: &[SourceDef] = &[
    SourceDef {
        prefix: "codex:",
        source: "codex",
        root: codex_root,
        layout: Layout::File { depth: 8 },
        parse: parse_codex,
        scan: scan_codex_summary,
    },
    SourceDef {
        prefix: "claude:",
        source: "claude",
        root: claude_root,
        layout: Layout::File { depth: 4 },
        parse: parse_claude,
        scan: scan_claude_summary,
    },
    SourceDef {
        prefix: "antigravity:",
        source: "antigravity",
        root: antigravity_root,
        layout: Layout::Antigravity,
        parse: parse_antigravity,
        scan: |_| None,
    },
    SourceDef {
        prefix: "antigravity-backup:",
        source: "antigravity-backup",
        root: antigravity_backup_root,
        layout: Layout::Antigravity,
        parse: parse_antigravity,
        scan: |_| None,
    },
    SourceDef {
        prefix: "antigravity-ide:",
        source: "antigravity-ide",
        root: antigravity_ide_root,
        layout: Layout::AntigravityIde,
        parse: parse_antigravity,
        scan: |_| None,
    },
    SourceDef {
        prefix: "gemini:",
        source: "gemini",
        root: gemini_tmp_root,
        layout: Layout::Gemini,
        parse: parse_gemini,
        scan: scan_gemini_summary,
    },
];

fn def_for(hash: &str) -> Option<&'static SourceDef> {
    SOURCES.iter().find(|d| hash.starts_with(d.prefix))
}

/// 本模块是否负责该 workspace_hash
pub fn handles(hash: &str) -> bool {
    def_for(hash).is_some()
}

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}
fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".codex")))
}
fn codex_root() -> Option<PathBuf> {
    codex_home().map(|h| h.join("sessions"))
}
fn claude_home() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".claude")))
}
fn claude_root() -> Option<PathBuf> {
    claude_home().map(|h| h.join("projects"))
}
fn antigravity_root() -> Option<PathBuf> {
    home().map(|h| h.join(".gemini").join("antigravity"))
}
fn antigravity_backup_root() -> Option<PathBuf> {
    home().map(|h| h.join(".gemini").join("antigravity-backup"))
}
fn antigravity_ide_root() -> Option<PathBuf> {
    home().map(|h| h.join(".gemini").join("antigravity-ide"))
}
fn gemini_tmp_root() -> Option<PathBuf> {
    home().map(|h| h.join(".gemini").join("tmp"))
}

/// 解析后的中间结构
struct Parsed {
    cwd: String,
    title: String,
    created: Option<i64>,
    updated: Option<i64>,
    blocks: Vec<(String, String)>, // (role, text)
}

struct ParsedSummary {
    stable_id: Option<String>,
    cwd: String,
    title: String,
    created: Option<i64>,
    updated: Option<i64>,
    message_count: usize,
}

#[derive(Default, serde::Deserialize)]
struct AntigravityArtifactMeta {
    #[serde(default)]
    summary: String,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
}

#[derive(Clone, Default)]
struct AntigravityIdeIndexEntry {
    title: String,
    cwd: String,
}

include!("external_sessions/common.rs");
include!("external_sessions/antigravity.rs");
include!("external_sessions/codex.rs");
include!("external_sessions/claude.rs");
include!("external_sessions/gemini.rs");
include!("external_sessions/catalog.rs");
include!("external_sessions/operations.rs");

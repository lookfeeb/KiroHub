use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Result, Context};
use crate::models::ide_session::{IdeSession, SessionSummary, HistoryItem, Message, ContentItem};
use super::external_sessions;

// 安全限制
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50MB

/// CLI 会话工作区标识前缀：`cli:<cwd>`（按工作目录分组）
const CLI_PREFIX: &str = "cli:";

/// kiro-cli 会话元数据（~/.kiro/sessions/cli/<id>.json）
#[derive(Default, serde::Deserialize)]
struct CliSessionMeta {
    #[serde(default)] session_id: String,
    #[serde(default)] cwd: Option<String>,
    #[serde(default)] title: Option<String>,
    #[serde(default)] created_at: Option<String>,
    #[serde(default)] updated_at: Option<String>,
    #[serde(default)] session_created_reason: Option<String>,
}

pub struct SessionStorage {
    base_path: PathBuf,
}

impl SessionStorage {
    pub fn new() -> Result<Self> {
        let base_path = Self::get_storage_path()?;
        Ok(Self { base_path })
    }

    /// 验证路径组件是否安全（防止路径遍历）
    fn is_safe_path_component(component: &str) -> bool {
        // 只允许字母、数字、下划线、连字符和点号
        // 不允许路径分隔符和特殊字符
        !component.is_empty()
            && !component.contains("..")
            && !component.contains('/')
            && !component.contains('\\')
            && component.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
    }
    
    fn get_storage_path() -> Result<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            let appdata = std::env::var("APPDATA")
                .context("Failed to get APPDATA environment variable")?;
            Ok(PathBuf::from(appdata)
                .join("Kiro")
                .join("User")
                .join("globalStorage")
                .join("kiro.kiroagent")
                .join("workspace-sessions"))
        }
        
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var("HOME")
                .context("Failed to get HOME environment variable")?;
            Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Kiro")
                .join("User")
                .join("globalStorage")
                .join("kiro.kiroagent")
                .join("workspace-sessions"))
        }
        
        #[cfg(target_os = "linux")]
        {
            let home = std::env::var("HOME")
                .context("Failed to get HOME environment variable")?;
            Ok(PathBuf::from(home)
                .join(".config")
                .join("Kiro")
                .join("User")
                .join("globalStorage")
                .join("kiro.kiroagent")
                .join("workspace-sessions"))
        }
    }
    
    /// 列出所有 workspace
    pub fn list_workspaces(&self) -> Result<Vec<String>> {
        let mut workspaces = Vec::new();
        
        if !self.base_path.exists() {
            return Ok(workspaces);
        }
        
        // 收集工作区及其修改时间
        let mut workspace_with_time: Vec<(String, std::time::SystemTime)> = Vec::new();
        
        for entry in fs::read_dir(&self.base_path)
            .context("Failed to read workspace-sessions directory")? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                let modified = entry.metadata()?.modified()?;
                workspace_with_time.push((name, modified));
            }
        }
        
        // 按修改时间倒序排序（最近使用的在前）
        workspace_with_time.sort_by(|a, b| b.1.cmp(&a.1));
        
        // 只返回名称
        workspaces = workspace_with_time.into_iter().map(|(name, _)| name).collect();

        // CLI 会话按工作目录(cwd)分组为多个工作区，置顶
        let mut cli_ws: Vec<String> = Vec::new();
        for s in self.collect_cli_sessions(None) {
            let id = format!("{CLI_PREFIX}{}", s.workspace_directory);
            if !cli_ws.contains(&id) {
                cli_ws.push(id);
            }
        }
        cli_ws.extend(workspaces);
        workspaces = cli_ws;

        workspaces.extend(external_sessions::list_workspaces());

        Ok(workspaces)
    }
    
    /// 列出指定 workspace 的所有 sessions
    pub fn list_sessions(&self, workspace_hash: &str) -> Result<Vec<SessionSummary>> {
        if external_sessions::handles(workspace_hash) {
            return Ok(external_sessions::list_sessions(workspace_hash));
        }
        if let Some(cwd) = workspace_hash.strip_prefix(CLI_PREFIX) {
            return Ok(self.collect_cli_sessions(Some(cwd)));
        }
        // 安全检查：防止路径遍历攻击
        if !Self::is_safe_path_component(workspace_hash) {
            log::warn!("[安全] 检测到非法的 workspace_hash: {}", workspace_hash);
            return Err(anyhow::anyhow!("Invalid workspace hash"));
        }

        let workspace_path = self.base_path.join(workspace_hash);
        let mut sessions = Vec::new();
        
        if !workspace_path.exists() {
            log::warn!("Workspace directory does not exist: {}", workspace_hash);
            return Ok(sessions);
        }
        
        for entry in fs::read_dir(&workspace_path)
            .context(format!("Failed to read workspace directory: {}", workspace_hash))? {
            let entry = entry?;
            let path = entry.path();
            
            // 只处理 .json 文件
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            
            // 跳过 sessions.json（索引文件）
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                if filename == "sessions.json" {
                    continue;
                }
            }
            
            match self.load_session_summary(&path, workspace_hash) {
                Ok(summary) => sessions.push(summary),
                Err(e) => {
                    log::error!("Failed to load session from {:?}: {}", path, e);
                    // 继续处理其他文件
                }
            }
        }
        
        // 按修改时间倒序排序
        sessions.sort_by(|a, b| {
            b.modified_at.unwrap_or(0).cmp(&a.modified_at.unwrap_or(0))
        });
        
        Ok(sessions)
    }
    
    /// 加载 session 摘要
    fn load_session_summary(&self, path: &PathBuf, workspace_hash: &str) -> Result<SessionSummary> {
        let metadata = fs::metadata(path)
            .context(format!("Failed to read metadata for {:?}", path))?;

        // 安全检查：文件大小限制
        if metadata.len() > MAX_FILE_SIZE {
            return Err(anyhow::anyhow!("File too large: {} bytes", metadata.len()));
        }

        let content = fs::read_to_string(path)
            .context(format!("Failed to read file {:?}", path))?;

        let session: IdeSession = serde_json::from_str(&content)
            .map_err(|e| {
                log::error!("JSON parse error for {:?}: {}", path, e);
                // 打印前 500 个字符帮助调试
                log::error!("File content preview: {}", &content.chars().take(500).collect::<String>());
                e
            })
            .context(format!("Failed to parse JSON from {:?}", path))?;
        
        Ok(SessionSummary {
            session_id: session.session_id,
            title: session.title,
            session_type: session.session_type,
            workspace_directory: session.workspace_directory,
            workspace_hash: workspace_hash.to_string(),
            message_count: session.history.len(),
            file_size: metadata.len(),
            created_at: metadata.created().ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            modified_at: metadata.modified().ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            source: "ide".to_string(),
        })
    }
    
    /// 返回 session 文件在磁盘上的真实完整路径
    pub fn get_session_file_path(&self, workspace_hash: &str, session_id: &str) -> Result<String> {
        if external_sessions::handles(workspace_hash) {
            return external_sessions::file_path(workspace_hash, session_id);
        }
        if !Self::is_safe_path_component(session_id) {
            return Err(anyhow::anyhow!("Invalid session id"));
        }
        // CLI 会话：~/.kiro/sessions/cli/<id>.jsonl
        if workspace_hash.starts_with(CLI_PREFIX) {
            let dir = Self::cli_dir().ok_or_else(|| anyhow::anyhow!("No home directory"))?;
            return Ok(dir.join(format!("{session_id}.jsonl")).to_string_lossy().to_string());
        }
        // IDE 会话：<base>/<workspace_hash>/<id>.json
        if !Self::is_safe_path_component(workspace_hash) {
            return Err(anyhow::anyhow!("Invalid workspace hash"));
        }
        Ok(self.base_path
            .join(workspace_hash)
            .join(format!("{session_id}.json"))
            .to_string_lossy()
            .to_string())
    }

    /// 加载完整 session
    pub fn load_session(&self, workspace_hash: &str, session_id: &str) -> Result<IdeSession> {
        if external_sessions::handles(workspace_hash) {
            return external_sessions::load_session(workspace_hash, session_id);
        }
        if workspace_hash.starts_with(CLI_PREFIX) {
            return self.load_cli_session(session_id);
        }
        // 安全检查：防止路径遍历攻击
        if !Self::is_safe_path_component(workspace_hash) || !Self::is_safe_path_component(session_id) {
            log::warn!("[安全] 检测到非法的路径参数: workspace_hash={}, session_id={}", workspace_hash, session_id);
            return Err(anyhow::anyhow!("Invalid path parameters"));
        }

        let path = self.base_path
            .join(workspace_hash)
            .join(format!("{}.json", session_id));

        // 安全检查：文件大小限制
        let metadata = fs::metadata(&path)
            .context(format!("Failed to read metadata for session: {}", session_id))?;
        if metadata.len() > MAX_FILE_SIZE {
            return Err(anyhow::anyhow!("Session file too large: {} bytes", metadata.len()));
        }

        let content = fs::read_to_string(&path)
            .context(format!("Failed to read session file: {}", session_id))?;
        let session = serde_json::from_str(&content)
            .context("Failed to parse session JSON")?;
        Ok(session)
    }
    
    /// 删除 session
    pub fn delete_session(&self, workspace_hash: &str, session_id: &str) -> Result<()> {
        if external_sessions::handles(workspace_hash) {
            return external_sessions::delete_session(workspace_hash, session_id);
        }
        if workspace_hash.starts_with(CLI_PREFIX) {
            return self.delete_cli_session(session_id);
        }
        // 安全检查：防止路径遍历攻击
        if !Self::is_safe_path_component(workspace_hash) || !Self::is_safe_path_component(session_id) {
            log::warn!("[安全] 检测到非法的路径参数: workspace_hash={}, session_id={}", workspace_hash, session_id);
            return Err(anyhow::anyhow!("Invalid path parameters"));
        }

        let path = self.base_path
            .join(workspace_hash)
            .join(format!("{}.json", session_id));
        
        fs::remove_file(&path)
            .context(format!("Failed to delete session: {}", session_id))?;
        
        Ok(())
    }
    
    /// 删除整个工作区目录
    pub fn delete_workspace(&self, workspace_hash: &str) -> Result<()> {
        if external_sessions::handles(workspace_hash) {
            return external_sessions::delete_workspace(workspace_hash);
        }
        if let Some(cwd) = workspace_hash.strip_prefix(CLI_PREFIX) {
            return self.delete_cli_workspace(cwd);
        }
        // 安全检查：防止路径遍历攻击
        if !Self::is_safe_path_component(workspace_hash) {
            log::warn!("[安全] 检测到非法的 workspace_hash: {}", workspace_hash);
            return Err(anyhow::anyhow!("Invalid workspace hash"));
        }

        let workspace_path = self.base_path.join(workspace_hash);
        
        if workspace_path.exists() {
            fs::remove_dir_all(&workspace_path)
                .context(format!("Failed to delete workspace: {}", workspace_hash))?;
        }
        
        Ok(())
    }
    
    /// 导出 session
    pub fn export_session(
        &self,
        workspace_hash: &str,
        session_id: &str,
        format: ExportFormat,
    ) -> Result<String> {
        // 安全检查已在 load_session 中完成
        let session = self.load_session(workspace_hash, session_id)?;
        
        match format {
            ExportFormat::Json => {
                serde_json::to_string_pretty(&session)
                    .context("Failed to serialize session to JSON")
            }
            ExportFormat::Markdown => {
                Ok(self.session_to_markdown(&session))
            }
        }
    }
    
    fn session_to_markdown(&self, session: &IdeSession) -> String {
        let mut md = String::new();
        md.push_str(&format!("# {}\n\n", session.title));
        md.push_str(&format!("- **Session ID**: {}\n", session.session_id));
        md.push_str(&format!("- **Type**: {}\n", session.session_type));
        md.push_str(&format!("- **Workspace**: {}\n", session.workspace_directory));
        md.push_str(&format!("- **Messages**: {}\n\n", session.history.len()));
        md.push_str("---\n\n");
        
        for (i, item) in session.history.iter().enumerate() {
            md.push_str(&format!("## Message {}\n\n", i + 1));
            md.push_str(&format!("**{}**:\n\n", 
                if item.message.role == "user" { "User" } else { "Assistant" }
            ));
            
            for content in &item.message.content {
                md.push_str(&format!("{}\n\n", content.text));
            }
            
            md.push_str("---\n\n");
        }
        
        md
    }

    // ===== Kiro CLI 会话来源（~/.kiro/sessions/cli/）=====

    fn cli_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".kiro").join("sessions").join("cli"))
    }

    fn parse_iso_secs(s: &Option<String>) -> Option<i64> {
        s.as_ref()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|dt| dt.timestamp())
    }

    /// 统计 jsonl 中的消息数（Prompt + AssistantMessage）与文件大小
    fn cli_jsonl_stats(jsonl: &Path) -> (usize, u64) {
        let size = fs::metadata(jsonl).map(|m| m.len()).unwrap_or(0);
        if size == 0 || size > MAX_FILE_SIZE {
            return (0, size);
        }
        let count = fs::read_to_string(jsonl)
            .map(|c| c.lines().filter(|l| {
                serde_json::from_str::<serde_json::Value>(l)
                    .ok()
                    .and_then(|v| v.get("kind").and_then(|k| k.as_str())
                        .map(|k| k == "Prompt" || k == "AssistantMessage"))
                    .unwrap_or(false)
            }).count())
            .unwrap_or(0);
        (count, size)
    }

    /// 收集 CLI 会话（仅含真实对话的）；filter_cwd 为 Some 时只返回该工作目录下的会话
    fn collect_cli_sessions(&self, filter_cwd: Option<&str>) -> Vec<SessionSummary> {
        let dir = match Self::cli_dir() {
            Some(d) if d.is_dir() => d,
            _ => return Vec::new(),
        };
        let read = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let mut sessions = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else { continue };
            let Ok(meta) = serde_json::from_str::<CliSessionMeta>(&content) else { continue };
            if meta.session_id.is_empty() {
                continue;
            }
            let cwd = meta.cwd.clone().unwrap_or_default();
            if let Some(f) = filter_cwd {
                if cwd != f {
                    continue;
                }
            }
            let (msg_count, file_size) = Self::cli_jsonl_stats(&path.with_extension("jsonl"));
            // 仅展示有真实对话（含 Prompt/AssistantMessage）的会话，跳过纯元数据会话
            if msg_count == 0 {
                continue;
            }
            let file_mtime = entry.metadata().ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            let title = meta.title.filter(|t| !t.is_empty()).unwrap_or_else(|| meta.session_id.clone());
            sessions.push(SessionSummary {
                title,
                session_type: meta.session_created_reason.unwrap_or_else(|| "cli".to_string()),
                workspace_hash: format!("{CLI_PREFIX}{cwd}"),
                workspace_directory: cwd,
                message_count: msg_count,
                file_size,
                created_at: Self::parse_iso_secs(&meta.created_at),
                modified_at: Self::parse_iso_secs(&meta.updated_at).or(file_mtime),
                source: "cli".to_string(),
                session_id: meta.session_id,
            });
        }
        sessions.sort_by(|a, b| b.modified_at.unwrap_or(0).cmp(&a.modified_at.unwrap_or(0)));
        sessions
    }

    fn load_cli_session(&self, session_id: &str) -> Result<IdeSession> {
        if !Self::is_safe_path_component(session_id) {
            return Err(anyhow::anyhow!("Invalid session id"));
        }
        let dir = Self::cli_dir().ok_or_else(|| anyhow::anyhow!("No home directory"))?;
        let meta: CliSessionMeta = fs::read_to_string(dir.join(format!("{session_id}.json")))
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default();

        let jsonl_path = dir.join(format!("{session_id}.jsonl"));
        let mut history = Vec::new();
        if jsonl_path.exists() {
            let size = fs::metadata(&jsonl_path).map(|m| m.len()).unwrap_or(0);
            if size > MAX_FILE_SIZE {
                return Err(anyhow::anyhow!("Session file too large: {} bytes", size));
            }
            let content = fs::read_to_string(&jsonl_path)
                .context("Failed to read kiro-cli session transcript")?;
            for (i, line) in content.lines().enumerate() {
                if let Some(item) = Self::cli_line_to_history(line, i) {
                    history.push(item);
                }
            }
        }

        Ok(IdeSession {
            title: meta.title.filter(|t| !t.is_empty()).unwrap_or_else(|| session_id.to_string()),
            session_type: meta.session_created_reason.unwrap_or_else(|| "cli".to_string()),
            workspace_directory: meta.cwd.unwrap_or_default(),
            history,
            conversation_summary: None,
            session_id: session_id.to_string(),
        })
    }

    /// 把一行 jsonl 转成 HistoryItem；非对话行（ToolResults 等）返回 None
    fn cli_line_to_history(line: &str, idx: usize) -> Option<HistoryItem> {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        let role = match v.get("kind")?.as_str()? {
            "Prompt" => "user",
            "AssistantMessage" => "assistant",
            _ => return None,
        };
        let data = v.get("data")?;
        let id = data.get("message_id").and_then(|x| x.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("cli-{idx}"));
        let mut items = Vec::new();
        if let Some(arr) = data.get("content").and_then(|c| c.as_array()) {
            for c in arr {
                let text = match c.get("kind").and_then(|x| x.as_str()).unwrap_or("") {
                    "text" => c.get("data").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    "toolUse" => format!(
                        "🔧 调用工具: {}",
                        c.get("data").and_then(|d| d.get("name")).and_then(|x| x.as_str()).unwrap_or("?")
                    ),
                    _ => String::new(),
                };
                if !text.is_empty() {
                    items.push(ContentItem { content_type: "text".to_string(), text });
                }
            }
        }
        if items.is_empty() {
            return None;
        }
        Some(HistoryItem {
            message: Message { role: role.to_string(), content: items, is_hidden: false, id },
            context_items: Vec::new(),
            editor_state: serde_json::Value::Null,
            prompt_logs: Vec::new(),
        })
    }

    fn delete_cli_session(&self, session_id: &str) -> Result<()> {
        if !Self::is_safe_path_component(session_id) {
            return Err(anyhow::anyhow!("Invalid session id"));
        }
        let dir = Self::cli_dir().ok_or_else(|| anyhow::anyhow!("No home directory"))?;
        for ext in ["json", "jsonl", "history", "lock"] {
            let p = dir.join(format!("{session_id}.{ext}"));
            if p.exists() {
                let _ = fs::remove_file(&p);
            }
        }
        Ok(())
    }

    /// 删除某工作目录(cwd)下的所有 CLI 会话
    fn delete_cli_workspace(&self, cwd: &str) -> Result<()> {
        let dir = match Self::cli_dir() {
            Some(d) if d.is_dir() => d,
            _ => return Ok(()),
        };
        for entry in fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else { continue };
            let Ok(meta) = serde_json::from_str::<CliSessionMeta>(&content) else { continue };
            if meta.cwd.as_deref().unwrap_or_default() == cwd && !meta.session_id.is_empty() {
                let _ = self.delete_cli_session(&meta.session_id);
            }
        }
        Ok(())
    }
}

pub enum ExportFormat {
    Json,
    Markdown,
}

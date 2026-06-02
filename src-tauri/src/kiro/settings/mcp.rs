// MCP 配置文件读写

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use crate::utils::fs::atomic_write;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: HashMap<String, McpServer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub powers: Option<PowersMcpConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PowersMcpConfig {
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: HashMap<String, PowerMcpServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServer {
    Command(McpServerCommand),
    Url(McpServerUrl),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerCommand {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, rename = "autoApprove")]
    pub auto_approve: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerUrl {
    pub url: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, rename = "disabledTools")]
    pub disabled_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerMcpServer {
    pub url: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, rename = "disabledTools")]
    pub disabled_tools: Vec<String>,
}

impl McpConfig {
    /// 获取用户级 MCP 配置文件路径
    pub fn config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".kiro").join("settings").join("mcp.json"))
    }

    /// 获取项目级 MCP 配置文件路径
    pub fn project_config_path(project_dir: &str) -> PathBuf {
        PathBuf::from(project_dir)
            .join(".kiro")
            .join("settings")
            .join("mcp.json")
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path).map_err(|e| format!("读取配置文件失败: {e}"))?;

        serde_json::from_str(&content).map_err(|e| format!("解析配置文件失败: {e}"))
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }

        let content =
            serde_json::to_string_pretty(self).map_err(|e| format!("序列化配置失败: {e}"))?;

        atomic_write(path, &content, "MCP 配置文件")
    }

    /// 读取用户级配置（写操作使用）
    pub fn load() -> Result<Self, String> {
        let path = Self::config_path().ok_or("无法获取用户目录")?;
        Self::load_from_path(&path)
    }

    /// 读取有效配置：user -> workspace -> powers（后者覆盖前者）
    pub fn load_merged(project_dir: Option<&str>) -> Result<Self, String> {
        let user_path = Self::config_path().ok_or("无法获取用户目录")?;
        let user_config = Self::load_from_path(&user_path)?;

        let workspace_config = if let Some(pd) = project_dir {
            let ws_path = Self::project_config_path(pd);
            Self::load_from_path(&ws_path)?
        } else {
            Self::default()
        };

        let mut merged_servers = user_config.mcp_servers.clone();
        for (name, server) in workspace_config.mcp_servers {
            merged_servers.insert(name, server);
        }

        if let Some(powers) = user_config.powers.as_ref() {
            for (name, server) in &powers.mcp_servers {
                merged_servers.insert(
                    name.clone(),
                    McpServer::Url(McpServerUrl {
                        url: server.url.clone(),
                        disabled: server.disabled,
                        disabled_tools: server.disabled_tools.clone(),
                    }),
                );
            }
        }

        Ok(Self {
            mcp_servers: merged_servers,
            powers: user_config.powers,
        })
    }

    /// 保存用户级配置文件
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path().ok_or("无法获取用户目录")?;
        self.save_to_path(&path)
    }
}

impl From<PowerMcpServer> for McpServerUrl {
    fn from(value: PowerMcpServer) -> Self {
        Self {
            url: value.url,
            disabled: value.disabled,
            disabled_tools: value.disabled_tools,
        }
    }
}

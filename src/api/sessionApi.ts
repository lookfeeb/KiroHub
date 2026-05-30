import { invoke } from '@tauri-apps/api/core'
import { IdeSession, SessionSummary } from '@/types/session'

export const sessionApi = {
  // 列出所有 workspace
  async listWorkspaces(): Promise<string[]> {
    return invoke('list_workspaces')
  },

  // 列出指定 workspace 的 sessions
  async listSessions(workspaceHash: string): Promise<SessionSummary[]> {
    return invoke('list_sessions', { workspaceHash })
  },

  // 加载完整 session
  async loadSession(workspaceHash: string, sessionId: string): Promise<IdeSession> {
    return invoke('load_session', { workspaceHash, sessionId })
  },

  // 删除 session
  async deleteSession(workspaceHash: string, sessionId: string): Promise<void> {
    return invoke('delete_session', { workspaceHash, sessionId })
  },

  // 获取 session 文件的真实磁盘路径
  async getSessionFilePath(workspaceHash: string, sessionId: string): Promise<string> {
    return invoke('get_session_file_path', { workspaceHash, sessionId })
  },

  // 删除整个 workspace
  async deleteWorkspace(workspaceHash: string): Promise<void> {
    return invoke('delete_workspace', { workspaceHash })
  },

  // 导出 session
  async exportSession(
    workspaceHash: string,
    sessionId: string,
    format: 'json' | 'markdown'
  ): Promise<string> {
    return invoke('export_session', { workspaceHash, sessionId, format })
  },

  // 搜索 sessions
  async searchSessions(query: string): Promise<SessionSummary[]> {
    return invoke('search_sessions', { query })
  },
}

export interface IdeSession {
  sessionId: string
  title: string
  sessionType: string
  workspaceDirectory: string
  history: HistoryItem[]
  conversationSummary?: string
}

export interface SessionSummary {
  sessionId: string
  title: string
  sessionType: string
  workspaceDirectory: string
  workspaceHash: string
  messageCount: number
  fileSize: number
  createdAt?: number
  modifiedAt?: number
  source?: 'ide' | 'cli' | 'codex' | 'claude' | 'antigravity' | 'antigravity-ide'
}

export interface SessionTree {
  workspaces: string[]
  sessionsByWorkspace: Record<string, SessionSummary[]>
}

export interface HistoryItem {
  message: Message
  contextItems: any[]
  editorState: any
  promptLogs: PromptLog[]
}

export interface Message {
  role: 'user' | 'assistant' | 'system' | 'artifact'
  content: ContentItem[]
  id: string
}

export interface ContentItem {
  type: string
  text: string
}

export interface PromptLog {
  modelTitle: string
  prompt: string
  completion: string
  completionOptions: any
}

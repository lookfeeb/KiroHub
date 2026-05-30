import { useState, useEffect } from 'react'
import { sessionApi } from '@/api/sessionApi'
import { SessionSummary, IdeSession } from '@/types/session'
import { Card } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Checkbox } from '@/components/ui/checkbox'
import { Loader2, Search, Trash2, Download, MessageSquare, ChevronRight, ChevronDown, Terminal, Monitor, Folder } from 'lucide-react'
import { save } from '@tauri-apps/plugin-dialog'
import { writeTextFile } from '@tauri-apps/plugin-fs'
import { useDialog } from '@/contexts/DialogContext'
import { showSuccess, showError, showWarning } from '@/utils/toast'
import { useTranslation } from 'react-i18next'

export default function SessionManager() {
  const { t } = useTranslation()
  const { showConfirm } = useDialog()
  const [workspaces, setWorkspaces] = useState<string[]>([])
  const [selectedWorkspace, setSelectedWorkspace] = useState<string | null>(null)
  const [expandedWorkspaces, setExpandedWorkspaces] = useState<Set<string>>(new Set())
  const [workspaceSessions, setWorkspaceSessions] = useState<Map<string, SessionSummary[]>>(new Map())
  const [selectedSession, setSelectedSession] = useState<IdeSession | null>(null)
  const [selectedWorkspaceHash, setSelectedWorkspaceHash] = useState<string>('')
  const [loading, setLoading] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [selectedWorkspaceHashes, setSelectedWorkspaceHashes] = useState<Set<string>>(new Set())
  const [expandedSources, setExpandedSources] = useState<Set<string>>(new Set(['ide', 'cli']))

  // 加载 workspaces
  useEffect(() => {
    loadWorkspaces()
  }, [])

  const toggleWorkspace = async (workspaceHash: string) => {
    const newExpanded = new Set(expandedWorkspaces)

    if (newExpanded.has(workspaceHash)) {
      // 折叠
      newExpanded.delete(workspaceHash)
    } else {
      // 展开 - 加载该工作区的 sessions
      newExpanded.add(workspaceHash)
      if (!workspaceSessions.has(workspaceHash)) {
        await loadSessionsForWorkspace(workspaceHash)
      }
    }

    setExpandedWorkspaces(newExpanded)
  }

  const loadSessionsForWorkspace = async (workspaceHash: string) => {
    try {
      const data = await sessionApi.listSessions(workspaceHash)
      setWorkspaceSessions(prev => new Map(prev).set(workspaceHash, data))
    } catch (error) {
      console.error('Failed to load sessions:', error)
      showError('加载会话列表失败：' + error)
    }
  }

  const sourceOf = (hash: string): 'ide' | 'cli' => (hash.startsWith('cli:') ? 'cli' : 'ide')

  const toggleSource = (src: string) => {
    setExpandedSources(prev => {
      const n = new Set(prev)
      if (n.has(src)) n.delete(src)
      else n.add(src)
      return n
    })
  }

  const decodeWorkspaceName = (hash: string) => {
    if (hash.startsWith('cli:')) {
      const cwd = hash.slice(4)
      const parts = cwd.split(/[/\\]/).filter(Boolean)
      return parts[parts.length - 1] || cwd || 'CLI'
    }
    try {
      // 移除末尾的 __ 或 _
      const cleaned = hash.replace(/_+$/, '')
      // Base64 解码
      const decoded = atob(cleaned)
      // 提取最后一个路径段作为显示名称
      const parts = decoded.split(/[/\\]/)
      const name = parts[parts.length - 1] || parts[parts.length - 2] || decoded
      return name
    } catch {
      return hash
    }
  }

  const loadWorkspaces = async () => {
    try {
      setLoading(true)
      const data = await sessionApi.listWorkspaces()
      setWorkspaces(data)
      // 预取每个工作区的会话，使计数全自动显示（无需展开）
      data.forEach(ws => { loadSessionsForWorkspace(ws) })
    } catch (error) {
      console.error('Failed to load workspaces:', error)
      showError('加载工作区失败：' + error)
    } finally {
      setLoading(false)
    }
  }

  const handleSelectSession = async (workspaceHash: string, session: SessionSummary) => {
    // 如果点击的是当前已选中的 session，不重复加载
    if (selectedSession?.sessionId === session.sessionId) {
      return
    }

    try {
      setLoading(true)
      setSelectedSession(null) // 先清空，避免显示旧数据
      const data = await sessionApi.loadSession(workspaceHash, session.sessionId)
      setSelectedSession(data)
      setSelectedWorkspaceHash(workspaceHash)
    } catch (error) {
      console.error('Failed to load session:', error)
      showError('加载失败：' + error)
    } finally {
      setLoading(false)
    }
  }

  const handleDeleteWorkspace = async (workspaceHash: string) => {
    const workspaceName = decodeWorkspaceName(workspaceHash)

    const confirmed = await showConfirm(
      '删除工作区',
      `确定要删除工作区 "${workspaceName}" 及其所有会话吗？\n\n此操作不可恢复！`
    )

    if (!confirmed) return

    try {
      setLoading(true)

      // 直接删除整个工作区目录
      await sessionApi.deleteWorkspace(workspaceHash)

      // 重新加载工作区列表
      await loadWorkspaces()

      // 清空相关状态
      setExpandedWorkspaces(prev => {
        const newSet = new Set(prev)
        newSet.delete(workspaceHash)
        return newSet
      })
      setWorkspaceSessions(prev => {
        const newMap = new Map(prev)
        newMap.delete(workspaceHash)
        return newMap
      })
      if (selectedWorkspace === workspaceHash) {
        setSelectedWorkspace(null)
        setSelectedSession(null)
      }

      showSuccess(`成功删除工作区 "${workspaceName}"`)
    } catch (error) {
      console.error('Failed to delete workspace:', error)
      showError('删除工作区失败：' + error)
    } finally {
      setLoading(false)
    }
  }

  const handleDeleteSession = async (workspaceHash: string, session: SessionSummary) => {
    const confirmed = await showConfirm(
      '删除会话',
      `确定要删除会话 "${session.title}" 吗？`
    )

    if (!confirmed) return

    try {
      await sessionApi.deleteSession(session.workspaceHash, session.sessionId)

      // 重新加载该工作区的会话列表
      await loadSessionsForWorkspace(workspaceHash)

      // 如果删除的是当前选中的 session，清空详情
      if (selectedSession?.sessionId === session.sessionId) {
        setSelectedSession(null)
      }
      showSuccess('会话已删除')
    } catch (error) {
      console.error('Failed to delete session:', error)
      showError('删除失败：' + error)
    }
  }

  const toggleWorkspaceSelection = (workspaceHash: string) => {
    const newSelected = new Set(selectedWorkspaceHashes)
    if (newSelected.has(workspaceHash)) {
      newSelected.delete(workspaceHash)
    } else {
      newSelected.add(workspaceHash)
    }
    setSelectedWorkspaceHashes(newSelected)
  }

  const toggleSelectAllWorkspaces = () => {
    if (selectedWorkspaceHashes.size === workspaces.length) {
      setSelectedWorkspaceHashes(new Set())
    } else {
      setSelectedWorkspaceHashes(new Set(workspaces))
    }
  }

  const handleBatchDeleteWorkspaces = async () => {
    if (selectedWorkspaceHashes.size === 0) {
      showWarning('请先选择要删除的工作区')
      return
    }

    const workspaceNames = Array.from(selectedWorkspaceHashes)
      .map(hash => decodeWorkspaceName(hash))
      .join('、')

    const confirmed = await showConfirm(
      '批量删除工作区',
      `确定要删除选中的 ${selectedWorkspaceHashes.size} 个工作区及其所有会话吗？\n\n工作区：${workspaceNames}\n\n此操作不可恢复！`
    )

    if (!confirmed) return

    try {
      setLoading(true)

      // 直接删除所有选中的工作区目录
      for (const workspaceHash of selectedWorkspaceHashes) {
        await sessionApi.deleteWorkspace(workspaceHash)
      }

      // 重新加载工作区列表
      await loadWorkspaces()

      // 清空相关状态
      setExpandedWorkspaces(new Set())
      setWorkspaceSessions(new Map())
      setSelectedWorkspaceHashes(new Set())
      setSelectedWorkspace(null)
      setSelectedSession(null)

      showSuccess(`成功删除 ${selectedWorkspaceHashes.size} 个工作区`)
    } catch (error) {
      console.error('Failed to batch delete workspaces:', error)
      showError('批量删除失败：' + error)
    } finally {
      setLoading(false)
    }
  }

  const handleExportSession = async (format: 'json' | 'markdown') => {
    if (!selectedSession || !selectedWorkspaceHash) return

    try {
      const content = await sessionApi.exportSession(
        selectedWorkspaceHash,
        selectedSession.sessionId,
        format
      )

      const ext = format === 'json' ? 'json' : 'md'
      const defaultPath = `${selectedSession.title}.${ext}`

      const filePath = await save({
        defaultPath,
        filters: [{
          name: format === 'json' ? 'JSON' : 'Markdown',
          extensions: [ext]
        }]
      })

      if (filePath) {
        await writeTextFile(filePath, content)
        showSuccess('导出成功！')
      }
    } catch (error) {
      console.error('Failed to export session:', error)
      showError('导出失败：' + error)
    }
  }

  const filteredSessions = searchQuery
    ? Array.from(workspaceSessions.values())
      .flat()
      .filter(session => session.title.toLowerCase().includes(searchQuery.toLowerCase()))
    : []

  const formatFileSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  const formatDate = (timestamp?: number) => {
    if (!timestamp) return '-'
    return new Date(timestamp * 1000).toLocaleString('zh-CN')
  }

  // 清洗会话标题：去除 HTML/代码标签、压缩空白、截断
  const cleanTitle = (raw?: string) => {
    if (!raw) return '未命名会话'
    let text = raw
      .replace(/<\/?[a-zA-Z][^>]*>?/g, ' ')   // 去 HTML 标签（含未闭合的 <button ...）
      .replace(/&[a-zA-Z#0-9]+;/g, ' ')        // 去 HTML 实体
      .replace(/\s+/g, ' ')
      .trim()
    if (!text) return '未命名会话'
    return text.length > 60 ? text.slice(0, 60) + '…' : text
  }

  const renderSourceBadge = (s?: string) => (
    <Badge
      variant="outline"
      className={`text-xs h-4 px-1.5 ${s === 'cli'
        ? 'border-purple-500/40 text-purple-600 dark:text-purple-400'
        : 'border-blue-500/40 text-blue-600 dark:text-blue-400'}`}
    >
      {s === 'cli' ? 'CLI' : 'IDE'}
    </Badge>
  )

  // 获取工作区的会话列表
  const getWorkspaceSessions = (workspaceHash: string) => {
    return workspaceSessions.get(workspaceHash) || []
  }

  return (
    <div className="flex flex-col h-full glass-main">
      {/* Header（紧凑）*/}
      <div className="px-5 py-3 border-b border-border flex items-center gap-2.5">
        <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-primary/80 to-primary flex items-center justify-center shadow-md ring-1 ring-primary/20">
          <MessageSquare size={20} className="text-primary-foreground" />
        </div>
        <div className="flex flex-col">
          <h1 className="text-lg font-semibold text-foreground leading-tight">会话管理</h1>
          <p className="text-sm text-muted-foreground leading-tight">{t('session.subtitle')}</p>
        </div>
      </div>

      <div className="flex-1 flex overflow-hidden">
        {/* Left Sidebar - Workspaces with expandable sessions */}
        <div className="w-72 border-r border-border flex flex-col">
          <div className="p-3 border-b border-border space-y-2">
            <div className="flex items-center justify-between">
              <h2 className="text-xs font-semibold text-foreground">工作区与会话</h2>
              {selectedWorkspaceHashes.size > 0 && (
                <Button
                  variant="destructive"
                  size="sm"
                  className="h-6 text-[11px]"
                  onClick={handleBatchDeleteWorkspaces}
                >
                  <Trash2 className="h-3 w-3 mr-1" />
                  删除 ({selectedWorkspaceHashes.size})
                </Button>
              )}
            </div>
            <div className="flex items-center justify-between text-[11px] text-muted-foreground">
              <span>{workspaces.length} 个工作区</span>
              {workspaces.length > 0 && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-5 px-2 text-[11px]"
                  onClick={toggleSelectAllWorkspaces}
                >
                  {selectedWorkspaceHashes.size === workspaces.length ? '取消全选' : '全选'}
                </Button>
              )}
            </div>
            {/* Search */}
            <div className="relative">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                placeholder="搜索会话..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="h-8 pl-8 text-xs"
              />
            </div>
          </div>

          <ScrollArea className="flex-1 w-72">
            <div className="p-2 space-y-1 overflow-hidden">
              {/* 搜索模式：显示所有匹配的会话 */}
              {searchQuery && (
                <div className="space-y-2">
                  {filteredSessions.length === 0 ? (
                    <div className="text-center py-8 text-muted-foreground text-sm">
                      未找到匹配的会话
                    </div>
                  ) : (
                    filteredSessions.map(session => (
                      <Card
                        key={session.sessionId}
                        className={`p-3 cursor-pointer hover:bg-accent transition-colors ${selectedSession?.sessionId === session.sessionId ? 'bg-accent' : ''
                          }`}
                        onClick={() => handleSelectSession(session.workspaceHash, session)}
                      >
                        <div className="space-y-2">
                          <div className="flex items-start justify-between gap-2">
                            <div className="flex-1 min-w-0">
                              <h3 className="font-medium text-sm line-clamp-2">
                                {session.title}
                              </h3>
                              <p className="text-xs text-muted-foreground truncate mt-1">
                                {decodeWorkspaceName(session.workspaceHash)}
                              </p>
                            </div>
                            <Button
                              variant="ghost"
                              size="icon"
                              className="h-6 w-6 shrink-0 hover:bg-destructive hover:text-destructive-foreground"
                              onClick={(e) => {
                                e.stopPropagation()
                                handleDeleteSession(session.workspaceHash, session)
                              }}
                              title="删除会话"
                            >
                              <Trash2 className="h-3 w-3" />
                            </Button>
                          </div>
                          <div className="flex items-center gap-2 flex-wrap">
                            {renderSourceBadge(session.source)}
                            <Badge variant="secondary" className="text-xs">
                              {session.sessionType}
                            </Badge>
                            <span className="text-xs text-muted-foreground flex items-center gap-1">
                              <MessageSquare className="h-3 w-3" />
                              {session.messageCount}
                            </span>
                            <span className="text-xs text-muted-foreground">
                              {formatFileSize(session.fileSize)}
                            </span>
                          </div>
                        </div>
                      </Card>
                    ))
                  )}
                </div>
              )}

              {/* 正常模式：树形列表 —— 来源分组 → 工作区 → 会话 */}
              {!searchQuery && (['cli', 'ide'] as const).map(src => {
                const groupWorkspaces = workspaces.filter(w => sourceOf(w) === src)
                if (groupWorkspaces.length === 0) return null
                const groupOpen = expandedSources.has(src)
                return (
                  <div key={src}>
                    {/* 分组头 */}
                    <button
                      onClick={() => toggleSource(src)}
                      className="w-full flex items-center gap-1.5 px-1.5 h-7 rounded-md hover:bg-muted/60 transition-colors"
                    >
                      <ChevronRight className={`h-3.5 w-3.5 text-muted-foreground/70 shrink-0 transition-transform ${groupOpen ? 'rotate-90' : ''}`} />
                      {src === 'cli'
                        ? <Terminal className="h-3.5 w-3.5 text-primary shrink-0" />
                        : <Monitor className="h-3.5 w-3.5 text-primary shrink-0" />}
                      <span className="text-xs font-semibold text-foreground">{src === 'cli' ? 'Kiro CLI' : 'Kiro IDE'}</span>
                      <span className="text-[10px] text-muted-foreground ml-auto tabular-nums">{groupWorkspaces.length}</span>
                    </button>

                    {/* 工作区 + 会话 */}
                    {groupOpen && (
                      <div className="mt-0.5">
                        {groupWorkspaces.map(workspace => {
                          const isExpanded = expandedWorkspaces.has(workspace)
                          const sessions = getWorkspaceSessions(workspace)
                          const checked = selectedWorkspaceHashes.has(workspace)
                          return (
                            <div key={workspace}>
                              {/* 工作区行 */}
                              <div
                                className="group flex items-center h-8 pl-4 pr-1.5 rounded-md cursor-pointer hover:bg-muted/50 transition-colors"
                                onClick={() => toggleWorkspace(workspace)}
                                title={workspace}
                              >
                                <ChevronRight className={`h-3.5 w-3.5 text-muted-foreground/70 shrink-0 transition-transform ${isExpanded ? 'rotate-90' : ''}`} />
                                <Checkbox
                                  checked={checked}
                                  onCheckedChange={() => toggleWorkspaceSelection(workspace)}
                                  onClick={(e) => e.stopPropagation()}
                                  className={`ml-1 shrink-0 cursor-pointer transition-opacity ${checked ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'}`}
                                />
                                <Folder className="h-3.5 w-3.5 text-muted-foreground shrink-0 ml-1.5" />
                                <span className="ml-1.5 flex-1 min-w-0 truncate text-xs font-medium text-foreground">
                                  {decodeWorkspaceName(workspace)}
                                </span>
                                {sessions.length > 0 && (
                                  <span className="text-[10px] text-muted-foreground tabular-nums mr-1 shrink-0">{sessions.length}</span>
                                )}
                                <button
                                  className="h-5 w-5 shrink-0 rounded inline-flex items-center justify-center text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-destructive hover:text-destructive-foreground transition-all"
                                  onClick={(e) => { e.stopPropagation(); handleDeleteWorkspace(workspace) }}
                                  title="删除工作区"
                                >
                                  <Trash2 className="h-3 w-3" />
                                </button>
                              </div>

                              {/* 会话条目 */}
                              {isExpanded && (
                                <div>
                                  {loading && sessions.length === 0 ? (
                                    <div className="flex items-center justify-center py-3 pl-10">
                                      <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
                                    </div>
                                  ) : sessions.length === 0 ? (
                                    <div className="text-[11px] text-muted-foreground/70 py-1.5 pl-10">暂无会话</div>
                                  ) : (
                                    sessions.map(session => {
                                      const active = selectedSession?.sessionId === session.sessionId
                                      return (
                                        <div
                                          key={session.sessionId}
                                          className={`group relative flex items-center h-8 pl-10 pr-1.5 cursor-pointer transition-colors before:absolute before:left-[26px] before:top-1/2 before:-translate-y-1/2 before:h-1.5 before:w-1.5 before:rounded-full ${active
                                            ? 'bg-primary/10 before:bg-primary'
                                            : 'hover:bg-muted/50 before:bg-muted-foreground/30'}`}
                                          onClick={() => handleSelectSession(workspace, session)}
                                          title={cleanTitle(session.title)}
                                        >
                                          <span className={`flex-1 min-w-0 truncate text-xs ${active ? 'text-primary font-medium' : 'text-foreground/90'}`}>
                                            {cleanTitle(session.title)}
                                          </span>
                                          <span className="flex items-center gap-0.5 text-[10px] text-muted-foreground tabular-nums ml-2 shrink-0 group-hover:opacity-0 transition-opacity">
                                            <MessageSquare className="h-2.5 w-2.5" />{session.messageCount}
                                          </span>
                                          <button
                                            className="absolute right-1.5 h-5 w-5 rounded inline-flex items-center justify-center text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-destructive hover:text-destructive-foreground transition-all"
                                            onClick={(e) => { e.stopPropagation(); handleDeleteSession(workspace, session) }}
                                            title="删除会话"
                                          >
                                            <Trash2 className="h-3 w-3" />
                                          </button>
                                        </div>
                                      )
                                    })
                                  )}
                                </div>
                              )}
                            </div>
                          )
                        })}
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          </ScrollArea>
        </div>

        {/* Right Panel - Session Detail */}
        <div className="flex-1 flex flex-col">
          {loading && selectedSession === null ? (
            <div className="flex-1 flex items-center justify-center">
              <Loader2 className="h-8 w-8 animate-spin" />
            </div>
          ) : selectedSession ? (
            <>
              <div className="px-4 py-3 border-b border-border flex items-start justify-between gap-3">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1 flex-wrap">
                    {renderSourceBadge(sourceOf(selectedWorkspaceHash))}
                    <span className="text-[11px] text-muted-foreground flex items-center gap-1 min-w-0 max-w-[280px]">
                      <Folder className="h-3 w-3 shrink-0" />
                      <span className="truncate" title={selectedSession.workspaceDirectory}>
                        {selectedSession.workspaceDirectory.split(/[/\\]/).filter(Boolean).pop() || selectedSession.workspaceDirectory || '—'}
                      </span>
                    </span>
                    <span className="text-[11px] text-muted-foreground flex items-center gap-1 shrink-0">
                      <MessageSquare className="h-3 w-3" />
                      {selectedSession.history.length}
                    </span>
                  </div>
                  <h2
                    className="text-sm font-semibold text-foreground line-clamp-2 leading-snug cursor-pointer hover:underline"
                    title="点击复制会话文件路径"
                    onClick={async () => {
                      try {
                        const path = await sessionApi.getSessionFilePath(selectedWorkspaceHash, selectedSession.sessionId)
                        await navigator.clipboard.writeText(path)
                        showSuccess('已复制文件路径')
                      } catch (e) {
                        showError('复制失败：' + String(e))
                      }
                    }}
                  >
                    {selectedSession.title}
                  </h2>
                </div>
                <div className="flex gap-1.5 shrink-0">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs"
                    onClick={() => handleExportSession('json')}
                  >
                    <Download className="h-3.5 w-3.5 mr-1" />
                    JSON
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs"
                    onClick={() => handleExportSession('markdown')}
                  >
                    <Download className="h-3.5 w-3.5 mr-1" />
                    Markdown
                  </Button>
                </div>
              </div>

              <ScrollArea className="flex-1">
                <div className="p-4 space-y-4 max-w-4xl">
                  {/* Conversation Summary - 从第一条消息中提取 */}
                  {selectedSession.history.length > 0 &&
                    selectedSession.history[0].message.role === 'user' &&
                    selectedSession.history[0].message.content.length > 0 &&
                    (selectedSession.history[0].message.content[0].text.includes('CONTEXT TRANSFER') ||
                      selectedSession.history[0].message.content[0].text.includes('## TASK') ||
                      selectedSession.title.includes('(Continued)')) && (
                      <Card className="p-4 bg-blue-50 dark:bg-blue-950 border-blue-200 dark:border-blue-800">
                        <div className="flex items-start gap-3">
                          <div className="text-2xl shrink-0">📝</div>
                          <div className="flex-1 min-w-0">
                            <div className="font-medium mb-2 text-blue-900 dark:text-blue-100">
                              对话摘要（上下文压缩）
                            </div>
                            <div className="text-sm text-blue-800 dark:text-blue-200 whitespace-pre-wrap break-words">
                              {selectedSession.history[0].message.content[0].text}
                            </div>
                          </div>
                        </div>
                      </Card>
                    )}

                  {/* Messages */}
                  {selectedSession.history.length === 0 ? (
                    <div className="text-center py-8 text-muted-foreground">
                      此会话没有消息
                    </div>
                  ) : (
                    selectedSession.history.map((item, index) => {
                      // 跳过第一条摘要消息（如果是压缩会话）
                      const isSummaryMessage = index === 0 &&
                        item.message.role === 'user' &&
                        item.message.content.length > 0 &&
                        (item.message.content[0].text.includes('CONTEXT TRANSFER') ||
                          item.message.content[0].text.includes('## TASK') ||
                          selectedSession.title.includes('(Continued)'))

                      if (isSummaryMessage) {
                        return null
                      }

                      return (
                        <Card key={item.message.id} className="p-4">
                          <div className="flex items-start gap-3">
                            <div className="text-2xl shrink-0">
                              {item.message.role === 'user' ? '👤' : '🤖'}
                            </div>
                            <div className="flex-1 min-w-0">
                              <div className="font-medium mb-2">
                                {item.message.role === 'user' ? 'User' : 'Assistant'}
                              </div>
                              {item.message.content.map((content, i) => (
                                <div key={i} className="whitespace-pre-wrap text-sm break-words">
                                  {content.text}
                                </div>
                              ))}
                            </div>
                          </div>
                        </Card>
                      )
                    })
                  )}
                </div>
              </ScrollArea>
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center">
              <div className="text-center">
                <MessageSquare className="h-12 w-12 mx-auto text-muted-foreground mb-4" />
                <p className="text-muted-foreground">选择一个会话查看详情</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

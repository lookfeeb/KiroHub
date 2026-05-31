import { useState, useEffect } from 'react'
import { sessionApi } from '@/api/sessionApi'
import { SessionSummary, IdeSession } from '@/types/session'
import { Card } from '@/components/ui/card'
import Markdown from '../../shared/Markdown'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Checkbox } from '@/components/ui/checkbox'
import { Loader2, Search, Trash2, MessageSquare, ChevronRight, ChevronDown, Folder, Copy, FileJson, FileText, RefreshCw, Check } from 'lucide-react'
import { save } from '@tauri-apps/plugin-dialog'
import { writeTextFile } from '@tauri-apps/plugin-fs'
import { useDialog } from '@/contexts/DialogContext'
import { showSuccess, showError, showWarning } from '@/utils/toast'
import { useTranslation } from '../../../i18n'

// 顶层分组：Kiro(IDE+CLI) 合并为一组，其余各 AI 工具各自一组
const PLATFORMS: { key: string; label: string; sources: string[]; color: string }[] = [
  { key: 'kiro', label: 'Kiro 对话历史', sources: ['ide', 'cli'], color: 'text-blue-600 dark:text-blue-400' },
  { key: 'codex', label: 'Codex 对话历史', sources: ['codex'], color: 'text-emerald-600 dark:text-emerald-400' },
  { key: 'claude', label: 'Claude 对话历史', sources: ['claude'], color: 'text-orange-600 dark:text-orange-400' },
  { key: 'antigravity', label: 'Antigravity 对话历史', sources: ['antigravity'], color: 'text-cyan-600 dark:text-cyan-400' },
]

// 来源徽标样式
const SOURCE_META: Record<string, { label: string; cls: string }> = {
  cli: { label: 'CLI', cls: 'border-purple-500/40 text-purple-600 dark:text-purple-400' },
  ide: { label: 'IDE', cls: 'border-blue-500/40 text-blue-600 dark:text-blue-400' },
  codex: { label: 'Codex', cls: 'border-emerald-500/40 text-emerald-600 dark:text-emerald-400' },
  claude: { label: 'Claude', cls: 'border-orange-500/40 text-orange-600 dark:text-orange-400' },
  antigravity: { label: 'Gemini', cls: 'border-cyan-500/40 text-cyan-600 dark:text-cyan-400' },
}

// 带前缀的来源（ide 无前缀，作为兜底）；由 SOURCE_META 派生，新增来源只改 SOURCE_META/PLATFORMS
const PREFIXES = Object.keys(SOURCE_META).filter(k => k !== 'ide').map(k => `${k}:`)

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
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const [refreshOk, setRefreshOk] = useState(false)
  const [refreshing, setRefreshing] = useState(false)

  // 静默刷新：不切换全局 loading，刷新完成后短暂显示绿色勾
  const handleRefresh = async () => {
    if (refreshing) return
    setRefreshing(true)
    try {
      await sessionApi.refreshSessionCache()
      const data = await sessionApi.listWorkspaces()
      setWorkspaces(data)
      await Promise.all(data.map(ws => loadSessionsForWorkspace(ws)))
      setRefreshOk(true)
      setTimeout(() => setRefreshOk(false), 1500)
    } catch (error) {
      showError('刷新失败：' + error)
    } finally {
      setRefreshing(false)
    }
  }

  // 加载 workspaces
  useEffect(() => {
    loadWorkspaces()
  }, [])

  const toggleDir = (key: string) => {
    setExpandedWorkspaces(prev => {
      const n = new Set(prev)
      if (n.has(key)) n.delete(key)
      else n.add(key)
      return n
    })
  }

  const toggleGroup = (key: string) => {
    setCollapsedGroups(prev => {
      const n = new Set(prev)
      if (n.has(key)) n.delete(key)
      else n.add(key)
      return n
    })
  }

  const toggleSelectHashes = (hashes: string[]) => {
    setSelectedWorkspaceHashes(prev => {
      const n = new Set(prev)
      const all = hashes.length > 0 && hashes.every(h => n.has(h))
      hashes.forEach(h => all ? n.delete(h) : n.add(h))
      return n
    })
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

  const sourceOf = (hash: string): string => {
    const p = PREFIXES.find(p => hash.startsWith(p))
    return p ? p.slice(0, -1) : 'ide'
  }

  // 工作目录（合并同源同目录）：取解码后的完整路径并归一化作为分组键
  const dirPathOf = (hash: string): string => {
    const p = PREFIXES.find(p => hash.startsWith(p))
    if (p) return hash.slice(p.length)
    try { return atob(hash.replace(/_+$/, '')) } catch { return hash }
  }
  const dirKeyOf = (hash: string) =>
    dirPathOf(hash).replace(/[\\/]+$/, '').replace(/\\/g, '/').toLowerCase()

  const decodeWorkspaceName = (hash: string) => {
    const p = PREFIXES.find(p => hash.startsWith(p))
    if (p) {
      const cwd = hash.slice(p.length)
      const parts = cwd.split(/[/\\]/).filter(Boolean)
      return parts[parts.length - 1] || cwd || (SOURCE_META[p.slice(0, -1)]?.label ?? p.slice(0, -1))
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

  const handleDeleteDir = async (hashes: string[], name: string, platformLabel: string, platformColor: string) => {
    const confirmed = await showConfirm(
      '删除工作目录',
      <>确定要删除 <span className={`font-semibold ${platformColor}`}>{platformLabel}</span> 工作区 “{name}” 及其所有会话吗？{'\n\n'}此操作不可恢复！</>
    )
    if (!confirmed) return
    try {
      setLoading(true)
      for (const h of hashes) await sessionApi.deleteWorkspace(h)
      await loadWorkspaces()
      setExpandedWorkspaces(prev => { const n = new Set(prev); n.delete(dirKeyOf(hashes[0])); return n })
      setWorkspaceSessions(prev => { const m = new Map(prev); hashes.forEach(h => m.delete(h)); return m })
      setSelectedWorkspaceHashes(prev => { const n = new Set(prev); hashes.forEach(h => n.delete(h)); return n })
      if (selectedWorkspace && hashes.includes(selectedWorkspace)) { setSelectedWorkspace(null); setSelectedSession(null) }
      showSuccess(`成功删除工作目录 "${name}"`)
    } catch (error) {
      showError('删除失败：' + error)
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

  // 分组列表 = 已知 PLATFORMS + 任何后端新增但前端未登记的来源（自动兜底成组，无需改代码）
  const allPlatforms = (() => {
    const known = new Set(PLATFORMS.flatMap(p => p.sources))
    const extra = Array.from(new Set(workspaces.map(sourceOf))).filter(s => !known.has(s))
    return [
      ...PLATFORMS,
      ...extra.map(s => ({ key: s, label: `${SOURCE_META[s]?.label ?? s} 对话历史`, sources: [s], color: 'text-foreground' })),
    ]
  })()

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

  const renderSourceBadge = (s?: string) => {
    const m = SOURCE_META[s ?? 'ide'] ?? SOURCE_META.ide
    return (
      <Badge variant="outline" className={`text-xs h-4 px-1.5 ${m.cls}`}>
        {m.label}
      </Badge>
    )
  }

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
          <div className="p-3 border-b border-border space-y-3 bg-gradient-to-b from-muted/20 to-transparent">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2 min-w-0">
                <span className="flex h-8 w-8 items-center justify-center rounded-xl bg-gradient-to-br from-primary/20 to-primary/5 text-primary shrink-0">
                  <Folder size={15} />
                </span>
                <div className="flex flex-col leading-tight min-w-0">
                  <h2 className="text-[13px] font-semibold text-foreground truncate">工作区与会话</h2>
                  <span className="text-[10px] text-muted-foreground">{workspaces.length} 个工作区</span>
                </div>
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <button
                  onClick={handleRefresh}
                  title="刷新列表"
                  className={`inline-flex items-center justify-center h-6 w-6 rounded-md transition-colors ${refreshOk ? 'text-green-500' : 'text-muted-foreground hover:text-foreground hover:bg-muted'}`}
                >
                  {refreshOk ? <Check className="h-3.5 w-3.5" /> : <RefreshCw className={`h-3.5 w-3.5 ${refreshing ? 'animate-spin' : ''}`} />}
                </button>
                {selectedWorkspaceHashes.size > 0 && (
                  <button
                    onClick={handleBatchDeleteWorkspaces}
                    className="inline-flex items-center gap-1 h-6 px-2 rounded-md text-[11px] font-medium bg-red-500/12 text-red-500 hover:bg-red-500/20 transition-colors"
                  >
                    <Trash2 className="h-3 w-3" />{selectedWorkspaceHashes.size}
                  </button>
                )}
              </div>
            </div>
            {/* 搜索 */}
            <div className="relative">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground pointer-events-none" />
              <input
                placeholder="搜索会话..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="w-full h-9 pl-9 pr-3 rounded-xl border border-input bg-background text-xs outline-none focus:border-primary focus:ring-2 focus:ring-primary/20 transition-all placeholder:text-muted-foreground"
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

              {/* 正常模式：分组(平台) → 工作目录 → 会话 */}
              {!searchQuery && allPlatforms.map(platform => {
                const platWorkspaces = workspaces.filter(w => platform.sources.includes(sourceOf(w)))
                if (platWorkspaces.length === 0) return null
                const groups = new Map<string, string[]>()
                platWorkspaces.forEach(w => {
                  const k = dirKeyOf(w)
                  if (!groups.has(k)) groups.set(k, [])
                  groups.get(k)!.push(w)
                })
                const dirs = Array.from(groups.entries())
                  .map(([key, hashes]) => ({ key: platform.key + '|' + key, hashes, name: decodeWorkspaceName(hashes[0]) }))
                  .sort((a, b) => a.name.localeCompare(b.name))
                const open = !collapsedGroups.has(platform.key)
                const allChecked = platWorkspaces.every(h => selectedWorkspaceHashes.has(h))
                return (
                  <div key={platform.key}>
                    {/* 分组头 + 全选框 */}
                    <div className={`w-full flex items-center gap-1.5 px-1.5 h-7 rounded-md transition-colors ${open ? 'bg-muted/50' : 'hover:bg-muted/60'}`}>
                      <button onClick={() => toggleGroup(platform.key)} className="flex items-center gap-1.5 flex-1 min-w-0 cursor-pointer">
                        <ChevronRight className={`h-3.5 w-3.5 shrink-0 transition-transform ${open ? `rotate-90 ${platform.color}` : 'text-muted-foreground/70'}`} />
                        <span className={`text-xs font-semibold truncate ${open ? platform.color : 'text-foreground'}`}>{platform.label}</span>
                      </button>
                      <Checkbox
                        checked={allChecked}
                        onCheckedChange={() => toggleSelectHashes(platWorkspaces)}
                        onClick={(e) => e.stopPropagation()}
                        className="shrink-0 cursor-pointer"
                      />
                      <span className="text-[10px] text-muted-foreground tabular-nums shrink-0">{dirs.length}</span>
                    </div>
                    {open && (
                      <div className="mt-0.5">
                        {dirs.map(({ key, hashes, name }) => {
                          const isExpanded = expandedWorkspaces.has(key)
                          const sessions = hashes.flatMap(h => getWorkspaceSessions(h))
                          const checked = hashes.every(h => selectedWorkspaceHashes.has(h))
                          return (
                            <div key={key}>
                              {/* 工作目录行 */}
                              <div
                                className={`group flex items-center gap-1 h-9 pl-2.5 pr-1.5 rounded-lg cursor-pointer transition-all ${
                                  checked ? 'bg-primary/[0.07] hover:bg-primary/10' : 'hover:bg-muted/60'
                                }`}
                                onClick={() => toggleDir(key)}
                                title={dirPathOf(hashes[0])}
                              >
                                <ChevronRight className={`h-3.5 w-3.5 text-muted-foreground/70 shrink-0 transition-transform ${isExpanded ? 'rotate-90' : ''}`} />
                                <span className={`flex h-6 w-6 items-center justify-center rounded-md shrink-0 transition-colors ${isExpanded ? 'bg-primary/12 text-primary' : 'text-muted-foreground'}`}>
                                  <Folder className="h-3.5 w-3.5" />
                                </span>
                                <span className="flex-1 min-w-0 truncate text-xs font-medium text-foreground">{name}</span>
                                {Array.from(new Set(hashes.map(sourceOf))).sort().map(s => (
                                  <span key={s} className="shrink-0">{renderSourceBadge(s)}</span>
                                ))}
                                {sessions.length > 0 && (
                                  <span className="inline-flex items-center h-5 px-1.5 rounded-full bg-muted text-[10px] font-medium text-muted-foreground tabular-nums shrink-0">{sessions.length}</span>
                                )}
                                <Checkbox
                                  checked={checked}
                                  onCheckedChange={() => setSelectedWorkspaceHashes(prev => {
                                    const n = new Set(prev)
                                    const all = hashes.every(h => n.has(h))
                                    hashes.forEach(h => all ? n.delete(h) : n.add(h))
                                    return n
                                  })}
                                  onClick={(e) => e.stopPropagation()}
                                  className="shrink-0 cursor-pointer"
                                />
                                <button
                                  className="h-6 w-6 shrink-0 rounded-md inline-flex items-center justify-center text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-destructive/12 hover:text-destructive transition-all"
                                  onClick={(e) => { e.stopPropagation(); handleDeleteDir(hashes, name, platform.label, platform.color) }}
                                  title="删除工作目录"
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
                                          onClick={() => handleSelectSession(session.workspaceHash, session)}
                                          title={cleanTitle(session.title)}
                                        >
                                          <span className={`flex-1 min-w-0 truncate text-xs ${active ? 'text-primary font-medium' : 'text-foreground/90'}`}>
                                            {cleanTitle(session.title)}
                                          </span>
                                          {renderSourceBadge(session.source)}
                                          <span className="flex items-center gap-0.5 text-[10px] text-muted-foreground tabular-nums ml-1.5 shrink-0 group-hover:opacity-0 transition-opacity">
                                            <MessageSquare className="h-2.5 w-2.5" />{session.messageCount}
                                          </span>
                                          <button
                                            className="absolute right-1.5 h-5 w-5 rounded inline-flex items-center justify-center text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-destructive hover:text-destructive-foreground transition-all"
                                            onClick={(e) => { e.stopPropagation(); handleDeleteSession(session.workspaceHash, session) }}
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
              <div className="px-4 py-3 border-b border-border bg-gradient-to-r from-muted/30 via-transparent to-transparent flex items-start justify-between gap-3">
                <div className="flex-1 min-w-0">
                  <div
                    className="group inline-flex items-start gap-1.5 cursor-pointer max-w-full"
                    title="点击复制会话文件路径"
                    onClick={async () => {
                      try {
                        const path = await sessionApi.getSessionFilePath(selectedWorkspaceHash, selectedSession.sessionId)
                        await navigator.clipboard.writeText(`"${path}"`)
                        showSuccess('已复制文件路径')
                      } catch (e) {
                        showError('复制失败：' + String(e))
                      }
                    }}
                  >
                    <h2 className="text-sm font-semibold text-foreground line-clamp-2 leading-snug transition-colors group-hover:text-primary">
                      {selectedSession.title}
                    </h2>
                    <Copy className="h-3 w-3 mt-0.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
                  </div>
                  <div className="flex items-center gap-1.5 mt-2 flex-wrap">
                    {renderSourceBadge(sourceOf(selectedWorkspaceHash))}
                    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] text-muted-foreground bg-muted/50 border border-border/60 min-w-0 max-w-[280px]">
                      <Folder className="h-3 w-3 shrink-0" />
                      <span className="truncate" title={selectedSession.workspaceDirectory}>
                        {selectedSession.workspaceDirectory.split(/[/\\]/).filter(Boolean).pop() || selectedSession.workspaceDirectory || '—'}
                      </span>
                    </span>
                    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] text-muted-foreground bg-muted/50 border border-border/60 shrink-0">
                      <MessageSquare className="h-3 w-3" />
                      {selectedSession.history.length}
                    </span>
                  </div>
                </div>
                <div className="flex gap-1.5 shrink-0">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs"
                    onClick={() => handleExportSession('json')}
                  >
                    <FileJson className="h-3.5 w-3.5 mr-1" />
                    JSON
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs"
                    onClick={() => handleExportSession('markdown')}
                  >
                    <FileText className="h-3.5 w-3.5 mr-1" />
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

                      const isUser = item.message.role === 'user'
                      return (
                        <div key={item.message.id} className="flex gap-2.5">
                          <div className={`w-8 h-8 rounded-xl flex items-center justify-center text-base shrink-0 shadow-sm ring-1 ${
                            isUser ? 'bg-gradient-to-br from-blue-500/20 to-indigo-500/10 ring-blue-500/20' : 'bg-gradient-to-br from-emerald-500/20 to-teal-500/10 ring-emerald-500/20'
                          }`}>
                            {isUser ? '👤' : '🤖'}
                          </div>
                          <div className={`flex-1 min-w-0 rounded-2xl border px-4 py-3 ${
                            isUser ? 'bg-blue-500/[0.05] border-blue-500/20' : 'bg-card border-border'
                          }`}>
                            <div className={`text-xs font-semibold mb-1.5 ${isUser ? 'text-blue-600 dark:text-blue-400' : 'text-emerald-600 dark:text-emerald-400'}`}>
                              {isUser ? 'User' : 'Assistant'}
                            </div>
                            {item.message.content.map((content, i) => (
                              <Markdown key={i} text={content.text} />
                            ))}
                          </div>
                        </div>
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

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { KeyboardEvent as ReactKeyboardEvent, PointerEvent as ReactPointerEvent } from 'react'
import {
  ArrowDown,
  ArrowUp,
  CalendarClock,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Download,
  FileJson,
  FileText,
  Folder,
  HardDrive,
  ListChecks,
  Loader2,
  MessageSquare,
  MoreHorizontal,
  RefreshCw,
  Search,
  Trash2,
  X,
} from 'lucide-react'
import { save } from '@tauri-apps/plugin-dialog'
import { writeTextFile } from '@tauri-apps/plugin-fs'
import { sessionApi } from '@/api/sessionApi'
import { Button } from '@/components/ui/actions/button'
import { Input } from '@/components/ui/forms/input'
import { Checkbox } from '@/components/ui/forms/checkbox'
import { Skeleton } from '@/components/ui/feedback/skeleton'
import { ScrollArea } from '@/components/ui/layout/scroll-area'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/overlays/dropdown-menu'
import { useDialog } from '@/contexts/DialogContext'
import { useTranslation } from '@/i18n'
import { cn } from '@/lib/utils'
import type { HistoryItem, IdeSession, SessionSummary } from '@/types/session'
import { showError, showSuccess, showWarning } from '@/utils/toast'
import {
  cleanTitle,
  ConversationSummaryCard,
  decodeWorkspaceName,
  dirKeyOf,
  dirPathOf,
  embeddedSummaryOf,
  formatFileSize,
  formatFullDate,
  formatRelativeTime,
  historyItemText,
  latestModifiedAt,
  PLATFORMS,
  SessionMessageCard,
  SOURCE_META,
  SourceBadge,
  sourceOf,
  type PlatformMeta,
} from './sessionUi'

const DEFAULT_SIDEBAR_WIDTH = 328
const MIN_SIDEBAR_WIDTH = 288
const MAX_SIDEBAR_WIDTH = 420
const SIDEBAR_WIDTH_KEY = 'kirohub-session-sidebar-width'

interface DirectoryGroup {
  key: string
  hashes: string[]
  name: string
  sessions: SessionSummary[]
  modifiedAt?: number
}

interface PlatformGroup {
  platform: PlatformMeta
  workspaces: string[]
  directories: DirectoryGroup[]
}

type PreviewRow =
  | { kind: 'summary'; key: string; text: string }
  | { kind: 'message'; key: string; item: HistoryItem; historyIndex: number }

function isReadOnlySessionSummary(session: SessionSummary): boolean {
  return session.sessionId.startsWith('@') || session.source === 'antigravity-backup'
}

function clampSidebarWidth(width: number): number {
  return Math.min(MAX_SIDEBAR_WIDTH, Math.max(MIN_SIDEBAR_WIDTH, width))
}

function DeleteMenu({
  label,
  onDelete,
  disabled,
  triggerClassName,
}: {
  label: string
  onDelete: () => void
  disabled?: boolean
  triggerClassName?: string
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          disabled={disabled}
          className={cn('shrink-0 text-muted-foreground', triggerClassName)}
          title="更多操作"
          aria-label={`更多操作：${label}`}
        >
          <MoreHorizontal />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-40">
        <DropdownMenuItem variant="destructive" onSelect={() => onDelete()}>
          <Trash2 />
          {label}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function SessionListRow({
  session,
  active,
  pending,
  workspaceName,
  showSource = true,
  disabled,
  deletable = true,
  onSelect,
  onDelete,
}: {
  session: SessionSummary
  active: boolean
  pending: boolean
  workspaceName?: string
  showSource?: boolean
  disabled?: boolean
  deletable?: boolean
  onSelect: () => void
  onDelete: () => void
}) {
  const title = cleanTitle(session.title)
  return (
    <div
      className={cn(
        'group/session relative flex min-w-0 items-center rounded-xl transition-colors',
        active ? 'bg-primary/[0.09] text-primary' : 'hover:bg-muted/55',
        pending && 'ring-1 ring-primary/25',
      )}
    >
      <button
        type="button"
        onClick={onSelect}
        disabled={disabled}
        aria-current={active ? 'true' : undefined}
        className={cn(
          'relative min-w-0 flex-1 px-3 py-2 text-left outline-none before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50',
          active && 'before:bg-primary',
        )}
        title={title}
      >
        <span className={cn('block truncate text-xs font-medium', active ? 'text-primary' : 'text-foreground/90')}>
          {title}
        </span>
        <span className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground">
          {workspaceName && <span className="max-w-28 truncate">{workspaceName}</span>}
          {workspaceName && <span aria-hidden="true">·</span>}
          <span className="shrink-0" title={formatFullDate(session.modifiedAt)}>{formatRelativeTime(session.modifiedAt)}</span>
          <span aria-hidden="true">·</span>
          <span className="shrink-0 tabular-nums">{session.messageCount} 条</span>
        </span>
      </button>
      <div className="mr-1.5 flex shrink-0 items-center gap-1">
        {pending ? (
          <Loader2 className="size-3.5 animate-spin text-primary" />
        ) : showSource ? (
          <SourceBadge source={session.source} compact />
        ) : null}
        {deletable && (
          <DeleteMenu
            label="删除会话"
            onDelete={onDelete}
            disabled={disabled}
            triggerClassName="opacity-0 transition-opacity group-hover/session:opacity-100 group-focus-within/session:opacity-100"
          />
        )}
      </div>
    </div>
  )
}

export default function SessionManagerView() {
  const { t } = useTranslation()
  const { showConfirm } = useDialog()
  const [workspaces, setWorkspaces] = useState<string[]>([])
  const [workspaceSessions, setWorkspaceSessions] = useState<Map<string, SessionSummary[]>>(new Map())
  const [expandedWorkspaces, setExpandedWorkspaces] = useState<Set<string>>(new Set())
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const [selectedSession, setSelectedSession] = useState<IdeSession | null>(null)
  const [selectedWorkspaceHash, setSelectedWorkspaceHash] = useState('')
  const [selectedWorkspaceHashes, setSelectedWorkspaceHashes] = useState<Set<string>>(new Set())
  const [searchQuery, setSearchQuery] = useState('')
  const [messageQuery, setMessageQuery] = useState('')
  const [initialLoading, setInitialLoading] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [refreshOk, setRefreshOk] = useState(false)
  const [sessionLoadingKey, setSessionLoadingKey] = useState<string | null>(null)
  const [mutating, setMutating] = useState(false)
  const [manageMode, setManageMode] = useState(false)
  const [sidebarWidth, setSidebarWidth] = useState(() => {
    if (typeof window === 'undefined') return DEFAULT_SIDEBAR_WIDTH
    const stored = Number(window.localStorage.getItem(SIDEBAR_WIDTH_KEY))
    return Number.isFinite(stored) && stored > 0 ? clampSidebarWidth(stored) : DEFAULT_SIDEBAR_WIDTH
  })
  const [resizingSidebar, setResizingSidebar] = useState(false)
  const [copiedMessageKey, setCopiedMessageKey] = useState<string | null>(null)

  const refreshOkTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const sessionLoadRequestRef = useRef(0)
  const resizeStartRef = useRef({ x: 0, width: DEFAULT_SIDEBAR_WIDTH })
  const messageScrollRef = useRef<HTMLDivElement | null>(null)

  const applySessionTree = useCallback((tree: Awaited<ReturnType<typeof sessionApi.listSessionTree>>) => {
    setWorkspaces(tree.workspaces)
    setWorkspaceSessions(new Map(Object.entries(tree.sessionsByWorkspace)))
    setSelectedWorkspaceHashes(previous => {
      const valid = new Set(tree.workspaces)
      return new Set(Array.from(previous).filter(hash => valid.has(hash)))
    })
  }, [])

  const reloadTree = useCallback(async () => {
    const tree = await sessionApi.listSessionTree()
    applySessionTree(tree)
    return tree
  }, [applySessionTree])

  const loadWorkspaces = useCallback(async () => {
    setInitialLoading(true)
    try {
      await reloadTree()
    } catch (error) {
      console.error('Failed to load workspaces:', error)
      showError(`加载工作区失败：${String(error)}`)
    } finally {
      setInitialLoading(false)
    }
  }, [reloadTree])

  useEffect(() => {
    void loadWorkspaces()
    return () => {
      if (refreshOkTimerRef.current) clearTimeout(refreshOkTimerRef.current)
      if (copiedTimerRef.current) clearTimeout(copiedTimerRef.current)
      sessionLoadRequestRef.current += 1
    }
  }, [loadWorkspaces])

  useEffect(() => {
    if (!resizingSidebar) return
    const handlePointerMove = (event: PointerEvent) => {
      setSidebarWidth(clampSidebarWidth(resizeStartRef.current.width + event.clientX - resizeStartRef.current.x))
    }
    const handlePointerUp = () => {
      setResizingSidebar(false)
      setSidebarWidth(width => {
        const next = clampSidebarWidth(width)
        window.localStorage.setItem(SIDEBAR_WIDTH_KEY, String(next))
        return next
      })
    }
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    window.addEventListener('pointermove', handlePointerMove)
    window.addEventListener('pointerup', handlePointerUp, { once: true })
    return () => {
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      window.removeEventListener('pointermove', handlePointerMove)
      window.removeEventListener('pointerup', handlePointerUp)
    }
  }, [resizingSidebar])

  const allPlatforms = useMemo<PlatformMeta[]>(() => {
    const knownSources = new Set(PLATFORMS.flatMap(platform => platform.sources))
    const extraSources = Array.from(new Set(workspaces.map(sourceOf))).filter(source => !knownSources.has(source))
    return [
      ...PLATFORMS,
      ...extraSources.map(source => ({
        key: source,
        label: SOURCE_META[source]?.label || source,
        sources: [source],
        dotClass: 'bg-slate-400',
      })),
    ]
  }, [workspaces])

  const platformGroups = useMemo<PlatformGroup[]>(() => {
    return allPlatforms.flatMap(platform => {
      const platformWorkspaces = workspaces.filter(workspace => platform.sources.includes(sourceOf(workspace)))
      if (platformWorkspaces.length === 0) return []
      const grouped = new Map<string, string[]>()
      for (const workspace of platformWorkspaces) {
        const directoryKey = dirKeyOf(workspace)
        const hashes = grouped.get(directoryKey) || []
        hashes.push(workspace)
        grouped.set(directoryKey, hashes)
      }
      const directories = Array.from(grouped.entries())
        .map(([directoryKey, hashes]) => {
          const sessions = hashes
            .flatMap(hash => workspaceSessions.get(hash) || [])
            .sort((left, right) => (right.modifiedAt || 0) - (left.modifiedAt || 0))
          return {
            key: `${platform.key}|${directoryKey}`,
            hashes,
            name: decodeWorkspaceName(hashes[0]),
            sessions,
            modifiedAt: latestModifiedAt(sessions),
          }
        })
        .sort((left, right) => left.name.localeCompare(right.name, 'zh-CN'))
      return [{ platform, workspaces: platformWorkspaces, directories }]
    })
  }, [allPlatforms, workspaces, workspaceSessions])

  const totalSessions = useMemo(
    () => Array.from(workspaceSessions.values()).reduce((total, sessions) => total + sessions.length, 0),
    [workspaceSessions],
  )
  const totalDirectories = useMemo(
    () => platformGroups.reduce((total, group) => total + group.directories.length, 0),
    [platformGroups],
  )
  const selectedDirectoryCount = useMemo(
    () => platformGroups.reduce(
      (total, group) => total + group.directories.filter(directory => directory.hashes.some(hash => selectedWorkspaceHashes.has(hash))).length,
      0,
    ),
    [platformGroups, selectedWorkspaceHashes],
  )
  const deletableWorkspaceHashes = useMemo(() => new Set(
    workspaces.filter(hash => (workspaceSessions.get(hash) || []).every(session => !isReadOnlySessionSummary(session))),
  ), [workspaces, workspaceSessions])

  const filteredSessions = useMemo(() => {
    const query = searchQuery.trim().toLocaleLowerCase('zh-CN')
    if (!query) return []
    return Array.from(workspaceSessions.values())
      .flat()
      .filter(session => {
        const sourceLabel = SOURCE_META[session.source || 'ide']?.label || session.source || ''
        return [session.title, session.workspaceDirectory, decodeWorkspaceName(session.workspaceHash), sourceLabel]
          .join('\n')
          .toLocaleLowerCase('zh-CN')
          .includes(query)
      })
      .sort((left, right) => (right.modifiedAt || 0) - (left.modifiedAt || 0))
  }, [searchQuery, workspaceSessions])

  const selectedSummary = useMemo(() => {
    if (!selectedSession || !selectedWorkspaceHash) return undefined
    return (workspaceSessions.get(selectedWorkspaceHash) || [])
      .find(summary => summary.sessionId === selectedSession.sessionId)
  }, [selectedSession, selectedWorkspaceHash, workspaceSessions])

  const embeddedSummary = useMemo(
    () => selectedSession ? embeddedSummaryOf(selectedSession) : null,
    [selectedSession],
  )
  const summaryText = selectedSession?.conversationSummary?.trim() || embeddedSummary?.text || ''
  const previewRows = useMemo<PreviewRow[]>(() => {
    if (!selectedSession) return []
    const query = messageQuery.trim().toLocaleLowerCase('zh-CN')
    const rows: PreviewRow[] = []
    if (summaryText && (!query || summaryText.toLocaleLowerCase('zh-CN').includes(query))) {
      rows.push({ kind: 'summary', key: `summary:${selectedSession.sessionId}`, text: summaryText })
    }
    selectedSession.history.forEach((item, historyIndex) => {
      if (historyIndex === embeddedSummary?.historyIndex) return
      const roleText = item.message.role === 'user'
        ? '用户 user'
        : item.message.role === 'assistant'
          ? '助手 assistant'
          : item.message.role === 'artifact'
            ? '产物 artifact'
            : '系统 system'
      const searchable = `${roleText}\n${historyItemText(item)}`.toLocaleLowerCase('zh-CN')
      if (query && !searchable.includes(query)) return
      rows.push({
        kind: 'message',
        key: `message:${item.message.id || historyIndex}:${historyIndex}`,
        item,
        historyIndex,
      })
    })
    return rows
  }, [embeddedSummary?.historyIndex, messageQuery, selectedSession, summaryText])

  const visibleMessageCount = useMemo(
    () => previewRows.filter(row => row.kind === 'message').length,
    [previewRows],
  )
  const availableMessageCount = useMemo(() => {
    if (!selectedSession) return 0
    return selectedSession.history.length - (embeddedSummary ? 1 : 0)
  }, [embeddedSummary, selectedSession])

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      messageScrollRef.current?.scrollTo({ top: 0 })
    })
    return () => window.cancelAnimationFrame(frame)
  }, [messageQuery, selectedSession?.sessionId])

  const toggleGroup = (key: string) => {
    setCollapsedGroups(previous => {
      const next = new Set(previous)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      return next
    })
  }

  const toggleDirectory = (key: string) => {
    setExpandedWorkspaces(previous => {
      const next = new Set(previous)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      return next
    })
  }

  const toggleSelectHashes = (hashes: string[]) => {
    const selectable = hashes.filter(hash => deletableWorkspaceHashes.has(hash))
    setSelectedWorkspaceHashes(previous => {
      const next = new Set(previous)
      const allSelected = selectable.length > 0 && selectable.every(hash => next.has(hash))
      for (const hash of selectable) {
        if (allSelected) next.delete(hash)
        else next.add(hash)
      }
      return next
    })
  }

  const handleRefresh = async () => {
    if (refreshing) return
    setRefreshing(true)
    try {
      await sessionApi.refreshSessionCache()
      await reloadTree()
      if (refreshOkTimerRef.current) clearTimeout(refreshOkTimerRef.current)
      setRefreshOk(true)
      refreshOkTimerRef.current = setTimeout(() => {
        setRefreshOk(false)
        refreshOkTimerRef.current = null
      }, 1500)
    } catch (error) {
      showError(`刷新失败：${String(error)}`)
    } finally {
      setRefreshing(false)
    }
  }

  const handleSelectSession = async (workspaceHash: string, session: SessionSummary) => {
    const key = `${workspaceHash}|${session.sessionId}`
    if (`${selectedWorkspaceHash}|${selectedSession?.sessionId || ''}` === key && !sessionLoadingKey) return
    const requestId = ++sessionLoadRequestRef.current
    setSessionLoadingKey(key)
    setMessageQuery('')
    try {
      const data = await sessionApi.loadSession(workspaceHash, session.sessionId)
      if (requestId !== sessionLoadRequestRef.current) return
      setSelectedSession(data)
      setSelectedWorkspaceHash(workspaceHash)
    } catch (error) {
      if (requestId !== sessionLoadRequestRef.current) return
      console.error('Failed to load session:', error)
      showError(`加载失败：${String(error)}`)
    } finally {
      if (requestId === sessionLoadRequestRef.current) setSessionLoadingKey(null)
    }
  }

  const handleDeleteDirectory = async (directory: DirectoryGroup, platformLabel: string) => {
    const confirmed = await showConfirm(
      '删除工作目录',
      <>确定要删除 <span className="font-semibold text-foreground">{platformLabel}</span> 工作区“{directory.name}”及其全部会话吗？{'\n\n'}此操作不可恢复！</>,
    )
    if (!confirmed) return
    setMutating(true)
    try {
      for (const hash of directory.hashes) await sessionApi.deleteWorkspace(hash)
      await reloadTree()
      setExpandedWorkspaces(previous => {
        const next = new Set(previous)
        next.delete(directory.key)
        return next
      })
      setSelectedWorkspaceHashes(previous => {
        const next = new Set(previous)
        directory.hashes.forEach(hash => next.delete(hash))
        return next
      })
      if (directory.hashes.includes(selectedWorkspaceHash)) {
        sessionLoadRequestRef.current += 1
        setSelectedSession(null)
        setSelectedWorkspaceHash('')
        setSessionLoadingKey(null)
      }
      showSuccess(`已删除工作目录“${directory.name}”`)
    } catch (error) {
      try { await reloadTree() } catch { /* 保留原始删除错误 */ }
      showError(`删除失败：${String(error)}`)
    } finally {
      setMutating(false)
    }
  }

  const handleDeleteSession = async (session: SessionSummary) => {
    const confirmed = await showConfirm('删除会话', `确定要删除会话“${cleanTitle(session.title)}”吗？`)
    if (!confirmed) return
    setMutating(true)
    try {
      await sessionApi.deleteSession(session.workspaceHash, session.sessionId)
      await reloadTree()
      if (selectedWorkspaceHash === session.workspaceHash && selectedSession?.sessionId === session.sessionId) {
        sessionLoadRequestRef.current += 1
        setSelectedSession(null)
        setSelectedWorkspaceHash('')
        setSessionLoadingKey(null)
      }
      showSuccess('会话已删除')
    } catch (error) {
      try { await reloadTree() } catch { /* 保留原始删除错误 */ }
      showError(`删除失败：${String(error)}`)
    } finally {
      setMutating(false)
    }
  }

  const handleBatchDeleteWorkspaces = async () => {
    if (selectedWorkspaceHashes.size === 0) {
      showWarning('请先选择要删除的工作区')
      return
    }
    const names = Array.from(new Set(Array.from(selectedWorkspaceHashes).map(decodeWorkspaceName))).join('、')
    const confirmed = await showConfirm(
      '批量删除工作区',
      `确定要删除选中的 ${selectedDirectoryCount} 个工作目录及其全部会话吗？\n\n工作目录：${names}\n\n此操作不可恢复！`,
    )
    if (!confirmed) return
    const hashes = Array.from(selectedWorkspaceHashes)
    setMutating(true)
    try {
      for (const hash of hashes) await sessionApi.deleteWorkspace(hash)
      await reloadTree()
      if (hashes.includes(selectedWorkspaceHash)) {
        sessionLoadRequestRef.current += 1
        setSelectedSession(null)
        setSelectedWorkspaceHash('')
        setSessionLoadingKey(null)
      }
      setSelectedWorkspaceHashes(new Set())
      setManageMode(false)
      showSuccess(`已删除 ${selectedDirectoryCount} 个工作目录`)
    } catch (error) {
      try { await reloadTree() } catch { /* 保留原始批量删除错误 */ }
      showError(`批量删除失败：${String(error)}`)
    } finally {
      setMutating(false)
    }
  }

  const handleExportSession = async (format: 'json' | 'markdown') => {
    if (!selectedSession || !selectedWorkspaceHash) return
    try {
      const content = await sessionApi.exportSession(selectedWorkspaceHash, selectedSession.sessionId, format)
      const extension = format === 'json' ? 'json' : 'md'
      const safeTitle = cleanTitle(selectedSession.title).replace(/[<>:"/\\|?*]/g, '_')
      const filePath = await save({
        defaultPath: `${safeTitle}.${extension}`,
        filters: [{ name: format === 'json' ? 'JSON' : 'Markdown', extensions: [extension] }],
      })
      if (!filePath) return
      await writeTextFile(filePath, content)
      showSuccess('导出成功')
    } catch (error) {
      console.error('Failed to export session:', error)
      showError(`导出失败：${String(error)}`)
    }
  }

  const handleCopySessionPath = async () => {
    if (!selectedSession || !selectedWorkspaceHash) return
    try {
      const path = await sessionApi.getSessionFilePath(selectedWorkspaceHash, selectedSession.sessionId)
      await navigator.clipboard.writeText(`"${path}"`)
      showSuccess('会话文件路径已复制')
    } catch (error) {
      showError(`复制失败：${String(error)}`)
    }
  }

  const handleCopyMessage = async (item: HistoryItem, key: string) => {
    try {
      await navigator.clipboard.writeText(historyItemText(item))
      if (copiedTimerRef.current) clearTimeout(copiedTimerRef.current)
      setCopiedMessageKey(key)
      copiedTimerRef.current = setTimeout(() => {
        setCopiedMessageKey(null)
        copiedTimerRef.current = null
      }, 1500)
    } catch (error) {
      showError(`复制失败：${String(error)}`)
    }
  }

  const handleManageMode = () => {
    setManageMode(previous => {
      if (previous) setSelectedWorkspaceHashes(new Set())
      return !previous
    })
  }

  const handleSidebarResizeStart = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    resizeStartRef.current = { x: event.clientX, width: sidebarWidth }
    setResizingSidebar(true)
  }

  const handleSidebarResizeKey = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
    event.preventDefault()
    const step = event.shiftKey ? 24 : 8
    const direction = event.key === 'ArrowLeft' ? -1 : 1
    setSidebarWidth(width => {
      const next = clampSidebarWidth(width + direction * step)
      window.localStorage.setItem(SIDEBAR_WIDTH_KEY, String(next))
      return next
    })
  }

  const selectedKey = selectedSession ? `${selectedWorkspaceHash}|${selectedSession.sessionId}` : ''
  const selectedSource = selectedSummary?.source || sourceOf(selectedWorkspaceHash)
  const selectedWorkspaceName = selectedSession
    ? selectedSession.workspaceDirectory.split(/[/\\]/).filter(Boolean).pop() || selectedSession.workspaceDirectory || '未知工作区'
    : ''
  const showScrollControls = previewRows.length > 8

  return (
    <div className="flex h-full flex-col glass-main">
      <header className="flex shrink-0 items-center gap-3 border-b border-border px-5 py-3">
        <div className="flex size-9 items-center justify-center rounded-xl bg-primary/10 text-primary ring-1 ring-primary/15">
          <MessageSquare className="size-4.5" />
        </div>
        <div className="min-w-0">
          <h1 className="text-base font-semibold leading-tight text-foreground">会话管理</h1>
          <p className="truncate text-xs text-muted-foreground">{t('session.subtitle')}</p>
        </div>
        <div className="ml-auto flex items-center gap-2 text-[11px] text-muted-foreground">
          <span className="hidden rounded-full border border-border/70 bg-muted/35 px-2.5 py-1 tabular-nums sm:inline-flex">
            {totalDirectories} 个工作目录 · {totalSessions} 个会话
          </span>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 overflow-hidden">
        <aside
          className="relative flex shrink-0 flex-col border-r border-border bg-background/25"
          style={{ width: sidebarWidth }}
        >
            <div className="shrink-0 space-y-3 border-b border-border bg-gradient-to-b from-muted/20 to-transparent p-3">
              <div className="flex items-center gap-2">
                <div className="min-w-0 flex-1">
                  <h2 className="text-[13px] font-semibold text-foreground">工作区与会话</h2>
                  <p className="mt-0.5 text-[10px] text-muted-foreground">按工具和工作目录整理</p>
                </div>
                <Button
                  variant={manageMode ? 'secondary' : 'ghost'}
                  size="sm"
                  onClick={handleManageMode}
                  disabled={mutating}
                  className="h-7 px-2 text-[11px]"
                  title={manageMode ? '退出批量管理' : '批量管理工作区'}
                >
                  {manageMode ? <X /> : <ListChecks />}
                  {manageMode ? '完成' : '管理'}
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onClick={handleRefresh}
                  disabled={refreshing || mutating}
                  className={refreshOk ? 'text-emerald-500' : 'text-muted-foreground'}
                  title="刷新列表"
                >
                  {refreshOk ? <Check /> : <RefreshCw className={refreshing ? 'animate-spin' : ''} />}
                </Button>
              </div>

              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                <Input
                  value={searchQuery}
                  onChange={event => setSearchQuery(event.target.value)}
                  placeholder="搜索标题、工作区或来源"
                  className="h-9 rounded-xl bg-background/70 pl-9 pr-8 text-xs"
                />
                {searchQuery && (
                  <button
                    type="button"
                    onClick={() => setSearchQuery('')}
                    className="absolute right-2 top-1/2 flex size-5 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
                    aria-label="清空搜索"
                  >
                    <X className="size-3" />
                  </button>
                )}
              </div>

              {manageMode && (
                <div className="flex items-center gap-2 rounded-xl border border-destructive/15 bg-destructive/[0.045] px-3 py-2">
                  <ListChecks className="size-3.5 shrink-0 text-destructive" />
                  <span className="min-w-0 flex-1 text-[11px] text-muted-foreground">
                    {selectedDirectoryCount > 0 ? `已选择 ${selectedDirectoryCount} 个工作目录` : '选择需要批量删除的工作目录'}
                  </span>
                  {selectedDirectoryCount > 0 && (
                    <Button
                      variant="destructive"
                      size="xs"
                      onClick={handleBatchDeleteWorkspaces}
                      disabled={mutating}
                    >
                      {mutating ? <Loader2 className="animate-spin" /> : <Trash2 />}
                      删除
                    </Button>
                  )}
                </div>
              )}
            </div>

            <ScrollArea className="min-h-0 flex-1">
              <div className="space-y-1.5 p-2.5">
                {initialLoading ? (
                  Array.from({ length: 7 }).map((_, index) => (
                    <div key={index} className="space-y-2 rounded-xl px-2 py-2.5">
                      <Skeleton className="h-3.5 w-2/3" />
                      <Skeleton className="h-2.5 w-1/2" />
                    </div>
                  ))
                ) : searchQuery.trim() ? (
                  <div className="space-y-1">
                    <div className="flex items-center justify-between px-2 pb-1 pt-0.5 text-[10px] text-muted-foreground">
                      <span>搜索结果</span>
                      <span className="tabular-nums">{filteredSessions.length}</span>
                    </div>
                    {filteredSessions.length === 0 ? (
                      <div className="flex flex-col items-center justify-center px-4 py-12 text-center text-muted-foreground">
                        <Search className="mb-3 size-8 opacity-30" />
                        <p className="text-xs font-medium">没有找到匹配会话</p>
                        <p className="mt-1 text-[10px]">可以尝试工作区名称或来源</p>
                      </div>
                    ) : filteredSessions.map(session => {
                      const rowKey = `${session.workspaceHash}|${session.sessionId}`
                      return (
                        <SessionListRow
                          key={rowKey}
                          session={session}
                          active={selectedKey === rowKey}
                          pending={sessionLoadingKey === rowKey}
                          workspaceName={decodeWorkspaceName(session.workspaceHash)}
                          disabled={mutating}
                          deletable={!isReadOnlySessionSummary(session)}
                          onSelect={() => handleSelectSession(session.workspaceHash, session)}
                          onDelete={() => handleDeleteSession(session)}
                        />
                      )
                    })}
                  </div>
                ) : platformGroups.length === 0 ? (
                  <div className="flex flex-col items-center justify-center px-4 py-12 text-center text-muted-foreground">
                    <Folder className="mb-3 size-9 opacity-30" />
                    <p className="text-xs font-medium">暂无历史会话</p>
                    <p className="mt-1 text-[10px]">点击刷新重新扫描本地记录</p>
                  </div>
                ) : platformGroups.map(({ platform, workspaces: platformWorkspaces, directories }) => {
                  const open = !collapsedGroups.has(platform.key)
                  const selectableWorkspaces = platformWorkspaces.filter(hash => deletableWorkspaceHashes.has(hash))
                  const selectedCount = selectableWorkspaces.filter(hash => selectedWorkspaceHashes.has(hash)).length
                  const allChecked = selectableWorkspaces.length > 0 && selectedCount === selectableWorkspaces.length
                  const groupChecked = allChecked ? true : selectedCount > 0 ? 'indeterminate' : false
                  return (
                    <section key={platform.key}>
                      <div className={cn('flex h-9 items-center gap-2 rounded-xl px-2 transition-colors', open ? 'bg-muted/45' : 'hover:bg-muted/45')}>
                        {manageMode && (
                          <Checkbox
                            checked={groupChecked}
                            onCheckedChange={() => toggleSelectHashes(selectableWorkspaces)}
                            disabled={mutating || selectableWorkspaces.length === 0}
                            aria-label={`选择全部 ${platform.label} 工作区`}
                          />
                        )}
                        <button
                          type="button"
                          onClick={() => toggleGroup(platform.key)}
                          className="flex min-w-0 flex-1 items-center gap-2 rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                          aria-expanded={open}
                        >
                          <ChevronRight className={cn('size-3.5 shrink-0 text-muted-foreground transition-transform', open && 'rotate-90')} />
                          <span className={cn('size-2 shrink-0 rounded-full shadow-sm', platform.dotClass)} />
                          <span className="truncate text-xs font-semibold text-foreground">{platform.label}</span>
                        </button>
                        <span className="rounded-full bg-background/70 px-1.5 py-0.5 text-[10px] tabular-nums text-muted-foreground">
                          {directories.length}
                        </span>
                      </div>

                      {open && (
                        <div className="mt-1 space-y-1">
                          {directories.map(directory => {
                            const expanded = expandedWorkspaces.has(directory.key)
                            const selectedHashes = directory.hashes.filter(hash => selectedWorkspaceHashes.has(hash)).length
                            const checked = selectedHashes === directory.hashes.length
                            const checkboxState = checked ? true : selectedHashes > 0 ? 'indeterminate' : false
                            const directorySources = Array.from(new Set(directory.hashes.map(sourceOf))).sort()
                            const directoryReadOnly = directory.sessions.some(isReadOnlySessionSummary)
                            return (
                              <div key={directory.key}>
                                <div
                                  className={cn(
                                    'group/directory flex min-w-0 items-center rounded-xl transition-colors',
                                    checked && manageMode ? 'bg-destructive/[0.055]' : 'hover:bg-muted/45',
                                  )}
                                >
                                  {manageMode && (
                                    <div className="pl-2.5">
                                      <Checkbox
                                        checked={checkboxState}
                                        onCheckedChange={() => toggleSelectHashes(directory.hashes)}
                                        disabled={mutating || directoryReadOnly}
                                        aria-label={`选择工作目录 ${directory.name}`}
                                      />
                                    </div>
                                  )}
                                  <button
                                    type="button"
                                    onClick={() => toggleDirectory(directory.key)}
                                    className="flex min-w-0 flex-1 items-center gap-2 px-2.5 py-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50"
                                    title={dirPathOf(directory.hashes[0])}
                                    aria-expanded={expanded}
                                  >
                                    <ChevronRight className={cn('size-3.5 shrink-0 text-muted-foreground/70 transition-transform', expanded && 'rotate-90')} />
                                    <span className={cn('flex size-7 shrink-0 items-center justify-center rounded-lg', expanded ? 'bg-primary/10 text-primary' : 'bg-muted/45 text-muted-foreground')}>
                                      <Folder className="size-3.5" />
                                    </span>
                                    <span className="min-w-0 flex-1">
                                      <span className="block truncate text-xs font-medium text-foreground">{directory.name}</span>
                                      <span className="mt-0.5 block truncate text-[10px] text-muted-foreground">
                                        {directory.sessions.length} 个会话 · {formatRelativeTime(directory.modifiedAt)}
                                      </span>
                                    </span>
                                  </button>
                                  {directorySources.length > 1 && (
                                    <div className="hidden shrink-0 items-center gap-1 2xl:flex">
                                      {directorySources.map(source => <SourceBadge key={source} source={source} compact />)}
                                    </div>
                                  )}
                                  {!directoryReadOnly && (
                                    <DeleteMenu
                                      label="删除工作目录"
                                      onDelete={() => handleDeleteDirectory(directory, platform.label)}
                                      disabled={mutating}
                                      triggerClassName="mr-1.5 opacity-0 transition-opacity group-hover/directory:opacity-100 group-focus-within/directory:opacity-100"
                                    />
                                  )}
                                </div>

                                {expanded && (
                                  <div className="relative ml-5 mt-0.5 space-y-0.5 border-l border-border/70 pl-2">
                                    {directory.sessions.length === 0 ? (
                                      <div className="px-3 py-3 text-[10px] text-muted-foreground">暂无会话</div>
                                    ) : directory.sessions.map(session => {
                                      const rowKey = `${session.workspaceHash}|${session.sessionId}`
                                      return (
                                        <SessionListRow
                                          key={rowKey}
                                          session={session}
                                          active={selectedKey === rowKey}
                                          pending={sessionLoadingKey === rowKey}
                                          showSource={directorySources.length > 1}
                                          disabled={mutating}
                                          deletable={!isReadOnlySessionSummary(session)}
                                          onSelect={() => handleSelectSession(session.workspaceHash, session)}
                                          onDelete={() => handleDeleteSession(session)}
                                        />
                                      )
                                    })}
                                  </div>
                                )}
                              </div>
                            )
                          })}
                        </div>
                      )}
                    </section>
                  )
                })}
              </div>
            </ScrollArea>

            <div
              role="separator"
              aria-label="调整会话列表宽度"
              aria-orientation="vertical"
              aria-valuemin={MIN_SIDEBAR_WIDTH}
              aria-valuemax={MAX_SIDEBAR_WIDTH}
              aria-valuenow={Math.round(sidebarWidth)}
              tabIndex={0}
              onPointerDown={handleSidebarResizeStart}
              onKeyDown={handleSidebarResizeKey}
              className={cn(
                'absolute inset-y-0 -right-1 z-20 w-2 cursor-col-resize outline-none after:absolute after:inset-y-0 after:left-1/2 after:w-px after:-translate-x-1/2 after:bg-transparent after:transition-colors hover:after:bg-primary/45 focus-visible:after:bg-primary',
                resizingSidebar && 'after:bg-primary',
              )}
            />
        </aside>

        <main className="relative flex min-w-0 flex-1 flex-col">
          {selectedSession ? (
            <>
              <div className="shrink-0 border-b border-border bg-gradient-to-r from-muted/25 via-transparent to-transparent px-5 py-3.5">
                <div className="min-w-0">
                  <div className="min-w-0">
                    <h2 className="line-clamp-2 break-words text-sm font-semibold leading-snug text-foreground [overflow-wrap:anywhere]" title={cleanTitle(selectedSession.title)}>
                      {cleanTitle(selectedSession.title)}
                    </h2>
                    <div className="mt-2 flex flex-wrap items-center gap-1.5">
                      <SourceBadge source={selectedSource} />
                      <span className="inline-flex h-6 max-w-64 items-center gap-1.5 rounded-lg border border-border/70 bg-muted/35 px-2 text-[10px] text-muted-foreground" title={selectedSession.workspaceDirectory}>
                        <Folder className="size-3 shrink-0" />
                        <span className="truncate">{selectedWorkspaceName}</span>
                      </span>
                      <span className="inline-flex h-6 items-center gap-1.5 rounded-lg border border-border/70 bg-muted/35 px-2 text-[10px] text-muted-foreground" title={formatFullDate(selectedSummary?.modifiedAt)}>
                        <CalendarClock className="size-3" />
                        {formatRelativeTime(selectedSummary?.modifiedAt)}
                      </span>
                      <span className="inline-flex h-6 items-center gap-1.5 rounded-lg border border-border/70 bg-muted/35 px-2 text-[10px] text-muted-foreground">
                        <MessageSquare className="size-3" />
                        {selectedSession.history.length} 条
                      </span>
                      {selectedSummary && (
                        <span className="inline-flex h-6 items-center gap-1.5 rounded-lg border border-border/70 bg-muted/35 px-2 text-[10px] text-muted-foreground">
                          <HardDrive className="size-3" />
                          {formatFileSize(selectedSummary.fileSize)}
                        </span>
                      )}
                    </div>
                  </div>

                </div>

                <div className="mt-3 flex flex-wrap items-center gap-2">
                  <div className="relative w-full max-w-sm">
                    <Search className="pointer-events-none absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                    <Input
                      value={messageQuery}
                      onChange={event => setMessageQuery(event.target.value)}
                      placeholder="在当前会话中搜索"
                      className="h-8 rounded-lg bg-background/70 pl-9 pr-8 text-xs"
                      aria-label="在当前会话中搜索"
                    />
                    {messageQuery && (
                      <button
                        type="button"
                        onClick={() => setMessageQuery('')}
                        className="absolute right-2 top-1/2 flex size-5 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
                        aria-label="清空会话搜索"
                      >
                        <X className="size-3" />
                      </button>
                    )}
                  </div>
                  {messageQuery && (
                    <span className="text-[10px] tabular-nums text-muted-foreground">
                      {visibleMessageCount} / {availableMessageCount} 条消息
                    </span>
                  )}
                  <div className="ml-auto flex shrink-0 items-center gap-1.5">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handleCopySessionPath}
                      disabled={selectedSummary?.sessionId.startsWith('@')}
                      title={selectedSummary?.sessionId.startsWith('@') ? '该索引会话没有正文文件' : '复制会话文件路径'}
                    >
                      <Copy />
                      路径
                    </Button>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="outline" size="sm">
                          <Download />
                          导出
                          <ChevronDown className="ml-0.5 size-3" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="w-44">
                        <DropdownMenuItem onSelect={() => handleExportSession('markdown')}>
                          <FileText />
                          导出 Markdown
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem onSelect={() => handleExportSession('json')}>
                          <FileJson />
                          导出 JSON
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                </div>
              </div>

              <div ref={messageScrollRef} className="min-h-0 flex-1 overflow-y-auto">
                {previewRows.length === 0 ? (
                  <div className="flex min-h-full items-center justify-center px-6 py-16">
                    <div className="text-center text-muted-foreground">
                      <Search className="mx-auto mb-3 size-9 opacity-30" />
                      <p className="text-sm font-medium">{messageQuery ? '没有匹配的消息' : '此会话没有可显示的消息'}</p>
                      {messageQuery && <p className="mt-1 text-xs">换一个关键词试试</p>}
                    </div>
                  </div>
                ) : (
                  <div className="mx-auto w-full max-w-5xl space-y-4 px-5 py-5 md:px-7">
                    {previewRows.map(row => (
                      <div key={row.key} className="[content-visibility:auto] [contain-intrinsic-size:auto_180px]">
                        {row.kind === 'summary' ? (
                          <ConversationSummaryCard text={row.text} />
                        ) : (
                          <SessionMessageCard
                            item={row.item}
                            index={row.historyIndex}
                            copied={copiedMessageKey === row.key}
                            onCopy={item => handleCopyMessage(item, row.key)}
                          />
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {showScrollControls && (
                <div className="absolute bottom-4 right-4 z-10 flex flex-col gap-1 rounded-xl border border-border/70 bg-background/85 p-1 shadow-lg backdrop-blur-md">
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={() => messageScrollRef.current?.scrollTo({ top: 0, behavior: 'smooth' })}
                    title="回到顶部"
                  >
                    <ArrowUp />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={() => {
                      const element = messageScrollRef.current
                      if (element) element.scrollTo({ top: element.scrollHeight, behavior: 'smooth' })
                    }}
                    title="前往底部"
                  >
                    <ArrowDown />
                  </Button>
                </div>
              )}
            </>
          ) : sessionLoadingKey ? (
            <div className="flex flex-1 flex-col px-8 py-6">
              <div className="space-y-3 border-b border-border pb-5">
                <Skeleton className="h-5 w-2/5" />
                <Skeleton className="h-5 w-3/5" />
              </div>
              <div className="mx-auto mt-6 w-full max-w-4xl space-y-4">
                {Array.from({ length: 4 }).map((_, index) => (
                  <div key={index} className="flex gap-3">
                    <Skeleton className="size-8 shrink-0 rounded-xl" />
                    <Skeleton className={cn('h-28 flex-1 rounded-2xl', index % 2 === 0 ? 'max-w-2xl' : '')} />
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div className="flex flex-1 items-center justify-center px-6 py-12">
              <div className="max-w-sm text-center">
                <div className="mx-auto mb-4 flex size-14 items-center justify-center rounded-2xl border border-primary/15 bg-primary/[0.06] text-primary shadow-sm">
                  <MessageSquare className="size-6" />
                </div>
                <h2 className="text-sm font-semibold text-foreground">选择一个会话开始预览</h2>
                <p className="mt-2 text-xs leading-5 text-muted-foreground">
                  从左侧展开工作目录，查看完整对话、上下文摘要以及导出选项。
                </p>
              </div>
            </div>
          )}

          {sessionLoadingKey && selectedSession && (
            <div className="absolute inset-0 z-20 flex items-center justify-center bg-background/55 backdrop-blur-[1px]">
              <div className="flex items-center gap-2 rounded-xl border border-border bg-background/90 px-4 py-2.5 text-xs font-medium text-foreground shadow-lg">
                <Loader2 className="size-4 animate-spin text-primary" />
                正在加载会话
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  )
}

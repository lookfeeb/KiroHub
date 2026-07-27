import { useEffect, useState } from 'react'
import {
  Bot,
  Check,
  Copy,
  FileCode2,
  Settings2,
  Sparkles,
  UserRound,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { Badge } from '@/components/ui/data-display/badge'
import Markdown from '@/components/shared/Markdown'
import type { HistoryItem, IdeSession, SessionSummary } from '@/types/session'

export interface PlatformMeta {
  key: string
  label: string
  sources: string[]
  dotClass: string
}

export const PLATFORMS: PlatformMeta[] = [
  { key: 'kiro', label: 'Kiro', sources: ['ide', 'cli'], dotClass: 'bg-blue-500' },
  { key: 'codex', label: 'Codex', sources: ['codex'], dotClass: 'bg-emerald-500' },
  { key: 'claude', label: 'Claude', sources: ['claude'], dotClass: 'bg-orange-500' },
  { key: 'antigravity', label: 'Antigravity', sources: ['antigravity', 'antigravity-ide'], dotClass: 'bg-cyan-500' },
]

export const SOURCE_META: Record<string, { label: string; cls: string }> = {
  cli: { label: 'CLI', cls: 'border-purple-500/30 bg-purple-500/[0.06] text-purple-600 dark:text-purple-400' },
  ide: { label: 'IDE', cls: 'border-blue-500/30 bg-blue-500/[0.06] text-blue-600 dark:text-blue-400' },
  codex: { label: 'Codex', cls: 'border-emerald-500/30 bg-emerald-500/[0.06] text-emerald-600 dark:text-emerald-400' },
  claude: { label: 'Claude', cls: 'border-orange-500/30 bg-orange-500/[0.06] text-orange-600 dark:text-orange-400' },
  antigravity: { label: 'Gemini', cls: 'border-cyan-500/30 bg-cyan-500/[0.06] text-cyan-600 dark:text-cyan-400' },
  'antigravity-ide': { label: 'AG IDE', cls: 'border-sky-500/30 bg-sky-500/[0.06] text-sky-600 dark:text-sky-400' },
}

const PREFIXES = Object.keys(SOURCE_META)
  .filter(key => key !== 'ide')
  .map(key => `${key}:`)

export function sourceOf(hash: string): string {
  const prefix = PREFIXES.find(item => hash.startsWith(item))
  return prefix ? prefix.slice(0, -1) : 'ide'
}

export function dirPathOf(hash: string): string {
  const prefix = PREFIXES.find(item => hash.startsWith(item))
  if (prefix) return hash.slice(prefix.length)
  try {
    return atob(hash.replace(/_+$/, ''))
  } catch {
    return hash
  }
}

export function dirKeyOf(hash: string): string {
  return dirPathOf(hash).replace(/[\\/]+$/, '').replace(/\\/g, '/').toLowerCase()
}

export function decodeWorkspaceName(hash: string): string {
  const prefix = PREFIXES.find(item => hash.startsWith(item))
  if (prefix) {
    const cwd = hash.slice(prefix.length)
    const parts = cwd.split(/[/\\]/).filter(Boolean)
    return parts.at(-1) || cwd || SOURCE_META[prefix.slice(0, -1)]?.label || prefix.slice(0, -1)
  }
  try {
    const decoded = atob(hash.replace(/_+$/, ''))
    const parts = decoded.split(/[/\\]/).filter(Boolean)
    return parts.at(-1) || decoded
  } catch {
    return hash
  }
}

export function cleanTitle(raw?: string): string {
  if (!raw) return '未命名会话'
  const text = raw
    .replace(/<\/?[a-zA-Z][^>]*>?/g, ' ')
    .replace(/&[a-zA-Z#0-9]+;/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
  if (!text) return '未命名会话'
  return text.length > 90 ? `${text.slice(0, 90)}…` : text
}

function timestampToDate(timestamp?: number): Date | null {
  if (!timestamp) return null
  const value = timestamp > 10_000_000_000 ? timestamp : timestamp * 1000
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

export function formatRelativeTime(timestamp?: number): string {
  const date = timestampToDate(timestamp)
  if (!date) return '时间未知'
  const now = new Date()
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const startOfDate = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()
  const dayDiff = Math.round((startOfToday - startOfDate) / 86_400_000)
  const time = date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false })
  if (dayDiff === 0) return `今天 ${time}`
  if (dayDiff === 1) return `昨天 ${time}`
  if (dayDiff > 1 && dayDiff < 7) return `${dayDiff} 天前`
  if (date.getFullYear() === now.getFullYear()) {
    return date.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit' })
  }
  return date.toLocaleDateString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit' })
}

export function formatFullDate(timestamp?: number): string {
  const date = timestampToDate(timestamp)
  return date ? date.toLocaleString('zh-CN', { hour12: false }) : '时间未知'
}

export function formatFileSize(bytes?: number): string {
  if (!bytes) return '0 B'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export function latestModifiedAt(sessions: SessionSummary[]): number | undefined {
  const values = sessions.map(session => session.modifiedAt || 0)
  const latest = values.length ? Math.max(...values) : 0
  return latest || undefined
}

export function historyItemText(item: HistoryItem): string {
  return item.message.content.map(content => content.text).filter(Boolean).join('\n\n')
}

export function embeddedSummaryOf(session: IdeSession): { text: string; historyIndex: number } | null {
  const first = session.history[0]
  if (!first || first.message.role !== 'user') return null
  const text = historyItemText(first).trim()
  if (!text) return null
  const hasMarker = /(CONTEXT TRANSFER|##\s*TASK|conversation summary|上下文压缩|对话摘要)/i.test(text)
  const continuedTransfer = /\(Continued\)/i.test(session.title)
    && text.length > 800
    && /(context|summary|previous|task|上下文|摘要)/i.test(text)
  return hasMarker || continuedTransfer ? { text, historyIndex: 0 } : null
}

export function SourceBadge({ source, compact = false }: { source?: string; compact?: boolean }) {
  const meta = SOURCE_META[source || 'ide'] || SOURCE_META.ide
  return (
    <Badge
      variant="outline"
      className={`${compact ? 'h-[18px] px-1.5 text-[9px]' : 'h-5 px-2 text-[10px]'} rounded-md font-semibold ${meta.cls}`}
    >
      {meta.label}
    </Badge>
  )
}

interface RoleMeta {
  label: string
  icon: LucideIcon
  avatarClass: string
  cardClass: string
  labelClass: string
}

const ROLE_META: Record<string, RoleMeta> = {
  user: {
    label: '用户',
    icon: UserRound,
    avatarClass: 'border-blue-500/20 bg-blue-500/10 text-blue-600 dark:text-blue-400',
    cardClass: 'border-blue-500/20 bg-blue-500/[0.045]',
    labelClass: 'text-blue-600 dark:text-blue-400',
  },
  assistant: {
    label: '助手',
    icon: Bot,
    avatarClass: 'border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400',
    cardClass: 'border-border/70 bg-card/65',
    labelClass: 'text-emerald-600 dark:text-emerald-400',
  },
  system: {
    label: '系统',
    icon: Settings2,
    avatarClass: 'border-amber-500/20 bg-amber-500/10 text-amber-600 dark:text-amber-400',
    cardClass: 'border-amber-500/20 bg-amber-500/[0.045]',
    labelClass: 'text-amber-600 dark:text-amber-400',
  },
  artifact: {
    label: '产物',
    icon: FileCode2,
    avatarClass: 'border-violet-500/20 bg-violet-500/10 text-violet-600 dark:text-violet-400',
    cardClass: 'border-violet-500/20 bg-violet-500/[0.045]',
    labelClass: 'text-violet-600 dark:text-violet-400',
  },
}

export function ConversationSummaryCard({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false)
  const isLong = text.length > 700 || text.split('\n').length > 12

  useEffect(() => {
    setExpanded(false)
  }, [text])

  return (
    <section className="overflow-hidden rounded-2xl border border-blue-500/20 bg-gradient-to-br from-blue-500/[0.08] via-blue-500/[0.035] to-transparent shadow-sm">
      <div className="flex items-center gap-2.5 border-b border-blue-500/15 px-4 py-3">
        <span className="flex size-7 items-center justify-center rounded-lg bg-blue-500/10 text-blue-600 dark:text-blue-400">
          <Sparkles className="size-3.5" />
        </span>
        <div className="min-w-0 flex-1">
          <h3 className="text-xs font-semibold text-foreground">上下文摘要</h3>
          <p className="text-[10px] text-muted-foreground">由历史会话压缩生成，帮助快速恢复上下文</p>
        </div>
        {isLong && (
          <button
            type="button"
            onClick={() => setExpanded(value => !value)}
            className="rounded-md px-2 py-1 text-[11px] font-medium text-blue-600 transition-colors hover:bg-blue-500/10 dark:text-blue-400"
          >
            {expanded ? '收起' : '展开全部'}
          </button>
        )}
      </div>
      <div className="relative px-4 py-3.5">
        <div className={isLong && !expanded ? 'max-h-44 overflow-hidden' : ''}>
          <Markdown text={text} />
        </div>
        {isLong && !expanded && (
          <div className="pointer-events-none absolute inset-x-0 bottom-0 h-16 bg-gradient-to-t from-background/95 to-transparent" />
        )}
      </div>
    </section>
  )
}

export function SessionMessageCard({
  item,
  index,
  copied,
  onCopy,
}: {
  item: HistoryItem
  index: number
  copied: boolean
  onCopy: (item: HistoryItem) => void
}) {
  const role = item.message.role || 'system'
  const meta = ROLE_META[role] || ROLE_META.system
  const Icon = meta.icon

  return (
    <article className="group/message flex items-start gap-3">
      <span className={`mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-xl border shadow-sm ${meta.avatarClass}`}>
        <Icon className="size-4" />
      </span>
      <div className={`min-w-0 flex-1 rounded-2xl border px-4 py-3 shadow-[0_1px_2px_rgb(15_23_42/0.03)] ${meta.cardClass}`}>
        <div className="mb-2 flex items-center gap-2">
          <span className={`text-[11px] font-semibold ${meta.labelClass}`}>{meta.label}</span>
          <span className="text-[10px] tabular-nums text-muted-foreground/65">#{index + 1}</span>
          {item.message.isHidden && (
            <Badge variant="outline" className="h-[18px] px-1.5 text-[9px] text-muted-foreground">隐藏消息</Badge>
          )}
          <button
            type="button"
            onClick={() => onCopy(item)}
            className="ml-auto inline-flex size-6 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-all hover:bg-muted hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 group-hover/message:opacity-100"
            title="复制此条消息"
            aria-label="复制此条消息"
          >
            {copied ? <Check className="size-3.5 text-emerald-500" /> : <Copy className="size-3.5" />}
          </button>
        </div>
        <div className="space-y-2">
          {item.message.content.length === 0 ? (
            <p className="text-xs italic text-muted-foreground">空消息</p>
          ) : item.message.content.map((content, contentIndex) => (
            <div key={`${item.message.id}-${contentIndex}`}>
              {content.type && content.type !== 'text' && (
                <div className="mb-1 text-[9px] font-semibold uppercase tracking-wider text-muted-foreground/70">
                  {content.type}
                </div>
              )}
              <Markdown text={content.text} />
            </div>
          ))}
        </div>
      </div>
    </article>
  )
}

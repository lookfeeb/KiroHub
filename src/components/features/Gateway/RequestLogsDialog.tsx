import React, { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/components/ui/dialog'
import { Switch } from '@/components/ui/switch'
import { Activity, RefreshCw, XCircle, Trash2, Database } from 'lucide-react'

interface ProcessedRequestLog {
  id: string
  timestamp: string
  path: string
  status: number
  duration: number
  model?: string
  error?: string
  inputTokens?: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  upstream?: string
  errorType?: string
  outcome?: string
  stream?: boolean
}

interface GatewayRequestStats {
  total: number
  success: number
  error: number
  streaming: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCacheReadTokens: number
  totalCacheCreationTokens: number
  requestsWithCache: number
  maxDurationMs: number
  avgDurationMs: number
}

interface CacheStats {
  delta_cache_size: number
  lru_cache_size: number
  persistent_cache_enabled: boolean
}

interface RequestLogsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  logLevel: string
  onLogLevelChange: (level: string) => void
  logRequests: boolean
  onLogRequestsChange: (enabled: boolean) => void
  onSave?: () => void
}

export function RequestLogsDialog({ open, onOpenChange, logLevel, onLogLevelChange, logRequests, onLogRequestsChange, onSave }: RequestLogsDialogProps) {
  const [requestLogs, setRequestLogs] = useState<ProcessedRequestLog[]>([])
  const [requestStats, setRequestStats] = useState<GatewayRequestStats | null>(null)
  const [cacheStats, setCacheStats] = useState<CacheStats | null>(null)
  const [isRefreshing, setIsRefreshing] = useState(false)
  const [activeFilter, setActiveFilter] = useState<'all' | 'success' | 'error'>('all')
  const [expandedLogId, setExpandedLogId] = useState<string | null>(null)
  const [displayLimit, setDisplayLimit] = useState(50)
  const [searchText, setSearchText] = useState('')

  const filteredLogs = requestLogs.filter(log => {
    if (activeFilter === 'success') return log.status < 400
    if (activeFilter === 'error') return log.status >= 400
    return true
  }).filter(log => {
    if (!searchText) return true
    const lower = searchText.toLowerCase()
    return (log.model?.toLowerCase().includes(lower))
      || log.path.toLowerCase().includes(lower)
      || log.upstream?.toLowerCase().includes(lower)
      || log.error?.toLowerCase().includes(lower)
      || String(log.status).includes(lower)
  })

  const fetchRequestLogs = async (limit?: number) => {
    setIsRefreshing(true)
    try {
      const [logs, stats, cache] = await Promise.all([
        invoke<any[]>('get_gateway_request_logs', { limit: limit || displayLimit }),
        invoke<GatewayRequestStats>('get_gateway_request_stats'),
        invoke<CacheStats>('get_cache_stats').catch(() => null)
      ])

      setRequestLogs(logs.map(log => ({
        id: `${log.requestIndex}-${log.occurredAt}`,
        timestamp: log.occurredAt,
        path: log.endpoint,
        status: log.statusCode,
        duration: log.durationMs,
        model: log.model,
        error: log.error?.length > 500 ? log.error.substring(0, 500) + '...' : log.error,
        inputTokens: log.inputTokens,
        outputTokens: log.outputTokens,
        cacheReadTokens: log.cacheReadInputTokens,
        cacheCreationTokens: log.cacheCreationInputTokens,
        upstream: log.upstreamSource,
        errorType: log.errorType,
        outcome: log.outcome,
        stream: log.stream
      })))
      setRequestStats(stats)
      if (cache) setCacheStats(cache)
    } catch (error) {
      console.error('Failed to fetch request logs:', error)
    } finally {
      setIsRefreshing(false)
    }
  }

  const handleClearCache = async () => {
    try {
      await invoke('clear_all_cache')
      setCacheStats(prev => prev ? { ...prev, delta_cache_size: 0, lru_cache_size: 0 } : null)
    } catch { /* ignore */ }
  }

  useEffect(() => {
    if (!open) return
    fetchRequestLogs()
    const interval = setInterval(fetchRequestLogs, 5000)
    return () => clearInterval(interval)
  }, [open])

  return (
    <Dialog open={open} onOpenChange={(v) => { onOpenChange(v); if (!v && onSave) onSave() }}>
      <DialogContent className="sm:max-w-[950px]">
        <DialogHeader>
          <DialogTitle>请求日志</DialogTitle>
          <DialogDescription>实时查看网关请求记录、缓存与统计</DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          {/* 顶部：统计 + 缓存 + 操作 */}
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-3">
              <Activity size={16} />
              {requestStats && (
                <div className="flex items-center gap-2 text-sm">
                  <span>总 <strong>{requestStats.total}</strong></span>
                  <span className="text-green-600">成功 <strong>{requestStats.success}</strong></span>
                  <span className="text-red-600">错误 <strong>{requestStats.error}</strong></span>
                  {requestStats.totalInputTokens > 0 && (
                    <span className="text-muted-foreground">
                      {(requestStats.totalInputTokens / 1000).toFixed(1)}K入/{(requestStats.totalOutputTokens / 1000).toFixed(1)}K出
                      {requestStats.totalCacheReadTokens > 0 && (
                        <span className="text-blue-600">
                          /{(requestStats.totalCacheReadTokens / 1000).toFixed(1)}K缓存读
                        </span>
                      )}
                    </span>
                  )}
                </div>
              )}
            </div>
            <div className="flex gap-1.5 items-center flex-wrap">
              <input
                type="text"
                placeholder="搜索..."
                className="text-sm border rounded px-2 py-1 w-28"
                value={searchText}
                onChange={(e) => setSearchText(e.target.value)}
              />
              <select
                className="text-sm border rounded px-2 py-1"
                value={activeFilter}
                onChange={(e) => setActiveFilter(e.target.value as any)}
              >
                <option value="all">全部</option>
                <option value="success">成功</option>
                <option value="error">错误</option>
              </select>
              <select
                className="text-sm border rounded px-2 py-1"
                value={displayLimit}
                onChange={(e) => { setDisplayLimit(Number(e.target.value)); fetchRequestLogs(Number(e.target.value)) }}
              >
                <option value={50}>50条</option>
                <option value={100}>100条</option>
                <option value={200}>200条</option>
              </select>
              <Button variant="outline" size="sm" className="h-7 px-2" onClick={async () => { await invoke('clear_gateway_request_logs'); setRequestLogs([]); setRequestStats(null) }} disabled={requestLogs.length === 0}>
                清空
              </Button>
              <Button variant="outline" size="sm" className="h-7 px-2" onClick={() => {
                const content = filteredLogs.map(log => `[${log.timestamp}] ${log.status} ${log.path} ${log.model || '-'} ${log.duration}ms in:${log.inputTokens || 0} out:${log.outputTokens || 0} cR:${log.cacheReadTokens || 0} cW:${log.cacheCreationTokens || 0}${log.error ? ' ERR:' + log.error : ''}`).join('\n')
                const blob = new Blob([content], { type: 'text/plain' })
                const url = URL.createObjectURL(blob)
                const a = document.createElement('a')
                a.href = url
                a.download = `gateway-logs-${new Date().toISOString().replace(/[:.]/g, '-')}.log`
                a.click()
                URL.revokeObjectURL(url)
              }} disabled={filteredLogs.length === 0} title="导出日志">
                导出
              </Button>
              <Button variant="outline" size="sm" className="h-7 px-2" onClick={() => invoke('open_gateway_log_dir')} title="打开日志目录">
                📂
              </Button>
              <div className="flex items-center gap-1.5">
                <Switch size="sm" checked={logRequests} onCheckedChange={onLogRequestsChange} />
                <span className="text-sm text-muted-foreground">记录</span>
              </div>
              <select
                className="text-sm border rounded px-2 py-1"
                value={logLevel}
                onChange={(e) => onLogLevelChange(e.target.value)}
                title="日志级别"
              >
                <option value="debug">debug</option>
                <option value="info">info</option>
                <option value="warn">warn</option>
                <option value="error">error</option>
              </select>
              <Button variant="outline" size="sm" className="h-7 px-2" onClick={() => fetchRequestLogs()} disabled={isRefreshing}>
                <RefreshCw size={12} className={isRefreshing ? 'animate-spin' : ''} />
              </Button>
            </div>
          </div>

          {/* 日志表格 */}
          <div className="border rounded-lg">
            <div className="h-[60vh] min-h-[300px] overflow-auto">
              <table className="w-full text-xs font-mono">
                <thead className="sticky top-0 bg-muted/80 backdrop-blur z-10">
                  <tr className="border-b">
                    <th className="px-2 py-1.5 text-left w-[130px]">时间</th>
                    <th className="px-2 py-1.5 text-left">路径</th>
                    <th className="px-2 py-1.5 text-left">模型</th>
                    <th className="px-2 py-1.5 text-center w-[50px]">状态</th>
                    <th className="px-2 py-1.5 text-right w-[55px]">输入</th>
                    <th className="px-2 py-1.5 text-right w-[55px]">输出</th>
                    <th className="px-2 py-1.5 text-right w-[65px]" title="缓存读取">缓存读</th>
                    <th className="px-2 py-1.5 text-right w-[65px]" title="缓存写入">缓存写</th>
                    <th className="px-2 py-1.5 text-right w-[55px]">耗时</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredLogs.length === 0 ? (
                    <tr>
                      <td colSpan={9} className="text-center py-16 text-muted-foreground font-sans text-sm">
                        {requestLogs.length === 0 ? '等待第一个请求...' : '无匹配结果'}
                      </td>
                    </tr>
                  ) : (
                    filteredLogs.map((log) => (
                      <React.Fragment key={log.id}>
                        <tr
                          className="border-b border-border/50 hover:bg-muted/40 cursor-pointer transition-colors"
                          onClick={() => setExpandedLogId(expandedLogId === log.id ? null : log.id)}
                        >
                          <td className="px-2 py-1.5 text-muted-foreground whitespace-nowrap">{log.timestamp}</td>
                          <td className="px-2 py-1.5 text-muted-foreground truncate max-w-[140px]" title={log.path}>
                            {log.outcome === 'cached' ? <span className="text-blue-500">⚡</span> : null}
                            {log.path}
                          </td>
                          <td className="px-2 py-1.5 truncate max-w-[120px]" title={log.model}>{log.model || '-'}</td>
                          <td className="px-2 py-1.5 text-center">
                            <span className={log.status >= 400 ? 'text-red-500 font-semibold' : 'text-green-600'}>
                              {log.status}
                            </span>
                            {log.stream && <span className="ml-0.5 text-blue-400" title="流式">⇣</span>}
                          </td>
                          <td className="px-2 py-1.5 text-right text-muted-foreground">{log.inputTokens?.toLocaleString() || '-'}</td>
                          <td className="px-2 py-1.5 text-right text-muted-foreground">{log.outputTokens?.toLocaleString() || '-'}</td>
                          <td className="px-2 py-1.5 text-right text-blue-600" title="缓存读取 tokens">{log.cacheReadTokens?.toLocaleString() || '-'}</td>
                          <td className="px-2 py-1.5 text-right text-purple-600" title="缓存写入 tokens">{log.cacheCreationTokens?.toLocaleString() || '-'}</td>
                          <td className="px-2 py-1.5 text-right">
                            <span className={log.duration > 3000 ? 'text-orange-500' : 'text-muted-foreground'}>{log.duration}ms</span>
                          </td>
                        </tr>
                        {expandedLogId === log.id && log.error && (
                          <tr className="bg-red-50/50 dark:bg-red-950/10">
                            <td colSpan={9} className="px-3 py-2">
                              <div className="flex items-start gap-2">
                                <XCircle size={12} className="text-red-500 mt-0.5 shrink-0" />
                                <pre className="text-xs text-red-600 dark:text-red-400 whitespace-pre-wrap break-words">{log.error}</pre>
                              </div>
                            </td>
                          </tr>
                        )}
                      </React.Fragment>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

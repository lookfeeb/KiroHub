import { useEffect, useState, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Server, KeyRound, ShieldCheck, Loader2, RefreshCw, Globe, TerminalSquare, Boxes, Check, CircleCheck, Trash2 } from 'lucide-react'
import { DialogRoot, DialogContent, DialogBody, DialogTitle } from '../../shared/dialog'
import { useDialog } from '../../../contexts/DialogContext'

interface McpServerItem {
  name: string;
  type: 'command' | 'url';
  detail: string;
  disabled: boolean;
}

interface McpOAuthStatus {
  authorized: boolean;
  expiresAt: number;
  expiringSoon: boolean;
}

function StatBox({ icon: Icon, label, value, color }: { icon: any; label: string; value: number; color: string }) {
  return (
    <div className="flex items-center gap-2.5 rounded-xl border border-border/60 bg-gradient-to-br from-muted/40 to-muted/10 p-2.5">
      <span className={`flex h-8 w-8 items-center justify-center rounded-lg ${color}`}><Icon size={15} /></span>
      <div className="min-w-0">
        <div className="text-lg font-bold text-foreground leading-none">{value}</div>
        <div className="text-[10px] text-muted-foreground mt-1 truncate">{label}</div>
      </div>
    </div>
  )
}

function McpToolsModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { showConfirm } = useDialog()
  const [stats, setStats] = useState<any>(null)
  const [servers, setServers] = useState<McpServerItem[]>([])
  const [loading, setLoading] = useState(false)
  const [auth, setAuth] = useState<Record<string, McpOAuthStatus>>({})
  const [busy, setBusy] = useState<string | null>(null)
  const [refreshed, setRefreshed] = useState(false)

  const loadAuth = useCallback((items: McpServerItem[]) => {
    items.filter(s => s.type === 'url').forEach(s => {
      invoke<McpOAuthStatus>('mcp_oauth_status', { serverKey: s.name })
        .then(st => setAuth(prev => ({ ...prev, [s.name]: st })))
        .catch(() => {})
    })
  }, [])

  const load = useCallback(() => {
    setLoading(true)
    return Promise.all([
      invoke<any>('get_mcp_tool_stats', { projectDir: null }),
      invoke<any>('get_mcp_config', { projectDir: null }),
    ])
      .then(([s, cfg]) => {
        setStats(s)
        const items: McpServerItem[] = Object.entries(cfg?.mcpServers || {}).map(([name, v]: [string, any]) => ({
          name,
          type: (v?.url ? 'url' : 'command') as McpServerItem['type'],
          detail: v?.url || v?.command || '',
          disabled: !!v?.disabled,
        })).sort((a, b) => {
          if (a.type !== b.type) return a.type === 'url' ? -1 : 1
          return a.name.localeCompare(b.name)
        })
        setServers(items)
        loadAuth(items)
      })
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [loadAuth])

  const handleRefresh = useCallback(async () => {
    setLoading(true)
    // 异步并行：扫描外部工具的 MCP 配置并导入，同时继续走原刷新
    await Promise.allSettled([invoke('discover_and_import_mcp_servers'), load()])
    // 导入可能新增了服务器，再拉取一次以反映最新结果
    await load()
    setRefreshed(true)
    setTimeout(() => setRefreshed(false), 1500)
  }, [load])

  useEffect(() => {
    if (!open) return
    load()
    const un = listen('mcp-tokens-updated', () => setServers(cur => { loadAuth(cur); return cur }))
    return () => { un.then(f => f()) }
  }, [open, load, loadAuth])

  const authorize = useCallback(async (name: string) => {
    setBusy(name)
    try {
      await invoke('mcp_oauth_authorize', { serverKey: name })
      const st = await invoke<McpOAuthStatus>('mcp_oauth_status', { serverKey: name })
      setAuth(prev => ({ ...prev, [name]: st }))
    } catch (e) { console.error('授权失败', e) } finally { setBusy(null) }
  }, [])

  const revoke = useCallback(async (name: string) => {
    const ok = await showConfirm('撤销授权', `确定要撤销 ${name} 的授权吗？撤销后需重新授权才能使用该 MCP 工具。`, { confirmText: '撤销', cancelText: '取消' })
    if (!ok) return
    setBusy(name)
    try {
      await invoke('mcp_oauth_revoke', { serverKey: name })
      setAuth(prev => ({ ...prev, [name]: { authorized: false, expiresAt: 0, expiringSoon: false } }))
    } catch (e) { console.error('撤销失败', e) } finally { setBusy(null) }
  }, [showConfirm])

  const deleteServer = useCallback(async (name: string) => {
    const ok = await showConfirm('删除 MCP 服务器', `确定要删除 ${name} 吗？将同时移除其授权，并从导入来源同步删除，操作不可恢复。`, { confirmText: '删除', cancelText: '取消' })
    if (!ok) return
    setBusy(name)
    try {
      await invoke('delete_mcp_server_synced', { name })
      await load()
    } catch (e) { console.error('删除失败', e) } finally { setBusy(null) }
  }, [showConfirm, load])

  return (
    <DialogRoot open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent maxWidth="520px" className="p-0 gap-0 overflow-hidden">
        {/* 头部 */}
        <div className="shrink-0 px-5 py-4 border-b border-border bg-gradient-to-br from-cyan-500/12 via-cyan-500/[0.04] to-transparent">
          <div className="flex items-center gap-3">
            <div className="w-11 h-11 rounded-2xl bg-gradient-to-br from-cyan-500 to-sky-600 flex items-center justify-center shadow-lg shadow-cyan-500/25 ring-1 ring-white/15 shrink-0">
              <Server size={20} className="text-white" />
            </div>
            <div className="flex flex-col min-w-0 flex-1">
              <DialogTitle className="text-base leading-tight">MCP 工具</DialogTitle>
              <div className="flex items-center gap-1.5 mt-1.5">
                <span className="inline-flex items-center gap-1 text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-cyan-500/12 text-cyan-600">
                  <CircleCheck size={10} />{stats?.enabledServers ?? 0}/{stats?.totalServers ?? 0} 启用
                </span>
                <span className="inline-flex items-center gap-1 text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-violet-500/12 text-violet-600">
                  <KeyRound size={10} />{stats?.estimatedTools ?? 0} 工具
                </span>
              </div>
            </div>
            {/* 刷新：重新拉取最新 MCP 服务器 */}
            <button
              onClick={handleRefresh}
              disabled={loading}
              title="刷新 MCP 服务器"
              className="mr-8 inline-flex items-center justify-center h-9 w-9 rounded-xl bg-cyan-500/15 text-cyan-600 hover:bg-cyan-500/25 active:scale-90 disabled:cursor-not-allowed transition-all duration-150 shrink-0"
            >
              {loading
                ? <RefreshCw size={15} className="animate-spin" />
                : refreshed
                  ? <Check size={15} className="text-green-500" />
                  : <RefreshCw size={15} />}
            </button>
          </div>
        </div>

        {/* 内容 */}
        <DialogBody className="bg-muted/10">
          <div className="grid grid-cols-3 gap-2">
            <StatBox icon={Boxes} label="服务器总数" value={stats?.totalServers ?? 0} color="bg-cyan-500/15 text-cyan-600" />
            <StatBox icon={CircleCheck} label="已启用" value={stats?.enabledServers ?? 0} color="bg-green-500/15 text-green-600" />
            <StatBox icon={KeyRound} label="预估工具数" value={stats?.estimatedTools ?? 0} color="bg-violet-500/15 text-violet-600" />
          </div>

          {loading && servers.length === 0 ? (
            <div className="flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground">
              <Loader2 size={16} className="animate-spin" />加载中...
            </div>
          ) : servers.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-10 text-sm text-muted-foreground">
              <Server size={24} className="opacity-40" />
              <span>暂无 MCP 服务器</span>
              <span className="text-[11px] opacity-70">点击右上角「刷新」重新拉取</span>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              {servers.map(s => {
                const authorized = auth[s.name]?.authorized
                return (
                  <div key={s.name} className="group flex items-center justify-between gap-3 rounded-xl border border-border/60 bg-card px-3 py-2.5 hover:border-cyan-500/40 hover:shadow-sm transition-all">
                    <div className="flex items-start gap-2.5 min-w-0">
                      <span className={`mt-0.5 flex h-7 w-7 items-center justify-center rounded-lg shrink-0 ${s.type === 'url' ? 'bg-sky-500/12 text-sky-600' : 'bg-amber-500/12 text-amber-600'}`}>
                        {s.type === 'url' ? <Globe size={14} /> : <TerminalSquare size={14} />}
                      </span>
                      <div className="flex flex-col min-w-0">
                        <div className="flex items-center gap-1.5">
                          <span className="text-sm font-semibold text-foreground truncate">{s.name}</span>
                          <span className="text-[9px] uppercase tracking-wide text-muted-foreground font-bold px-1 py-px rounded bg-muted shrink-0">{s.type}</span>
                        </div>
                        <span className="text-[10px] font-mono text-muted-foreground truncate mt-0.5">{s.detail}</span>
                      </div>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0">
                      {s.type === 'url' && (
                        authorized ? (
                          <>
                            <span className="inline-flex items-center gap-1 text-[10px] font-bold px-1.5 py-0.5 rounded-full bg-green-500/12 text-green-600">
                              <ShieldCheck size={11} />{auth[s.name]?.expiringSoon ? '刷新中' : '已授权'}
                            </span>
                            <button onClick={() => revoke(s.name)} disabled={busy === s.name}
                              className="cursor-pointer disabled:cursor-not-allowed text-[10px] px-1.5 py-0.5 rounded-md border border-border text-muted-foreground hover:text-red-500 hover:border-red-500/40 disabled:opacity-50 transition-colors">
                              撤销
                            </button>
                          </>
                        ) : (
                          <button onClick={() => authorize(s.name)} disabled={busy === s.name}
                            className="cursor-pointer disabled:cursor-not-allowed inline-flex items-center gap-1 text-[10px] font-bold px-2 py-0.5 rounded-md bg-cyan-500/15 text-cyan-600 hover:bg-cyan-500/25 disabled:opacity-50 transition-colors">
                            {busy === s.name ? <Loader2 size={11} className="animate-spin" /> : <KeyRound size={11} />}授权
                          </button>
                        )
                      )}
                      <span className={`inline-flex items-center gap-1 text-[10px] font-bold px-1.5 py-0.5 rounded-full ${s.disabled ? 'bg-muted text-muted-foreground' : 'bg-green-500/12 text-green-600'}`}>
                        <span className={`w-1.5 h-1.5 rounded-full ${s.disabled ? 'bg-muted-foreground/50' : 'bg-green-500'}`} />
                        {s.disabled ? '已禁用' : '已启用'}
                      </span>
                      <button onClick={() => deleteServer(s.name)} disabled={busy === s.name} title="删除"
                        className="cursor-pointer disabled:cursor-not-allowed inline-flex items-center justify-center h-6 w-6 rounded-md border border-border text-muted-foreground hover:text-red-500 hover:border-red-500/40 disabled:opacity-50 transition-colors">
                        {busy === s.name ? <Loader2 size={11} className="animate-spin" /> : <Trash2 size={12} />}
                      </button>
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </DialogBody>
      </DialogContent>
    </DialogRoot>
  )
}

export default McpToolsModal

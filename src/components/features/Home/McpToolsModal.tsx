import { useEffect, useState, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Server, KeyRound, ShieldCheck, Loader2 } from 'lucide-react'
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

function StatBox({ label, value }: { label: string; value: number }) {
  return (
    <div className="bg-muted/40 border border-border rounded-lg p-2.5 text-center">
      <div className="text-lg font-bold text-foreground leading-tight">{value}</div>
      <div className="text-[10px] text-muted-foreground mt-0.5">{label}</div>
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

  const loadAuth = useCallback((items: McpServerItem[]) => {
    items.filter(s => s.type === 'url').forEach(s => {
      invoke<McpOAuthStatus>('mcp_oauth_status', { serverKey: s.name })
        .then(st => setAuth(prev => ({ ...prev, [s.name]: st })))
        .catch(() => {})
    })
  }, [])

  const load = useCallback(() => {
    setLoading(true)
    Promise.all([
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
        })).sort((a, b) => a.name.localeCompare(b.name))
        setServers(items)
        loadAuth(items)
      })
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [loadAuth])

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

  return (
    <DialogRoot open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent maxWidth="480px" className="p-0 gap-0 overflow-hidden">
        {/* 固定头部：与内容明显分隔 */}
        <div className="shrink-0 flex items-center gap-3 px-5 py-4 border-b border-border bg-gradient-to-r from-cyan-500/10 to-transparent">
          <div className="w-10 h-10 rounded-xl bg-cyan-500/15 flex items-center justify-center shadow-sm shrink-0">
            <Server size={20} className="text-cyan-500" />
          </div>
          <div className="flex flex-col min-w-0">
            <DialogTitle className="text-base">MCP 工具</DialogTitle>
            <span className="text-[11px] text-muted-foreground">
              已启用 {stats?.enabledServers ?? 0} / {stats?.totalServers ?? 0} 个服务器 · 预估 {stats?.estimatedTools ?? 0} 个工具
            </span>
          </div>
        </div>

        {/* 可滚动内容 */}
        <DialogBody className="bg-muted/10">
          {loading ? (
            <div className="py-8 text-center text-sm text-muted-foreground">加载中...</div>
          ) : (
            <>
              <div className="grid grid-cols-3 gap-2">
                <StatBox label="服务器总数" value={stats?.totalServers ?? 0} />
                <StatBox label="已启用" value={stats?.enabledServers ?? 0} />
                <StatBox label="预估工具数" value={stats?.estimatedTools ?? 0} />
              </div>
              <div className="flex flex-col gap-1.5">
                {servers.length === 0 ? (
                  <div className="text-sm text-muted-foreground text-center py-4">暂无 MCP 服务器</div>
                ) : servers.map(s => (
                  <div key={s.name} className="flex items-center justify-between gap-2 bg-card border border-border rounded-lg px-3 py-2 hover:border-cyan-500/30 transition-colors">
                    <div className="flex flex-col min-w-0">
                      <div className="flex items-center gap-2">
                        <Server size={13} className="text-cyan-500 shrink-0" />
                        <span className="text-sm font-medium text-foreground truncate">{s.name}</span>
                        <span className="text-[9px] uppercase text-muted-foreground font-medium px-1 py-px rounded bg-muted">{s.type}</span>
                      </div>
                      <span className="text-[10px] font-mono text-muted-foreground truncate mt-0.5">{s.detail}</span>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0">
                      {s.type === 'url' && (
                        auth[s.name]?.authorized ? (
                          <>
                            <span className="flex items-center gap-1 text-[10px] font-bold px-1.5 py-0.5 rounded bg-green-500/10 text-green-500">
                              <ShieldCheck size={11} />{auth[s.name]?.expiringSoon ? '刷新中' : '已授权'}
                            </span>
                            <button onClick={() => revoke(s.name)} disabled={busy === s.name}
                              className="cursor-pointer disabled:cursor-not-allowed text-[10px] px-1.5 py-0.5 rounded border border-border text-muted-foreground hover:text-red-500 hover:border-red-500/40 disabled:opacity-50 transition-colors">
                              撤销
                            </button>
                          </>
                        ) : (
                          <button onClick={() => authorize(s.name)} disabled={busy === s.name}
                            className="cursor-pointer disabled:cursor-not-allowed flex items-center gap-1 text-[10px] font-medium px-2 py-0.5 rounded bg-cyan-500/15 text-cyan-500 hover:bg-cyan-500/25 disabled:opacity-50 transition-colors">
                            {busy === s.name ? <Loader2 size={11} className="animate-spin" /> : <KeyRound size={11} />}
                            授权
                          </button>
                        )
                      )}
                      <span className={`text-[10px] font-bold px-1.5 py-0.5 rounded ${s.disabled ? 'bg-muted text-muted-foreground' : 'bg-green-500/10 text-green-500'}`}>
                        {s.disabled ? '已禁用' : '已启用'}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </DialogBody>
      </DialogContent>
    </DialogRoot>
  )
}

export default McpToolsModal

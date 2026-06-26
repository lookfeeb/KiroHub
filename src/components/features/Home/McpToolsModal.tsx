import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { Boxes, Check, CircleCheck, KeyRound, Loader2, RefreshCw, Server } from 'lucide-react'
import { DialogRoot, DialogContent, DialogBody, DialogTitle } from '../../shared/dialog'
import { useDialog } from '../../../contexts/DialogContext'
import McpServerRow from './mcpTools/McpServerRow'
import StatBox from './mcpTools/StatBox'
import { useMcpTools } from './mcpTools/useMcpTools'
import type { McpClient } from './mcpTools/types'
import { CLIENTS, authKey } from './mcpTools/utils'

function McpToolsModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { showConfirm } = useDialog()
  const [activeClient, setActiveClient] = useState<McpClient>('codex')
  const {
    stats,
    servers,
    loading,
    loadError,
    auth,
    busyMap,
    refreshed,
    refreshOk,
    copyOk,
    activeStats,
    load,
    refreshAll,
    reloadAuthForCurrentServers,
    authorize,
    cancelAuthorize,
    refreshOne,
    revoke,
    deleteServer,
    copyTo,
  } = useMcpTools({ activeClient, showConfirm })

  useEffect(() => {
    if (!open) return
    let disposed = false
    let unlisten: (() => void) | null = null

    load()
    listen('mcp-tokens-updated', reloadAuthForCurrentServers).then(fn => {
      if (disposed) {
        fn()
      } else {
        unlisten = fn
      }
    })

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [open, load, reloadAuthForCurrentServers])

  return (
    <DialogRoot open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent maxWidth="620px" className="p-0 gap-0 overflow-hidden">
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
            <button
              onClick={refreshAll}
              disabled={loading}
              title="刷新 MCP 服务器与 OAuth Token"
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

        <DialogBody className="bg-muted/10">
          <div className="grid grid-cols-3 gap-2">
            <StatBox icon={Boxes} label="当前服务器" value={activeStats.totalServers} color="bg-cyan-500/15 text-cyan-600" />
            <StatBox icon={CircleCheck} label="当前启用" value={activeStats.enabledServers} color="bg-green-500/15 text-green-600" />
            <StatBox icon={KeyRound} label="当前工具数" value={activeStats.estimatedTools} color="bg-violet-500/15 text-violet-600" />
          </div>

          <div className="grid grid-cols-3 gap-1 rounded-xl border border-border/60 bg-muted/30 p-1">
            {CLIENTS.map(c => (
              <button
                key={c.key}
                onClick={() => setActiveClient(c.key)}
                className={`h-8 cursor-pointer rounded-lg text-xs font-semibold transition-colors ${activeClient === c.key ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}`}
              >
                {c.label}
              </button>
            ))}
          </div>

          {loadError ? (
            <div className="flex flex-col items-center gap-2 rounded-xl border border-red-500/25 bg-red-500/10 px-4 py-8 text-center text-sm text-red-600">
              <Server size={24} className="opacity-70" />
              <span className="font-medium">MCP 配置加载失败</span>
              <span className="max-w-full break-words text-[11px] opacity-80">{loadError}</span>
            </div>
          ) : loading && servers.length === 0 ? (
            <div className="flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground">
              <Loader2 size={16} className="animate-spin" />加载中...
            </div>
          ) : servers.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-10 text-sm text-muted-foreground">
              <Server size={24} className="opacity-40" />
              <span>暂无 MCP 服务器</span>
              <span className="text-[11px] opacity-70">点击右上角刷新重新拉取</span>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              {servers.map(server => (
                <McpServerRow
                  key={authKey(server.client, server.name)}
                  server={server}
                  status={auth[authKey(server.client, server.name)]}
                  busyMap={busyMap}
                  refreshOk={refreshOk}
                  copyOk={copyOk}
                  onAuthorize={authorize}
                  onCancelAuthorize={cancelAuthorize}
                  onRefresh={refreshOne}
                  onRevoke={revoke}
                  onDelete={deleteServer}
                  onCopyTo={copyTo}
                />
              ))}
            </div>
          )}
        </DialogBody>
      </DialogContent>
    </DialogRoot>
  )
}

export default McpToolsModal

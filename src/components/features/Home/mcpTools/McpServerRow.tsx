import {
  Check,
  Copy,
  Globe,
  KeyRound,
  Loader2,
  RefreshCw,
  TerminalSquare,
  Trash2,
  Unlink,
} from 'lucide-react'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/overlays/dropdown-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/overlays/tooltip'
import { TooltipIconButton } from '@/components/ui/actions/tooltip-icon-button'
import type { McpClient, McpOAuthStatus, McpServerItem } from './types'
import { CLIENTS, authKey, authMeta, compactClientLabel, copyKey, isRemoteType, refreshKey } from './utils'

interface McpServerRowProps {
  server: McpServerItem;
  status?: McpOAuthStatus;
  busyMap: Record<string, boolean>;
  refreshOk: Record<string, boolean>;
  copyOk: Record<string, boolean>;
  onAuthorize: (server: McpServerItem) => void;
  onRefresh: (server: McpServerItem) => void;
  onRevoke: (server: McpServerItem) => void;
  onDelete: (server: McpServerItem) => void;
  onCopyTo: (server: McpServerItem, toClient: McpClient) => void;
}

function McpServerRow({
  server,
  status,
  busyMap,
  refreshOk,
  copyOk,
  onAuthorize,
  onRefresh,
  onRevoke,
  onDelete,
  onCopyTo,
}: McpServerRowProps) {
  const key = authKey(server.client, server.name)
  const rowBusy = !!busyMap[key]
  const remote = isRemoteType(server.type)
  const tokenRefreshKey = refreshKey(server.client, server.name)
  const meta = authMeta(status)
  const MetaIcon = meta.icon
  const copyTargets = CLIENTS.filter(c => c.key !== server.client)
  const activeCopyKey = copyTargets.find(c => busyMap[copyKey(server.client, server.name, c.key)])
  const successfulCopyKey = copyTargets.find(c => copyOk[copyKey(server.client, server.name, c.key)])
  const copyButtonTitle = successfulCopyKey
    ? `已复制到 ${successfulCopyKey.label}`
    : '复制到其它客户端'

  return (
    <div className="group flex items-center justify-between gap-3 rounded-xl border border-border/60 bg-card px-3 py-2.5 hover:border-cyan-500/40 hover:shadow-sm transition-all">
      <div className="flex items-start gap-2.5 min-w-0">
        <span className={`mt-0.5 flex h-7 w-7 items-center justify-center rounded-lg shrink-0 ${remote ? 'bg-sky-500/12 text-sky-600' : 'bg-amber-500/12 text-amber-600'}`}>
          {remote ? <Globe size={14} /> : <TerminalSquare size={14} />}
        </span>
        <div className="flex flex-col min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="text-sm font-semibold text-foreground truncate">{server.name}</span>
            <span className="text-[9px] uppercase tracking-wide text-muted-foreground font-bold px-1 py-px rounded bg-muted shrink-0">{server.type}</span>
            <span className="text-[9px] uppercase tracking-wide text-cyan-600 font-bold px-1 py-px rounded bg-cyan-500/10 shrink-0">{server.client}</span>
          </div>
          <span className="text-[10px] font-mono text-muted-foreground truncate mt-0.5">{server.detail}</span>
        </div>
      </div>

      <div className="flex items-center gap-1.5 shrink-0">
        {remote && (
          <>
            <span className={`inline-flex items-center gap-1 text-[10px] font-bold px-1.5 py-0.5 rounded-full ${meta.cls}`} title={status?.message || undefined}>
              <MetaIcon size={11} />{meta.label}
            </span>
            {status?.authorized ? (
              <>
                <TooltipIconButton
                  onClick={() => onRefresh(server)}
                  disabled={rowBusy || !!busyMap[tokenRefreshKey]}
                  tooltip="刷新 Token"
                  className="cursor-pointer disabled:cursor-not-allowed inline-flex items-center justify-center h-6 w-6 rounded-md border border-border text-muted-foreground hover:text-green-500 hover:border-green-500/40 disabled:opacity-50 transition-colors"
                >
                  {busyMap[tokenRefreshKey] ? <RefreshCw size={12} className="animate-spin" /> : refreshOk[tokenRefreshKey] ? <Check size={12} className="text-green-500" /> : <RefreshCw size={12} />}
                </TooltipIconButton>
                <TooltipIconButton
                  onClick={() => onRevoke(server)}
                  disabled={rowBusy}
                  tooltip="撤销授权"
                  className="cursor-pointer disabled:cursor-not-allowed inline-flex h-6 w-6 items-center justify-center rounded-md border border-border text-muted-foreground hover:text-red-500 hover:border-red-500/40 disabled:opacity-50 transition-colors"
                >
                  {rowBusy ? <Loader2 size={11} className="animate-spin" /> : <Unlink size={12} />}
                </TooltipIconButton>
              </>
            ) : (
              <button
                onClick={() => onAuthorize(server)}
                disabled={rowBusy}
                className="cursor-pointer disabled:cursor-not-allowed inline-flex items-center gap-1 text-[10px] font-bold px-2 py-0.5 rounded-md bg-cyan-500/15 text-cyan-600 hover:bg-cyan-500/25 disabled:opacity-50 transition-colors"
              >
                {rowBusy ? <Loader2 size={11} className="animate-spin" /> : <KeyRound size={11} />}授权
              </button>
            )}
          </>
        )}

        {remote && (
          <DropdownMenu>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="inline-flex">
                  <DropdownMenuTrigger asChild>
                    <button
                      className={`cursor-pointer inline-flex h-6 w-6 items-center justify-center rounded-md border transition-colors ${
                        successfulCopyKey
                          ? 'border-green-500/40 bg-green-500/10 text-green-600'
                          : 'border-border text-muted-foreground hover:text-cyan-600 hover:border-cyan-500/40'
                      }`}
                    >
                      {activeCopyKey ? (
                        <Loader2 size={12} className="animate-spin" />
                      ) : successfulCopyKey ? (
                        <Check size={12} className="text-green-500" />
                      ) : (
                        <Copy size={12} />
                      )}
                    </button>
                  </DropdownMenuTrigger>
                </span>
              </TooltipTrigger>
              <TooltipContent>{copyButtonTitle}</TooltipContent>
            </Tooltip>
            <DropdownMenuContent align="end" className="min-w-28">
              {copyTargets.map(c => {
                const targetCopyKey = copyKey(server.client, server.name, c.key)
                const copied = !!copyOk[targetCopyKey]
                const copying = !!busyMap[targetCopyKey]
                const targetLabel = compactClientLabel(c.label)
                return (
                  <Tooltip key={c.key}>
                    <TooltipTrigger asChild>
                      <DropdownMenuItem
                        disabled={copying}
                        onClick={() => onCopyTo(server, c.key)}
                        className="cursor-pointer text-xs"
                      >
                        {copying ? (
                          <Loader2 size={12} className="animate-spin" />
                        ) : copied ? (
                          <Check size={12} className="text-green-500" />
                        ) : (
                          <Copy size={12} />
                        )}
                        {copied ? '已复制' : targetLabel}
                      </DropdownMenuItem>
                    </TooltipTrigger>
                    <TooltipContent side="left">
                      {copied ? `已复制到 ${targetLabel}` : `复制到 ${targetLabel}`}
                    </TooltipContent>
                  </Tooltip>
                )
              })}
            </DropdownMenuContent>
          </DropdownMenu>
        )}

        <span className={`inline-flex items-center gap-1 text-[10px] font-bold px-1.5 py-0.5 rounded-full ${server.disabled ? 'bg-muted text-muted-foreground' : 'bg-green-500/12 text-green-600'}`}>
          <span className={`w-1.5 h-1.5 rounded-full ${server.disabled ? 'bg-muted-foreground/50' : 'bg-green-500'}`} />
          {server.disabled ? '已禁用' : '已启用'}
        </span>
        <TooltipIconButton
          onClick={() => onDelete(server)}
          disabled={rowBusy}
          tooltip="删除"
          className="cursor-pointer disabled:cursor-not-allowed inline-flex items-center justify-center h-6 w-6 rounded-md border border-border text-muted-foreground hover:text-red-500 hover:border-red-500/40 disabled:opacity-50 transition-colors"
        >
          {rowBusy ? <Loader2 size={11} className="animate-spin" /> : <Trash2 size={12} />}
        </TooltipIconButton>
      </div>
    </div>
  )
}

export default McpServerRow

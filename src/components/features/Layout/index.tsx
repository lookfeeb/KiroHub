
import { useState, useEffect, ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { ChevronLeft, ChevronRight, Monitor, Terminal } from 'lucide-react'
import { Button } from '../../ui/button'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../../ui/tooltip'
import { cn } from '../../../lib/utils'
import { useApp } from '../../../hooks/useApp'
import { routes } from '../../../routes'

interface SidebarProps {
  activeMenu: string;
  onMenuChange: (id: string) => void;
  onLogout?: () => void;
}

interface LocalToken {
    provider?: string;
    expiresAt?: string | number;
}

function useMenuItems() {
  const { t } = useApp()
  return routes.map(r => ({
    id: r.id,
    icon: r.icon,
    label: t(r.nameKey),
    desc: r.descKey ? t(r.descKey) : undefined
  }))
}

function Sidebar({ activeMenu, onMenuChange }: SidebarProps) {
  const [localToken, setLocalToken] = useState<LocalToken | null>(null)
  const [cliConnected, setCliConnected] = useState(false)
  const [cliAuth, setCliAuth] = useState<string>('')
  const [version, setVersion] = useState('')
  const [collapsed, setCollapsed] = useState(false)
  const { t } = useApp()
  const menuItems = useMenuItems()

  useEffect(() => {
    invoke<LocalToken>('get_kiro_local_token').then(setLocalToken).catch(() => {})
    getVersion().then(setVersion)
    const saved = localStorage.getItem('sidebar-collapsed')
    if (saved === 'true') setCollapsed(true)
    // CLI 登录态判断：已安装 + 数据库存在有效 token
    invoke<any>('check_cli_installation').then(info => {
      if (!info?.cli_installed) return
      invoke<string>('get_kiro_cli_default_path').then(path => {
        if (!path) return
        invoke<any>('read_cli_db_snapshot', { dbPath: path }).then(snap => {
          const entry = snap?.token_entries?.[0]
          if (entry?.parsed_token) {
            setCliConnected(true)
            setCliAuth(entry.key?.includes('social') ? 'Social' : 'IdC')
          }
        }).catch(() => {})
      }).catch(() => {})
    }).catch(() => {})
  }, [])

  const toggleCollapsed = () => {
    const newState = !collapsed
    setCollapsed(newState)
    localStorage.setItem('sidebar-collapsed', String(newState))
  }

  return (
    <div
      className={cn("flex flex-col relative transition-[width] duration-200 ease-in-out glass-sidebar z-10 overflow-hidden")}
      style={{ width: collapsed ? 64 : 192 }}
    >
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <div
              className={cn("cursor-pointer select-none", collapsed ? "p-2" : "p-3", "pb-2")}
              onClick={toggleCollapsed}
            >
              <div
                className={cn(
                  "flex items-center mb-2 animate-fade-in-up pb-2.5 border-b border-border/40",
                  collapsed ? "justify-center gap-0" : "justify-start gap-2.5"
                )}
                style={{ animationDelay: '0.1s', animationFillMode: 'both' }}
              >
                <div className="w-10 h-10 rounded-xl flex items-center justify-center flex-shrink-0 bg-gradient-to-br from-blue-500 to-purple-600 shadow-md shadow-blue-500/30 ring-1 ring-white/15 transition-transform hover:scale-105">
                  <svg width="22" height="22" viewBox="0 0 40 40" fill="none">
                    <path d="M20 4C12 4 6 10 6 18C6 22 8 25 8 25C8 25 7 28 7 30C7 32 8 34 10 34C11 34 12 33 13 32C14 33 16 34 20 34C24 34 26 33 27 32C28 33 29 34 30 34C32 34 33 32 33 30C33 28 32 25 32 25C32 25 34 22 34 18C34 10 28 4 20 4ZM14 20C12.5 20 11 18.5 11 17C11 15.5 12.5 14 14 14C15.5 14 17 15.5 17 17C17 18.5 15.5 20 14 20ZM26 20C24.5 20 23 18.5 23 17C23 15.5 24.5 14 26 14C27.5 14 29 15.5 29 17C29 18.5 27.5 20 26 20Z" fill="white"/>
                  </svg>
                </div>
                {!collapsed && (
                  <div className="flex flex-col gap-0 leading-tight">
                    <span className="text-lg font-bold tracking-wide sidebar-foreground">KiroHub</span>
                    <span className="text-[10px] sidebar-muted tracking-wide">账号管理中心</span>
                  </div>
                )}
              </div>
            </div>
          </TooltipTrigger>
          <TooltipContent side="right">
            {collapsed ? t('nav.expandSidebar') : t('nav.collapseSidebar')}
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>

      <div className={cn("flex flex-col gap-0.5 flex-1 overflow-auto no-scrollbar", collapsed ? "px-2" : "px-2")}>
        {menuItems.map((item, idx) => {
          const Icon = item.icon
          const isActive = activeMenu === item.id
          return (
            <TooltipProvider key={item.id}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    onClick={() => onMenuChange(item.id)}
                    className={cn(
                      "relative flex items-center gap-2.5 px-2.5 py-2.5 rounded-xl transition-all animate-slide-in-left cursor-pointer",
                      !isActive && "sidebar-foreground sidebar-hover font-normal",
                      isActive && "sidebar-active font-semibold shadow-sm"
                    )}
                    style={{
                      animationDelay: `${0.15 + idx * 0.05}s`,
                      animationFillMode: 'both'
                    }}
                  >
                    <Icon size={18} strokeWidth={isActive ? 2.5 : 2} className="shrink-0" />
                    {!collapsed && (
                      <div className="flex-1 text-left whitespace-nowrap overflow-hidden">
                        <div className="text-sm">{item.label}</div>
                        {item.desc && <div className="text-[10px] sidebar-muted leading-tight">{item.desc}</div>}
                      </div>
                    )}
                  </button>
                </TooltipTrigger>
                {collapsed && <TooltipContent side="right">{item.label}</TooltipContent>}
              </Tooltip>
            </TooltipProvider>
          )
        })}
      </div>

      <div className={cn("mx-3 mb-3 rounded-xl overflow-hidden sidebar-card", collapsed && "mx-2 flex justify-center p-2")}>
        {!collapsed ? (
          <div className="grid grid-cols-2 divide-x divide-border/40">
            {/* IDE */}
            <div className="flex flex-col gap-1 px-2.5 py-2.5">
              <div className="flex items-center gap-1.5">
                <Monitor size={13} className={localToken ? "text-green-600 dark:text-green-400" : "text-muted-foreground"} />
                <span className="text-[11px] font-bold sidebar-foreground">IDE</span>
                <span className={cn("ml-auto w-1.5 h-1.5 rounded-full", localToken ? "bg-green-500" : "bg-muted-foreground/40")} />
              </div>
              <span className={cn("text-[10px] truncate", localToken ? "sidebar-muted" : "text-muted-foreground/60")} title={localToken?.provider || ''}>
                {localToken ? (localToken.provider || 'Local') : '未连接'}
              </span>
            </div>
            {/* CLI */}
            <div className="flex flex-col gap-1 px-2.5 py-2.5">
              <div className="flex items-center gap-1.5">
                <Terminal size={13} className={cliConnected ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground"} />
                <span className="text-[11px] font-bold sidebar-foreground">CLI</span>
                <span className={cn("ml-auto w-1.5 h-1.5 rounded-full", cliConnected ? "bg-green-500" : "bg-muted-foreground/40")} />
              </div>
              <span className={cn("text-[10px] truncate", cliConnected ? "sidebar-muted" : "text-muted-foreground/60")} title={cliAuth || ''}>
                {cliConnected ? (cliAuth || 'CLI') : '未连接'}
              </span>
            </div>
          </div>
        ) : (
          <div className="flex flex-col items-center gap-1.5" title={`IDE ${localToken ? '已连接' : '未连接'} · CLI ${cliConnected ? '已连接' : '未连接'}`}>
            <Monitor size={13} className={localToken ? "text-green-500" : "text-muted-foreground/50"} />
            <Terminal size={13} className={cliConnected ? "text-emerald-500" : "text-muted-foreground/50"} />
          </div>
        )}
      </div>

      <div className={cn("px-3 pb-3 flex items-center gap-2", collapsed ? "flex-col" : "justify-between")}>
        <div className="flex items-center gap-1">
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={toggleCollapsed}
                  className="sidebar-foreground sidebar-hover h-7 w-7"
                >
                  {collapsed ? <ChevronRight size={14} /> : <ChevronLeft size={14} />}
                </Button>
              </TooltipTrigger>
              <TooltipContent side={collapsed ? "right" : "top"}>
                {collapsed ? t('nav.expandSidebar') : t('nav.collapseSidebar')}
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>

        {!collapsed && (
          <span className="text-[10px] ml-auto sidebar-muted font-mono tracking-tighter">v{version || '...'}</span>
        )}
      </div>
    </div>
  )
}

export default Sidebar

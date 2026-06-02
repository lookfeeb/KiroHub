import { useState, useEffect, useMemo, useCallback, useRef, type ReactNode } from 'react'
import { Users, Zap, Shield, TrendingUp, Sparkles, Server, RefreshCw, Terminal, Clock, CreditCard, User, KeyRound, Database, Folder, Check } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useApp } from '../../../hooks/useApp'
import { useDialog } from '../../../contexts/DialogContext'
import { useAccount } from '../../../contexts/AccountContext'
import { usePrivacy } from '../../../contexts/usePrivacy'
import { getThemeAccent } from '../KiroConfig/themeAccent'
import { getSubPlan, getMergedQuota, formatUsage } from '../../../utils/accountStats'
import { getProviderDisplayName, isGitHubProvider } from '../../../utils/accountProvider'
import { getAccountStatusMeta } from '../../../utils/accountStatus'
import { Card, CardContent } from '@/components/ui/data-display/card'
import { Badge } from '@/components/ui/data-display/badge'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/overlays/tooltip'
import { TooltipIconButton } from '@/components/ui/actions/tooltip-icon-button'

// 子组件
import LoadingSkeleton from './LoadingSkeleton'
import StatCard from './StatCard'
import McpToolsModal from './McpToolsModal'
import CliCommandPreview from '../Settings/CliCommandPreview'

interface HomeProps {
  onNavigate: (path: string) => void;
}

function Home({ onNavigate }: HomeProps) {
  const { t, theme } = useApp()
  const accent = useMemo(() => getThemeAccent(theme), [theme])

  const { showError } = useDialog()
  const { maskEmail } = usePrivacy()
  const {
    accounts: tokens,
    localToken,
    loading,
    refreshing,
    stats,
    currentAccount,
    currentQuotaInfo,
    refreshAccount,
  } = useAccount()
  
  const [refreshingAccount, setRefreshingAccount] = useState(false)
  const [refreshDone, setRefreshDone] = useState(false)
  const [mcpToolCount, setMcpToolCount] = useState(0)
  const [mcpModalOpen, setMcpModalOpen] = useState(false)
  const [ideInstallInfo, setIdeInstallInfo] = useState<any>(null)
  const mountedRef = useRef(true)
  const refreshDoneTimerRef = useRef<NodeJS.Timeout | null>(null)

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      if (refreshDoneTimerRef.current) {
        clearTimeout(refreshDoneTimerRef.current)
      }
    }
  }, [])

  // 检测 IDE 安装状态
  useEffect(() => {
    const checkIdeInstallation = async () => {
      try {
        const info = await invoke<any>('check_ide_installation')
        if (mountedRef.current) {
          setIdeInstallInfo(info)
        }
      } catch (e) {
        console.error('检测 IDE 安装状态失败:', e)
      }
    }
    checkIdeInstallation()
  }, [])

  // 加载 MCP 工具数量
  useEffect(() => {
    const loadMcpToolCount = async () => {
      try {
        const statsResult = await invoke<any>('get_mcp_clients_overview')
        if (mountedRef.current) {
          setMcpToolCount(statsResult.estimatedTools)
        }
      } catch (e) {
        // 静默处理
      }
    }
    loadMcpToolCount()
  }, [])

  // CLI 账号数据
  const [cliSnapshot, setCliSnapshot] = useState<any>(null)
  const [cliLoading, setCliLoading] = useState(false)
  const [cliPath, setCliPath] = useState('')
  const [cliInstalled, setCliInstalled] = useState(false)

  const loadCliData = useCallback(async () => {
    setCliLoading(true)
    try {
      const info = await invoke<any>('check_cli_installation')
      if (!mountedRef.current) return
      // 只根据可执行文件是否存在判断 CLI 是否安装
      setCliInstalled(info?.cli_installed || false)

      const path = await invoke<string>('get_kiro_cli_default_path')
      if (!mountedRef.current) return
      setCliPath(path || '')
      if (path) {
        try {
          const snapshot = await invoke<any>('read_cli_db_snapshot', { dbPath: path })
          if (mountedRef.current) {
            setCliSnapshot(snapshot)
          }
        } catch {
          if (mountedRef.current) {
            setCliSnapshot(null)
          }
          // 数据库存在但读取失败，或未登录
        }
      } else {
        if (mountedRef.current) {
          setCliSnapshot(null)
        }
      }
    } catch (e) {
      if (mountedRef.current) {
        setCliInstalled(false)
        setCliSnapshot(null)
      }
    } finally {
      if (mountedRef.current) {
        setCliLoading(false)
      }
    }
  }, [])

  // 刷新当前账号 + CLI token
  const handleRefreshCurrentAccount = useCallback(async () => {
    if (refreshingAccount || cliLoading) return
    setRefreshingAccount(true)
    setRefreshDone(false)
    const errors: string[] = []
    try {
      if (currentAccount) {
        try {
          await refreshAccount(currentAccount.id)
        } catch (e) {
          errors.push(String(e))
        }
      }

      if (cliPath) {
        try {
          const snapshot = await invoke<any>('refresh_cli_db_token', { dbPath: cliPath })
          if (mountedRef.current) {
            setCliSnapshot(snapshot)
          }
        } catch (e) {
          errors.push(String(e))
          await loadCliData()
        }
      } else {
        await loadCliData()
      }

      if (errors.length > 0) {
        if (mountedRef.current) {
          showError(t('common.refreshFailed'), errors.join('\n'))
        }
      } else {
        if (refreshDoneTimerRef.current) {
          clearTimeout(refreshDoneTimerRef.current)
        }
        if (!mountedRef.current) return
        setRefreshDone(true)
        refreshDoneTimerRef.current = setTimeout(() => {
          if (mountedRef.current) {
            setRefreshDone(false)
            refreshDoneTimerRef.current = null
          }
        }, 1500)
      }
    } finally {
      if (mountedRef.current) {
        setRefreshingAccount(false)
      }
    }
  }, [currentAccount, refreshingAccount, cliLoading, cliPath, refreshAccount, loadCliData, showError, t])

  // 加载 CLI 账号
  useEffect(() => {
    loadCliData()
  }, [loadCliData])

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | null = null
    listen('cli-token-updated', () => {
      if (!disposed) {
        loadCliData()
      }
    }).then(fn => {
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
  }, [loadCliData])

  // 统计卡片
  const statCards = useMemo(() => [
    { icon: Users, iconBg: "info-badge", iconColor: accent.text, value: stats.total, label: t('home.totalAccounts'), delay: 'delay-100' },
    { icon: Shield, iconBg: "success-badge", iconColor: accent.text, value: `${stats.active}/${stats.unavailable}`, label: t('home.activeVsUnavailable'), delay: 'delay-200' },
    { icon: Zap, iconBg: "bg-purple-500/10 text-purple-500", iconColor: accent.text, value: stats.proPlus + stats.pro, label: t('home.proAccounts'), delay: 'delay-300' },
    { icon: TrendingUp, iconBg: "warning-badge", iconColor: 'text-orange-500', value: `${stats.usagePercent}%`, label: t('home.usagePercent'), delay: 'delay-400' },
    { 
      icon: Server, 
      iconBg: "bg-cyan-500/10 text-cyan-500", 
      iconColor: accent.text,
      value: mcpToolCount, 
      label: 'MCP 工具', 
      delay: 'delay-500',
      onClick: () => setMcpModalOpen(true),
      warning: mcpToolCount > 50
    },
  ], [accent, stats, mcpToolCount, t])

  if (loading) {
    return <LoadingSkeleton />
  }

  return (
    <div className="h-full overflow-auto glass-main p-6">
      <div className="w-full">
        {/* Header（紧凑）*/}
        <div className="mb-4 flex items-center gap-2.5 animate-slide-in-left">
          <div className={`w-10 h-10 rounded-xl bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} flex items-center justify-center shadow-md ring-1 ring-primary/20`}>
            <Sparkles size={20} className="text-white" />
          </div>
          <div className="flex flex-col">
            <h1 className="text-lg font-semibold text-foreground leading-tight">{t('home.title')}</h1>
            <p className="text-sm text-muted-foreground leading-tight">{t('home.subtitle')}</p>
          </div>
        </div>

        {/* 统计卡片 */}
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-3 mb-3">
          {statCards.map((card, index) => (
            <StatCard key={index} {...card} />
          ))}
        </div>

        {/* 主卡片：当前账号 | CLI 账号 */}
        <Card className="card-glow animate-scale-in delay-300 overflow-hidden">
          <div className={`flex items-center justify-between px-4 py-3 border-b border-border bg-gradient-to-r ${accent.gradientFrom}/10 ${accent.gradientTo}/5`}>
            <div className="flex items-center gap-2.5">
              <div className={`w-7 h-7 rounded-xl bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} flex items-center justify-center shadow-md ring-1 ring-primary/20`}>
                <Sparkles size={13} className="text-white" />
              </div>
              <div className="flex flex-col leading-tight">
                <span className="text-sm font-semibold text-foreground tracking-wide">Kiro 账号</span>
                <span className="text-[10px] text-muted-foreground">IDE / CLI 当前登录态</span>
              </div>
            </div>
            <TooltipIconButton
              onClick={handleRefreshCurrentAccount}
              disabled={refreshingAccount || refreshing || cliLoading}
              tooltip={t('common.refresh')}
              className="group/button inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-transparent bg-clip-padding text-sm font-medium whitespace-nowrap transition-all outline-none select-none hover:bg-muted hover:text-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 active:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-50 aria-expanded:bg-muted aria-expanded:text-foreground dark:hover:bg-muted/50"
            >
              {refreshDone ? (
                <Check size={13} className="text-green-500" />
              ) : (
                <RefreshCw size={13} className={`text-muted-foreground ${refreshingAccount ? 'animate-spin' : ''}`} />
              )}
            </TooltipIconButton>
          </div>

          <CardContent className="p-0">
            <div className="grid grid-cols-1 md:grid-cols-[3fr_2fr]">
              {/* 左：当前 IDE 账号 */}
              <div className="p-4 flex flex-col gap-3">
                <span className="text-[10px] font-bold uppercase text-muted-foreground tracking-wider flex items-center gap-1.5">
                  <span className={`w-1.5 h-1.5 rounded-full ${accent.text.replace('text-', 'bg-')}`} />
                  当前 IDE 账号
                </span>
                {currentAccount ? (
                  <CurrentAccountDetail
                    account={currentAccount}
                    accent={accent}
                    maskEmail={maskEmail}
                    t={t}
                  />
                ) : (
                  <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm py-10">
                    {localToken ? '未匹配到账号' : (
                      ideInstallInfo?.ide_installed === false
                        ? (ideInstallInfo?.ide_executable_exists === false
                            ? 'Kiro IDE 未安装'
                            : 'Kiro IDE 已安装，未登录')
                        : t('home.notLoggedIn')
                    )}
                  </div>
                )}
              </div>

              {/* 右：CLI 账号 */}
              <div className="p-4 flex flex-col gap-3 bg-muted/20 border-t md:border-t-0 md:border-l border-border">
                <span className="text-[10px] font-bold uppercase text-muted-foreground tracking-wider flex items-center gap-1.5">
                  <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
                  <Terminal size={11} />
                  当前 CLI 账号
                </span>
                {cliLoading ? (
                  <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
                    加载中...
                  </div>
                ) : cliSnapshot ? (
                  <CliAccountDetail snapshot={cliSnapshot} cliPath={cliPath} accounts={tokens} maskEmail={maskEmail} />
                ) : cliInstalled ? (
                  <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm flex-col gap-1.5 py-8">
                    <Terminal size={20} className="text-muted-foreground/50" />
                    <span>CLI 已安装，未登录</span>
                    <span className="text-[11px] text-muted-foreground/70">请运行 kiro-cli login 登录</span>
                  </div>
                ) : (
                  <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm flex-col gap-1.5 py-8">
                    <Terminal size={20} className="text-muted-foreground/50" />
                    <span>CLI 未安装</span>
                    <span className="text-[11px] text-muted-foreground/70">请安装 Kiro CLI 后重启</span>
                  </div>
                )}
              </div>
            </div>

          </CardContent>
        </Card>
      </div>

      <McpToolsModal open={mcpModalOpen} onClose={() => setMcpModalOpen(false)} />
    </div>
  )
}

// CLI 账号详情解析
function parseCliExpiresAt(value: string) {
  const direct = new Date(value)
  if (!Number.isNaN(direct.getTime())) {
    return direct
  }
  return new Date(value.replace(/^(\d{4})\/(\d{1,2})\/(\d{1,2}) /, '$1-$2-$3T'))
}

function formatCliTokenKey(key?: string) {
  return (key || '').replace(/^(kirocli|codewhisperer):/, '') || 'token'
}

function statusBadgeClass(tone: 'success' | 'warning' | 'danger') {
  if (tone === 'success') return 'bg-green-500'
  if (tone === 'danger') return 'bg-red-500'
  return 'bg-orange-500'
}

function CliAccountDetail({ snapshot, cliPath, accounts, maskEmail }: {
  snapshot: any;
  cliPath: string;
  accounts: any[];
  maskEmail: (s: string) => string;
}) {
  const entries = snapshot?.token_entries || []
  const deviceReg = snapshot?.device_registration

  // 找到主 token 条目
  const mainEntry = entries[0]
  const tokenData = mainEntry?.parsed_token

  if (!tokenData) {
    return (
      <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm flex-col gap-2">
        <span>无有效 Token</span>
        <span className="text-[10px] font-mono truncate max-w-full">{cliPath}</span>
      </div>
    )
  }

  // 判断认证类型
  const isOidc = mainEntry.key?.includes('odic')
  const isSocial = mainEntry.key?.includes('social')
  const authMethod = isSocial ? 'Social' : isOidc ? 'IdC (BuilderId)' : 'Unknown'
  const matchedAccount = accounts.find(account =>
    (tokenData.refresh_token && account.refreshToken === tokenData.refresh_token) ||
    (tokenData.access_token && account.accessToken === tokenData.access_token)
  )
  const displayName = matchedAccount?.email
    ? maskEmail(matchedAccount.email)
    : matchedAccount?.userId || authMethod
  const providerName = getProviderDisplayName(matchedAccount?.provider) || authMethod
  const plan = matchedAccount ? getSubPlan(matchedAccount) : ''
  const tokenKeyLabel = formatCliTokenKey(mainEntry.key)

  // Token 过期判断
  let expiresStr = '-'
  let isExpired = false
  if (tokenData.expires_at) {
    const expiresDate = parseCliExpiresAt(tokenData.expires_at)
    expiresStr = expiresDate.toLocaleString()
    isExpired = expiresDate.getTime() < Date.now()
  }

  // 截断显示
  const truncate = (s: string, len = 16) => s ? (s.length > len ? s.substring(0, len) + '...' : s) : '-'
  const tokenRegion = tokenData.region || 'us-east-1'
  const deviceRegion = deviceReg?.region || 'us-east-1'

  return (
    <div className="flex-1 flex flex-col gap-2.5">
      {/* 状态 */}
      <div className="flex items-center gap-2.5 rounded-xl p-3 bg-gradient-to-br from-muted/40 to-muted/10 border border-border/60 hover:border-emerald-500/40 transition-all">
        <div className="relative shrink-0">
          <div className="w-10 h-10 rounded-xl flex items-center justify-center bg-gradient-to-br from-emerald-500 to-teal-600 text-white shadow-lg ring-1 ring-white/10">
            <Terminal size={16} />
          </div>
          <span className={`absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full ring-2 ring-background ${isExpired ? 'bg-red-500' : 'bg-green-500'}`} />
        </div>
        <div className="flex flex-col min-w-0 flex-1 gap-1">
            <span className="text-sm font-semibold text-foreground truncate">{displayName}</span>
            <div className="flex items-center gap-1.5 flex-wrap min-w-0">
              <span className="text-[11px] text-muted-foreground">{providerName}</span>
              <span className="text-[11px] text-muted-foreground font-mono truncate max-w-[150px]">{tokenKeyLabel}</span>
            </div>
          </div>
        {plan && (
          <Badge variant="default" className="shrink-0 text-[10px] px-1.5 py-0">
            {plan}
          </Badge>
        )}
        <Badge variant="default" className={`shrink-0 text-[10px] px-1.5 py-0 ${isExpired ? 'bg-red-500' : 'bg-green-500'}`}>
          {isExpired ? '已过期' : '有效'}
        </Badge>
      </div>

      {/* CLI 启动命令 */}
      <Panel>
        <PanelTitle icon={Terminal}>CLI 启动命令</PanelTitle>
        <CliCommandPreview />
      </Panel>

      {/* Token 信息 */}
      <Panel>
        <PanelTitle icon={KeyRound}>Token</PanelTitle>
        <div className="flex flex-col gap-1.5">
          <InfoRow label="Access Token" value={truncate(tokenData.access_token, 20)} mono />
          <InfoRow label="Refresh Token" value={truncate(tokenData.refresh_token, 20)} mono />
          <InfoRow label="过期时间" value={expiresStr} valueClass={isExpired ? 'text-red-500' : 'text-green-500'} />
          <InfoRow label="Region" value={tokenRegion} mono />
          {tokenData.start_url && <InfoRow label="Start URL" value={truncate(tokenData.start_url, 24)} mono />}
          {tokenData.oauth_flow && <InfoRow label="OAuth Flow" value={tokenData.oauth_flow} />}
          {tokenData.scopes && tokenData.scopes.length > 0 && <InfoRow label="Scopes" value={`${tokenData.scopes.length} 个`} />}
        </div>
      </Panel>

      {/* Device Registration */}
      {deviceReg && (
        <Panel>
          <PanelTitle icon={Shield}>Device Registration</PanelTitle>
          <div className="flex flex-col gap-1.5">
            <InfoRow label="Client ID" value={truncate(deviceReg.client_id, 20)} mono />
            <InfoRow label="Client Secret" value={truncate(deviceReg.client_secret, 20)} mono />
            {deviceRegion !== tokenRegion && <InfoRow label="Region" value={deviceRegion} mono />}
          </div>
        </Panel>
      )}

      {/* 数据库路径 */}
      <div className="mt-auto">
        <PathBar icon={Database} label="DB 路径" value={cliPath.split(/[/\\]/).slice(-2).join('/')} title={cliPath} />
      </div>
    </div>
  )
}

// 当前账号完整解析卡片
function CurrentAccountDetail({ account, accent, maskEmail, t }: {
  account: any;
  accent: any;
  maskEmail: (s: string) => string;
  t: any;
}) {
  const { quota, used, remaining, percent, overageEnabled, baseQuota, baseUsed, overageCap: mergedOverageCap, overageUsed } = getMergedQuota(account)
  const [, setTick] = useState(0)
  const nextResetRaw = account.usageData?.nextDateReset
  useEffect(() => {
    if (!nextResetRaw) return
    const id = setInterval(() => setTick(t => t + 1), 1000)
    return () => clearInterval(id)
  }, [nextResetRaw])
  const plan = getSubPlan(account)
  const email = account.usageData?.userInfo?.email || account.email || ''
  const provider = account.provider || ''
  const usageData = account.usageData
  const breakdown = usageData?.usageBreakdownList?.[0]
  const overageConfig = usageData?.overageConfiguration
  const subInfo = usageData?.subscriptionInfo
  const userInfo = usageData?.userInfo
  const nextReset = usageData?.nextDateReset
  const freeTrial = breakdown?.freeTrialInfo
  const bonuses = breakdown?.bonuses || []
  const mainUsed = breakdown?.currentUsage ?? 0
  const mainUsedPrecision = breakdown?.currentUsageWithPrecision ?? mainUsed
  const mainLimit = breakdown?.usageLimit ?? 0
  const mainLimitPrecision = breakdown?.usageLimitWithPrecision ?? mainLimit
  const mainPercent = mainLimit > 0 ? Math.round((mainUsed / mainLimit) * 100) : 0

  // 超额相关字段
  const currentOverages = breakdown?.currentOverages ?? 0
  const currentOveragesPrecision = breakdown?.currentOveragesWithPrecision ?? currentOverages
  const overageCap = breakdown?.overageCap ?? 0
  const overageCapPrecision = breakdown?.overageCapWithPrecision ?? overageCap
  const overageCharges = breakdown?.overageCharges ?? 0
  const overageRate = breakdown?.overageRate ?? 0

  const isOverage = currentOverages > 0
  const overageAmount = used > quota ? used - quota : 0
  const displayName = breakdown?.displayName || 'Credit'
  const displayNamePlural = breakdown?.displayNamePlural || 'Credits'
  const resourceType = breakdown?.resourceType || ''
  const currency = breakdown?.currency || ''
  const unit = breakdown?.unit || ''

  const getBarGradient = (pct: number) => {
    if (pct > 100) return 'from-purple-400 to-fuchsia-500'
    if (pct > 80) return 'from-red-400 to-rose-500'
    if (pct > 50) return 'from-amber-400 to-orange-500'
    return 'from-green-400 to-emerald-500'
  }

  const getPercentClass = (pct: number) => {
    if (pct > 100) return 'text-purple-500'
    if (pct > 80) return 'text-red-500'
    if (pct > 50) return 'text-amber-500'
    return 'text-green-500'
  }

  // 重置时间（实时倒计时）
  let resetStr = ''
  let daysUntilReset: number | null = null
  let resetDateStr = ''
  if (nextReset) {
    const resetDate = new Date(typeof nextReset === 'string' ? nextReset : (nextReset < 1e12 ? nextReset * 1000 : nextReset))
    resetDateStr = resetDate.toLocaleDateString()
    const diff = resetDate.getTime() - Date.now()
    daysUntilReset = Math.max(0, Math.ceil(diff / 86400000))
    if (diff <= 0) {
      resetStr = '即将重置'
    } else {
      const d = Math.floor(diff / 86400000)
      const h = Math.floor((diff % 86400000) / 3600000)
      const m = Math.floor((diff % 3600000) / 60000)
      const s = Math.floor((diff % 60000) / 1000)
      resetStr = `${d > 0 ? `${d}天` : ''}${h}时${m}分${s}秒后重置`
    }
  }

  // 超额使用百分比（相对于超额上限）
  const overagePercent = overageCap > 0 ? Math.round((currentOverages / overageCap) * 100) : 0
  const statusMeta = getAccountStatusMeta(account, t)
  const statusLabel = statusMeta.key === 'active' ? '有效' : statusMeta.label
  const statusCls = statusBadgeClass(statusMeta.tone)

  return (
    <div className="flex-1 flex flex-col gap-2.5">
      {/* 头部：邮箱 + 计划 + Provider */}
      <div className="flex items-center gap-2.5 rounded-xl p-3 bg-gradient-to-br from-muted/40 to-muted/10 border border-border/60 hover:border-primary/40 transition-all">
        <div className="relative shrink-0">
          <div className={`w-10 h-10 rounded-xl flex items-center justify-center text-white font-bold text-sm shadow-lg ring-1 ring-white/10 ${
            provider === 'Google' ? 'bg-gradient-to-br from-red-500 to-orange-500' :
            isGitHubProvider(provider) ? 'bg-gradient-to-br from-gray-700 to-gray-900' :
            `bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo}`
          }`}>
            {provider?.[0]?.toUpperCase() || 'K'}
          </div>
          <span className={`absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full ${statusCls} ring-2 ring-background`} />
        </div>
        <div className="flex flex-col min-w-0 flex-1 gap-1">
          <span className="text-sm font-semibold text-foreground truncate">
            {email ? maskEmail(email) : getProviderDisplayName(provider)}
          </span>
          <div className="flex items-center gap-1.5 flex-wrap">
            <span className="text-[11px] text-muted-foreground">{getProviderDisplayName(provider)}</span>
            {daysUntilReset != null && (
              <span className="inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-full bg-amber-500/10 text-amber-600 border border-amber-500/20 tabular-nums">
                <Clock size={10} />{resetStr}
              </span>
            )}
          </div>
        </div>
        {plan && (
          <Badge variant="default" className="shrink-0 text-[10px] px-1.5 py-0"
            style={{ background: plan.includes('PRO+') ? 'linear-gradient(to right, rgb(168, 85, 247), rgb(236, 72, 153))' : plan.includes('PRO') ? 'rgb(59, 130, 246)' : undefined }}>
            {plan}
          </Badge>
        )}
        <Badge variant="default" className={`shrink-0 text-[10px] px-1.5 py-0 ${statusCls}`}>
          {statusLabel}
        </Badge>
      </div>

      {/* 总配额进度 */}
      <Panel>
        <div className="flex items-center justify-between mb-2">
          <span className="text-xs font-medium text-foreground">本月用量 ({displayNamePlural})</span>
          <span className={`text-sm font-bold font-mono ${getPercentClass(percent)}`}>{percent}%</span>
        </div>
        <div className="h-2 bg-muted rounded-full overflow-hidden mb-2 shadow-inner">
          <div className={`h-full rounded-full bg-gradient-to-r ${getBarGradient(percent)} transition-all duration-700 ease-out`} style={{ width: `${Math.min(percent, 100)}%` }} />
        </div>
        <div className="flex items-center justify-between">
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="text-[11px] text-muted-foreground font-mono cursor-help underline decoration-dotted decoration-muted-foreground/40 underline-offset-2">
                {overageEnabled
                  ? <>正常 {formatUsage(baseQuota)} / 透支 {formatUsage(mergedOverageCap)} / 总 {formatUsage(quota)}</>
                  : <>{formatUsage(used)} / {formatUsage(quota)} {displayName}</>}
              </span>
            </TooltipTrigger>
            <TooltipContent>
              <div className="text-[11px] leading-relaxed">
                <div>正常配额：{formatUsage(baseUsed)} / {formatUsage(baseQuota)}</div>
                {overageEnabled && <div className="text-purple-300">透支配额：{formatUsage(overageUsed)} / {formatUsage(mergedOverageCap)}</div>}
                <div>总配额：{formatUsage(used)} / {formatUsage(quota)}（剩余 {formatUsage(remaining)}）</div>
              </div>
            </TooltipContent>
          </Tooltip>
          <span className={`text-[11px] font-semibold ${getPercentClass(percent)}`}>剩余 {formatUsage(remaining)}</span>
        </div>
      </Panel>

      {/* 超额详情（仅超额时显示） */}
      {currentOverages > 0 && (
        <div className="rounded-xl p-3 bg-gradient-to-br from-purple-500/10 to-fuchsia-500/5 border border-purple-500/25">
          <PanelTitle icon={Zap} color="text-purple-500">超额详情</PanelTitle>
          <div className="flex flex-col gap-1.5">
            <InfoRow label="超额用量" value={`${currentOveragesPrecision} / ${overageCapPrecision}`} valueClass="text-purple-500 font-semibold" mono />
            <div className="h-[3px] bg-purple-500/10 rounded-full overflow-hidden">
              <div className="h-full rounded-full bg-gradient-to-r from-purple-400 to-fuchsia-500 transition-all" style={{ width: `${Math.min(overagePercent, 100)}%` }} />
            </div>
            <InfoRow label="超额费用" value={`$${overageCharges.toFixed(2)} ${currency}`} valueClass="text-purple-500 font-semibold" mono />
            <InfoRow label="费率" value={`$${overageRate}/${displayName}`} mono />
          </div>
        </div>
      )}

      {/* 订阅 & 账号信息 两列 */}
      <div className="grid grid-cols-2 gap-2">
        {/* 订阅信息 */}
        {subInfo && (
          <Panel>
            <PanelTitle icon={CreditCard}>订阅</PanelTitle>
            <div className="flex flex-col gap-1.5">
              <InfoRow label="类型" value={subInfo.subscriptionTitle || 'Free'} />
              <InfoRow label="计划" value={subInfo.type?.replace('Q_DEVELOPER_STANDALONE_', '') || '-'} mono />
              <InfoRow label="超额能力" value={subInfo.overageCapability === 'OVERAGE_CAPABLE' ? '✓ 支持' : '✗'} valueClass={subInfo.overageCapability === 'OVERAGE_CAPABLE' ? 'text-green-500' : ''} />
              <InfoRow label="升级能力" value={subInfo.upgradeCapability === 'UPGRADE_CAPABLE' ? '✓ 可升级' : '✗'} valueClass={subInfo.upgradeCapability === 'UPGRADE_CAPABLE' ? 'text-green-500' : ''} />
              {overageConfig && (
                <InfoRow label="超额开关" value={overageConfig.overageStatus === 'ENABLED' ? '⚡ 已开启' : '已关闭'} valueClass={overageConfig.overageStatus === 'ENABLED' ? 'text-purple-500 font-semibold' : ''} />
              )}
              {subInfo.subscriptionManagementTarget && (
                <InfoRow label="管理" value={subInfo.subscriptionManagementTarget} mono />
              )}
            </div>
          </Panel>
        )}

        {/* 账号 & 资源信息 */}
        <Panel>
          <PanelTitle icon={User}>账号 & 资源</PanelTitle>
          <div className="flex flex-col gap-1.5">
            <InfoRow label="IDP" value={getProviderDisplayName(provider) || '-'} />
            <InfoRow label="重置日" value={resetDateStr || '-'} />
            {userInfo?.userId && (
              <InfoRow label="用户ID" value={userInfo.userId.split('.').pop()?.substring(0, 12) || '-'} mono />
            )}
            {resourceType && (
              <InfoRow label="资源类型" value={resourceType} mono />
            )}
            {currency && (
              <InfoRow label="货币" value={currency} />
            )}
            {unit && (
              <InfoRow label="计量单位" value={unit === 'INVOCATIONS' ? '调用次数' : unit} />
            )}
            {overageCap > 0 && (
              <InfoRow label="超额上限" value={`${overageCapPrecision}`} />
            )}
            {overageRate > 0 && (
              <InfoRow label="超额费率" value={`$${overageRate}/${displayName}`} mono />
            )}
          </div>
        </Panel>
      </div>

      {/* IDE Token 路径 */}
      <div className="mt-auto">
        <PathBar icon={Folder} label="Token 路径" value=".aws/sso/cache/" title="~/.aws/sso/cache/" />
      </div>
    </div>
  )
}

// 区块容器
function Panel({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <div className={`rounded-xl border border-border/50 bg-gradient-to-br from-muted/35 to-muted/10 p-3 transition-all hover:border-primary/30 hover:shadow-sm ${className}`}>{children}</div>
}

// 区块标题
function PanelTitle({ icon: Icon, children, color = 'text-muted-foreground' }: { icon?: any; children: ReactNode; color?: string }) {
  return (
    <div className="flex items-center gap-1.5 mb-2.5">
      {Icon && <span className={`flex h-5 w-5 items-center justify-center rounded-md bg-current/10 ${color}`}><Icon size={11} className={color} /></span>}
      <span className={`text-[10px] font-bold uppercase tracking-wider ${color}`}>{children}</span>
    </div>
  )
}

// 信息行
function InfoRow({ label, value, valueClass, mono }: {
  label: string;
  value: string;
  valueClass?: string;
  mono?: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-2 py-0.5 px-1 -mx-1 rounded-md hover:bg-muted/40 transition-colors">
      <span className="text-[11px] text-muted-foreground shrink-0">{label}</span>
      <span className={`text-[11px] ${valueClass || 'text-foreground'} ${mono ? 'font-mono' : ''} truncate max-w-[120px] text-right`} title={value}>{value}</span>
    </div>
  )
}

// 路径条
function PathBar({ icon: Icon, label, value, title }: { icon?: any; label: string; value: string; title?: string }) {
  return (
    <div className="flex items-center justify-between gap-2 rounded-lg border border-border/60 bg-gradient-to-r from-muted/50 to-muted/20 px-3 py-2">
      <span className="text-xs font-medium text-foreground/70 flex items-center gap-1.5 shrink-0">
        {Icon && <span className="flex h-5 w-5 items-center justify-center rounded-md bg-primary/10"><Icon size={12} className="text-primary" /></span>}{label}
      </span>
      <code className="text-xs font-mono text-foreground/90 truncate max-w-[220px]" title={title || value}>{value}</code>
    </div>
  )
}

export default Home

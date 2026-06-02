import { memo, useCallback, useMemo } from 'react'
import { Eye, Copy, Check, Edit2, RefreshCcw, ArrowLeftRight, Trash2, AlertCircle } from 'lucide-react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/overlays/tooltip'
import { TooltipIconButton } from '@/components/ui/actions/tooltip-icon-button'
import { useApp } from '../../../hooks/useApp'
import { usePrivacy } from '../../../contexts/usePrivacy'
import { Switch } from '@/components/ui/forms/switch'
import { getQuota, getUsed, getSubType, getSubPlan, formatUsage, getAccountDisplayName } from '../../../utils/accountStats'
import { getAccountStatusMeta, isBannedStatus, isUnavailableStatus } from '../../../utils/accountStatus'
import { getProviderDisplayName, isGitHubProvider } from '../../../utils/accountProvider'
import { Account, TagDefinition, GroupDefinition } from '../../../types/account'

interface AccountCardProps {
  account: Account;
  selectedIdsSet?: Set<string>;
  onSelect: (checked: boolean) => void;
  copiedId: string | null;
  onCopy: (text: string, id?: string) => void;
  onSwitch: (account: Account) => void;
  onRefresh: (id: string) => void;
  onRefreshToken?: (id: string) => void;
  onRefreshAll?: (id: string) => void;
  onEdit: (account: Account) => void;
  onEditLabel?: (account: Account) => void;
  onToggleEnabled?: (account: Account, enabled: boolean) => void;
  onToggleOverage?: (account: Account, enabled: boolean) => void;
  onDelete: (id: string) => void;
  isRefreshing?: boolean;
  isRefreshingToken?: boolean;
  isSwitching?: boolean;
  isTogglingOverage?: boolean;
  isCurrentAccount: boolean;
  isCliCurrent?: boolean;
  tagDefinitions?: TagDefinition[];
  groupDefinitions?: GroupDefinition[];
  availableModels?: any;
  availableModelsLoading?: boolean;
  availableModelsError?: string;
  onLoadAvailableModels?: (id: string, options?: { forceRefresh?: boolean }) => Promise<void>;
  onContextMenuOpen: (x: number, y: number) => void;
  index?: number;
}

const stop = (e: React.MouseEvent) => e.stopPropagation()

const AccountCard = memo(function AccountCard({
  account,
  selectedIdsSet,
  onSelect,
  copiedId,
  onCopy,
  onSwitch,
  onRefresh,
  onRefreshToken,
  onRefreshAll,
  onEdit,
  onEditLabel,
  onToggleEnabled,
  onToggleOverage,
  onDelete,
  isRefreshing = false,
  isRefreshingToken = false,
  isSwitching = false,
  isTogglingOverage = false,
  isCurrentAccount,
  isCliCurrent = false,
  tagDefinitions = [],
  groupDefinitions = [],
  onContextMenuOpen,
  index = 0
}: AccountCardProps) {
  const { t } = useApp()
  const { maskEmail } = usePrivacy()

  const isSelected = selectedIdsSet?.has(account.id) ?? false

  const d = useMemo(() => {
    const quota = getQuota(account)
    const used = getUsed(account)
    const percent = quota > 0 ? Math.round((used / quota) * 100) : 0
    const statusMeta = getAccountStatusMeta(account, t)
    const breakdown = account.usageData?.usageBreakdownList?.[0]
    return {
      quota, used, percent,
      subPlan: getSubPlan(account),
      subType: getSubType(account),
      statusMeta,
      isBanned: isBannedStatus(account),
      isNormal: statusMeta.key === 'active',
      isUnavailable: isUnavailableStatus(account),
      isExpired: !!account.expiresAt && new Date(account.expiresAt.replace(/\//g, '-')) < new Date(),
      breakdown,
      isOverage: (breakdown?.currentOverages ?? 0) > 0,
      nextDateReset: account.usageData?.nextDateReset,
    }
  }, [account, t])

  const { quota, used, percent, subPlan, statusMeta, isBanned, isNormal, isUnavailable, breakdown, isOverage, nextDateReset } = d

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    onContextMenuOpen(e.clientX, e.clientY)
  }, [onContextMenuOpen])

  const handleDoubleClick = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest('button, input, [role="switch"]')) return
    onEdit(account)
  }, [onEdit, account])

  // 状态主色：选中 > 当前(IDE) > 封禁 > 异常 > 正常
  const accent = isSelected ? 'primary'
    : isCurrentAccount ? 'green'
    : isBanned ? 'red'
    : !isNormal ? 'orange'
    : 'none'

  const ring = {
    primary: 'border-primary/60 bg-primary/[0.04]',
    green: 'border-green-500/50 bg-green-500/[0.04]',
    red: 'border-red-500/50 bg-red-500/[0.04]',
    orange: 'border-orange-500/45 bg-orange-500/[0.04]',
    none: 'border-border bg-card hover:border-primary/30',
  }[accent]

  const barColor = isOverage ? 'bg-purple-500' : percent > 80 ? 'bg-red-500' : percent > 50 ? 'bg-orange-500' : 'bg-green-500'
  const pctColor = isOverage ? 'text-purple-500' : percent > 80 ? 'text-red-500' : percent > 50 ? 'text-orange-500' : 'text-green-500'

  const overageCapable = account.usageData?.subscriptionInfo?.overageCapability === 'OVERAGE_CAPABLE'

  return (
    <div
      onContextMenu={handleContextMenu}
      onDoubleClick={handleDoubleClick}
      className={`group relative rounded-2xl border flex flex-col overflow-hidden min-h-[200px] animate-stagger transition-all duration-300 hover:shadow-lg hover:-translate-y-0.5 ${ring} ${account.enabled === false ? 'opacity-50 grayscale' : ''}`}
      style={{ animationDelay: `${Math.min(index, 20) * 30}ms` }}
    >
      {/* 顶部信息条 */}
      <div className="flex items-start gap-3 px-3.5 pt-3.5">
        <input
          type="checkbox"
          checked={isSelected}
          onChange={(e) => onSelect(e.target.checked)}
          onClick={stop}
          className="mt-1 w-4 h-4 rounded border-border text-primary focus:ring-primary/20 cursor-pointer flex-shrink-0"
        />
        <div className={`w-10 h-10 rounded-xl flex items-center justify-center text-base font-bold flex-shrink-0 shadow-sm ${
          account.provider === 'Google' ? 'bg-red-500/12 text-red-500'
          : isGitHubProvider(account.provider) ? 'bg-slate-500/12 text-slate-400'
          : 'bg-primary/12 text-primary'
        }`}>
          {getAccountDisplayName(account)[0].toUpperCase()}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1">
            <span className="font-semibold text-foreground text-[13px] truncate">
              {account.email ? maskEmail(account.email) : getAccountDisplayName(account)}
            </span>
            <button
              onClick={(e) => { stop(e); onCopy(getAccountDisplayName(account), account.id) }}
              className="p-0.5 rounded hover:bg-muted text-muted-foreground hover:text-primary transition-colors flex-shrink-0 cursor-pointer"
            >
              {copiedId === account.id ? <Check size={11} className="text-green-500" /> : <Copy size={11} />}
            </button>
          </div>
          <div className="text-[10px] text-muted-foreground truncate mt-0.5">
            {account.label || getProviderDisplayName(account.provider) || t('common.noLabel')}
          </div>
        </div>
        <span className={`flex-shrink-0 inline-flex px-2 py-0.5 rounded-full text-[9px] font-bold uppercase tracking-wider ${
          statusMeta.key === 'active' ? 'bg-green-500/12 text-green-500'
          : statusMeta.tone === 'danger' ? 'bg-red-500/12 text-red-500'
          : 'bg-orange-500/12 text-orange-500'
        }`}>{statusMeta.label}</span>
      </div>

      {/* 标签 + 开关 */}
      <div className="flex items-center justify-between gap-2 px-3.5 pt-2.5">
        <div className="flex items-center gap-1 flex-wrap min-w-0">
          <span className={`px-1.5 py-0.5 rounded-md text-[10px] font-bold ${
            subPlan.toUpperCase().includes('ENTERPRISE') ? 'bg-orange-500 text-white'
            : subPlan.includes('PRO') ? 'bg-primary text-primary-foreground'
            : 'bg-muted text-muted-foreground'
          }`}>{subPlan || 'Free'}</span>
          <span className="px-1.5 py-0.5 rounded-md bg-muted/50 text-muted-foreground text-[10px] font-medium border border-border/30">
            {getProviderDisplayName(account.provider) || t('common.unknown')}
          </span>
          {isCurrentAccount && <span className="px-1.5 py-0.5 rounded-md text-[10px] font-bold bg-green-500/15 text-green-600 border border-green-500/30">IDE</span>}
          {isCliCurrent && <span className="px-1.5 py-0.5 rounded-md text-[10px] font-bold bg-blue-500/15 text-blue-600 border border-blue-500/30">CLI</span>}
          {account.groupId && (() => {
            const group = groupDefinitions.find(g => g.id === account.groupId)
            if (!group) return null
            return <span className="text-[10px] px-1.5 py-0.5 rounded-md font-bold bg-muted/40 border border-border/50" style={{ color: group.color }}>{group.name}</span>
          })()}
          {account.tagLinks?.slice(0, 2).map(tagLink => {
            const tag = tagDefinitions.find(tg => tg.id === tagLink.tagId)
            return <span key={tagLink.tagId} className="text-[10px] px-1.5 py-0.5 rounded-full bg-primary/10 text-primary border border-primary/20 font-medium truncate max-w-[80px]">{tag?.name || tagLink.tagName}</span>
          })}
          {(account.tagLinks?.length || 0) > 2 && <span className="text-[10px] text-muted-foreground">+{account.tagLinks!.length - 2}</span>}
        </div>
        <div className="flex items-center gap-2 flex-shrink-0" onClick={stop}>
          <Tooltip>
            <TooltipTrigger asChild>
              <div><Switch size="sm" checked={account.enabled !== false} onCheckedChange={(c) => onToggleEnabled?.(account, c)} /></div>
            </TooltipTrigger>
            <TooltipContent>启用/禁用账号</TooltipContent>
          </Tooltip>
          {overageCapable && (
            <Tooltip>
              <TooltipTrigger asChild>
                <div className="flex items-center gap-0.5">
                  <span className="text-[9px] text-amber-500">⚡</span>
                  <Switch size="sm" checked={account.usageData?.overageConfiguration?.overageStatus === 'ENABLED'} disabled={isTogglingOverage} onCheckedChange={(c) => onToggleOverage?.(account, c)} />
                </div>
              </TooltipTrigger>
              <TooltipContent>超额开关</TooltipContent>
            </Tooltip>
          )}
        </div>
      </div>

      {/* 用量面板 */}
      <div className="px-3.5 pt-2.5 pb-3 flex-1 flex flex-col">
        <div className="rounded-xl bg-muted/30 border border-border/40 px-3 py-2.5">
          <div className="flex items-center justify-between text-[11px] mb-1.5">
            <span className="text-muted-foreground font-medium">{t('common.usage')}</span>
            <span className={`font-bold ${pctColor}`}>{isOverage ? '超额' : `${percent}%`}</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div className={`h-full rounded-full transition-all duration-700 ${barColor}`} style={{ width: `${Math.min(percent, 100)}%` }} />
          </div>
          <div className="flex items-center justify-between text-[10px] font-medium mt-1.5">
            <span className="text-foreground">{isOverage ? `${formatUsage(quota)} / ${formatUsage(quota)}` : `${formatUsage(used)} / ${formatUsage(quota)}`}</span>
            {isOverage
              ? <span className="text-purple-500 font-bold">超额 {formatUsage(breakdown!.currentOverages)}</span>
              : <span className="text-muted-foreground">剩余 {formatUsage(Math.max(0, quota - used))}</span>}
          </div>

          {isOverage && (
            <div className="pt-1.5 mt-1.5 border-t border-border/30">
              <div className="flex items-center justify-between text-[10px]">
                <span className="text-purple-500 font-medium">⚡ {formatUsage(breakdown!.currentOverages)}{breakdown!.overageCap ? ` / ${formatUsage(breakdown!.overageCap)}` : ''} credits</span>
                {breakdown!.overageCharges != null && <span className="text-purple-500 font-bold">${breakdown!.overageCharges.toFixed(2)}</span>}
              </div>
              {breakdown!.overageCap > 0 && (
                <div className="h-1 rounded-full bg-purple-500/10 mt-1 overflow-hidden">
                  <div className="h-full rounded-full bg-purple-500 transition-all duration-500" style={{ width: `${Math.min((breakdown!.currentOverages / breakdown!.overageCap) * 100, 100)}%` }} />
                </div>
              )}
            </div>
          )}
          {!isOverage && account.usageData?.overageConfiguration?.overageStatus === 'ENABLED' && overageCapable && (
            <div className="flex items-center justify-between text-[10px] pt-1.5 mt-1.5 border-t border-border/30">
              <span className="text-green-500 font-medium">⚡ 超额已开启{breakdown?.overageCap ? ` (上限 ${formatUsage(breakdown.overageCap)})` : ''}</span>
              {breakdown?.overageRate != null && <span className="text-muted-foreground">${breakdown.overageRate}/credit</span>}
            </div>
          )}
          {(account.expiresAt || nextDateReset) && (
            <div className="flex items-center justify-between text-[10px] pt-1.5 mt-1.5 border-t border-border/30 gap-2">
              {account.expiresAt && (
                <span className={`flex items-center gap-1 ${d.isExpired ? 'text-red-500 font-bold bg-red-500/10 px-1.5 py-0.5 rounded' : 'text-muted-foreground'}`}>
                  {d.isExpired && '⚠️ '}Token: {new Date(account.expiresAt.replace(/\//g, '-')).toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })}
                </span>
              )}
              {nextDateReset && (
                <span className="text-muted-foreground whitespace-nowrap">
                  {new Date(nextDateReset * 1000).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hour12: false })}重置
                </span>
              )}
            </div>
          )}
          {account.lastError && (
            <div className="flex items-start gap-1 text-[10px] pt-1.5 mt-1.5 border-t border-border/30 text-red-500 font-medium">
              <AlertCircle size={11} className="mt-0.5 flex-shrink-0" /><span className="break-all">{account.lastError}</span>
            </div>
          )}
        </div>

        {/* 操作栏 */}
        <div className="mt-auto pt-3 flex items-center gap-1.5">
          <button
            onClick={(e) => { stop(e); onSwitch(account) }}
            disabled={isSwitching || isUnavailable}
            className={`flex-1 h-8 px-2 rounded-lg inline-flex items-center justify-center gap-1.5 text-xs font-semibold transition-all cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 ${
              isCurrentAccount ? 'bg-primary/15 text-primary ring-1 ring-primary/30 hover:bg-primary/25' : 'bg-primary text-primary-foreground hover:opacity-90 shadow-sm'
            }`}
            title={t('accountCard.switch')}
          >
            <ArrowLeftRight size={13} className={isSwitching ? 'animate-spin' : ''} />
            {t('accountCard.switch')}
          </button>
          <div className="flex items-center gap-0.5">
            <TooltipIconButton tooltip={t('accountCard.viewDetails')} onClick={(e: React.MouseEvent<HTMLButtonElement>) => { stop(e); onEdit(account) }}
              className="h-8 w-8 rounded-lg inline-flex items-center justify-center hover:bg-muted text-muted-foreground hover:text-foreground transition-colors cursor-pointer">
              <Eye size={14} />
            </TooltipIconButton>
            <TooltipIconButton tooltip={t('accountCard.refresh')} disabled={isRefreshing || isRefreshingToken}
              onClick={(e: React.MouseEvent<HTMLButtonElement>) => { stop(e); onRefreshAll ? onRefreshAll(account.id) : (onRefresh(account.id), onRefreshToken?.(account.id)) }}
              className="h-8 w-8 rounded-lg inline-flex items-center justify-center hover:bg-muted text-muted-foreground hover:text-primary transition-colors disabled:opacity-50 cursor-pointer">
              <RefreshCcw size={14} className={(isRefreshing || isRefreshingToken) ? 'animate-spin' : ''} />
            </TooltipIconButton>
            <TooltipIconButton tooltip={t('accountCard.editRemark')} onClick={(e: React.MouseEvent<HTMLButtonElement>) => { stop(e); onEditLabel ? onEditLabel(account) : onEdit(account) }}
              className="h-8 w-8 rounded-lg inline-flex items-center justify-center hover:bg-muted text-muted-foreground hover:text-foreground transition-colors cursor-pointer">
              <Edit2 size={14} />
            </TooltipIconButton>
            <TooltipIconButton tooltip={t('accountCard.delete')} onClick={(e: React.MouseEvent<HTMLButtonElement>) => { stop(e); onDelete(account.id) }}
              className="h-8 w-8 rounded-lg inline-flex items-center justify-center hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors cursor-pointer">
              <Trash2 size={14} />
            </TooltipIconButton>
          </div>
        </div>
      </div>
    </div>
  )
})

export default AccountCard

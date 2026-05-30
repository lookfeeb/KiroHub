import { memo, useCallback, useMemo } from 'react'
import { Eye, Copy, Check, Edit2, RefreshCcw, ArrowLeftRight, Trash2 } from 'lucide-react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useApp } from '../../../hooks/useApp'
import { usePrivacy } from '../../../contexts/PrivacyContext'
import { Switch } from '../../ui/switch'
import { getUsagePercent, getProgressBarColor } from './hooks/useAccountStats'
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

const TipButton = ({ tip, children, ...props }: any) => (
  <Tooltip>
    <TooltipTrigger asChild>
      <button {...props}>{children}</button>
    </TooltipTrigger>
    <TooltipContent>{tip}</TooltipContent>
  </Tooltip>
)

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

  const cardData = useMemo(() => {
    const quota = getQuota(account)
    const used = getUsed(account)
    const subType = getSubType(account)
    const subPlan = getSubPlan(account)
    const percent = quota > 0 ? Math.round((used / quota) * 100) : 0
    const statusMeta = getAccountStatusMeta(account, t)
    const isBanned = isBannedStatus(account)
    const isNormal = statusMeta.key === 'active'
    const isUnavailable = isUnavailableStatus(account)
    const isExpired = account.expiresAt && new Date(account.expiresAt.replace(/\//g, '-')) < new Date()
    const breakdown = account.usageData?.usageBreakdownList?.[0]
    const nextDateReset = account.usageData?.nextDateReset

    return { quota, used, subType, subPlan, percent, statusMeta, isBanned, isNormal, isUnavailable, isExpired, breakdown, nextDateReset }
  }, [account, t])

  const { quota, used, subPlan, percent, statusMeta, isBanned, isNormal, isUnavailable, breakdown, nextDateReset } = cardData

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    onContextMenuOpen(e.clientX, e.clientY)
  }, [onContextMenuOpen])

  const handleDoubleClick = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest('button, input, [role="switch"]')) return
    onEdit(account)
  }, [onEdit, account])

  const cardStatusClass = isSelected
    ? "border-primary bg-primary/5 shadow-primary/10"
    : isCurrentAccount
      ? "border-green-500/50 bg-green-500/5 shadow-green-500/10"
      : isBanned
        ? "border-red-500/50 bg-red-500/5 shadow-red-500/10"
        : !isNormal
          ? "border-orange-500/40 bg-orange-500/5 shadow-orange-500/5"
          : "bg-card border-border hover:border-primary/30"

  return (
    <div
      onContextMenu={handleContextMenu}
      onDoubleClick={handleDoubleClick}
      className={`relative rounded-xl border flex flex-col min-h-[200px] animate-stagger transition-all duration-300 ${cardStatusClass} ${account.enabled === false ? 'opacity-50 grayscale' : ''}`}
      style={{ animationDelay: `${Math.min(index, 20) * 30}ms` }}
    >
      {isCurrentAccount && (
        <div className="absolute -top-px -left-px -right-px h-1 bg-gradient-to-r from-green-500/80 to-emerald-500/80 rounded-t-xl z-20" />
      )}

      {/* 顶部栏：勾选框 + 启用/超额开关 + 状态徽标（合并为一个块） */}
      <div className="flex items-center justify-between gap-2 px-3 pt-3 z-10">
        <input
          type="checkbox"
          checked={isSelected}
          onChange={(e) => onSelect(e.target.checked)}
          className="w-4 h-4 rounded border-border text-primary focus:ring-primary/20 cursor-pointer"
        />
        <div className="flex items-center gap-2">
          <div onClick={(e) => e.stopPropagation()} className="flex items-center gap-1">
            <Switch
              size="sm"
              checked={account.enabled !== false}
              onCheckedChange={(checked) => onToggleEnabled?.(account, checked)}
              title="启用/禁用账号"
            />
          </div>
          {account.usageData?.subscriptionInfo?.overageCapability === 'OVERAGE_CAPABLE' && (
            <div onClick={(e) => e.stopPropagation()} className="flex items-center gap-1">
              <span className="text-[9px] text-muted-foreground">⚡</span>
              <Switch
                size="sm"
                checked={account.usageData?.overageConfiguration?.overageStatus === 'ENABLED'}
                disabled={isTogglingOverage}
                onCheckedChange={(checked) => onToggleOverage?.(account, checked)}
                title="超额开关"
              />
            </div>
          )}
          <span className={`inline-flex px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider ${statusMeta.key === 'active'
            ? "bg-green-500/10 text-green-500 border border-green-500/20"
            : statusMeta.tone === 'danger'
              ? "bg-red-500/10 text-red-500 border border-red-500/20"
              : "bg-orange-500/10 text-orange-500 border border-orange-500/20"
            }`}>{statusMeta.label}</span>
        </div>
      </div>

      <div className="px-3 pb-3 pt-2 flex-1 flex flex-col gap-2">
        <div className="flex items-start gap-2.5">
          <div className={`w-9 h-9 rounded-lg flex items-center justify-center text-sm font-bold border border-border/50 flex-shrink-0 ${account.provider === 'Google' ? "bg-red-500/10 text-red-500" :
            isGitHubProvider(account.provider) ? "bg-slate-500/10 text-slate-500" :
              "bg-primary/10 text-primary"
            }`}>
            {getAccountDisplayName(account)[0].toUpperCase()}
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-1 mb-0.5">
              <span className="font-semibold text-foreground text-xs truncate">
                {account.email ? maskEmail(account.email) : getAccountDisplayName(account)}
              </span>
              <button
                onClick={() => onCopy(getAccountDisplayName(account), account.id)}
                className="p-0.5 rounded hover:bg-muted/80 text-muted-foreground hover:text-primary transition-colors"
              >
                {copiedId === account.id ? <Check size={10} className="text-green-500" /> : <Copy size={10} />}
              </button>
            </div>
            <div className="text-[10px] text-muted-foreground truncate">
              {account.label || getProviderDisplayName(account.provider) || t('common.noLabel')}
            </div>
          </div>
        </div>

        {/* Plan + Provider + 分组 + 标签（一行 wrap） */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className={`px-1.5 py-0.5 rounded text-[10px] font-bold ${(subPlan.toUpperCase().includes('ENTERPRISE'))
            ? 'bg-orange-500 text-white'
            : (subPlan.includes('PRO'))
              ? 'bg-primary text-primary-foreground'
              : 'bg-muted text-muted-foreground'
            }`}>
            {subPlan || 'Free'}
          </span>
          <span className="px-1.5 py-0.5 rounded bg-muted/50 text-muted-foreground text-[10px] font-medium border border-border/30">
            {getProviderDisplayName(account.provider) || t('common.unknown')}
          </span>
          {isCurrentAccount && (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-green-500/15 text-green-600 border border-green-500/30">IDE</span>
          )}
          {isCliCurrent && (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-blue-500/15 text-blue-600 border border-blue-500/30">CLI</span>
          )}
          {account.groupId && (() => {
            const group = groupDefinitions.find(g => g.id === account.groupId)
            if (!group) return null
            return (
              <span className="text-[10px] px-1.5 py-0.5 rounded font-bold bg-muted/40 border border-border/50" style={{ color: group.color }}>
                {group.name}
              </span>
            )
          })()}
          {account.tagLinks?.slice(0, 2).map(tagLink => {
            const tag = tagDefinitions.find(t => t.id === tagLink.tagId)
            return (
              <span key={tagLink.tagId} className="text-[10px] px-1.5 py-0.5 rounded-full bg-primary/10 text-primary border border-primary/20 font-medium truncate max-w-[80px]">
                {tag?.name || tagLink.tagName}
              </span>
            )
          })}
          {(account.tagLinks?.length || 0) > 2 && (
            <span className="text-[10px] text-muted-foreground">+{account.tagLinks!.length - 2}</span>
          )}
        </div>

        <div className="mt-1 pt-2 border-t border-border/30">
          <div className="flex items-center justify-between text-[11px] mb-1">
            <span className="text-muted-foreground font-medium">{t('common.usage')}</span>
            <span className={`font-bold ${
              (breakdown?.currentOverages ?? 0) > 0 ? 'text-purple-500'
              : percent > 80 ? 'text-red-500'
              : percent > 50 ? 'text-orange-500'
              : 'text-green-500'
            }`}>
              {(breakdown?.currentOverages ?? 0) > 0 ? '超额' : `${Math.round(percent)}%`}
            </span>
          </div>
          <div className="h-1 bg-muted rounded-full overflow-hidden mb-1.5">
            <div
              className={`h-full rounded-full transition-all duration-700 ${
                (breakdown?.currentOverages ?? 0) > 0 ? 'bg-purple-500'
                : percent > 80 ? 'bg-red-500'
                : percent > 50 ? 'bg-orange-500'
                : 'bg-green-500'
              }`}
              style={{ width: `${Math.min(percent, 100)}%` }}
            />
          </div>
          <div className="flex items-center justify-between text-[10px] font-medium">
            <span className="text-foreground">
              {(breakdown?.currentOverages ?? 0) > 0
                ? `${formatUsage(quota)} / ${formatUsage(quota)}`
                : `${formatUsage(used)} / ${formatUsage(quota)}`}
            </span>
            {(breakdown?.currentOverages ?? 0) > 0 ? (
              <span className="text-purple-500 font-bold">超额 {formatUsage(breakdown!.currentOverages)}</span>
            ) : (
              <span className="text-muted-foreground">剩余 {formatUsage(Math.max(0, quota - used))}</span>
            )}
          </div>
          {breakdown?.currentOverages != null && breakdown.currentOverages > 0 && (
            <div className="pt-1.5 mt-1.5 border-t border-border/30">
              <div className="flex items-center justify-between text-[10px]">
                <span className="text-purple-500 font-medium">
                  ⚡ {formatUsage(breakdown.currentOverages)}{breakdown.overageCap ? ` / ${formatUsage(breakdown.overageCap)}` : ''} credits
                </span>
                {breakdown.overageCharges != null && (
                  <span className="text-purple-500 font-bold">${breakdown.overageCharges.toFixed(2)}</span>
                )}
              </div>
              {breakdown.overageCap > 0 && (
                <div className="h-1 rounded-full bg-purple-500/10 mt-1 overflow-hidden">
                  <div
                    className="h-full rounded-full bg-purple-500 transition-all duration-500"
                    style={{ width: `${Math.min((breakdown.currentOverages / breakdown.overageCap) * 100, 100)}%` }}
                  />
                </div>
              )}
            </div>
          )}
          {(breakdown?.currentOverages === 0 || breakdown?.currentOverages == null) && account.usageData?.overageConfiguration?.overageStatus === 'ENABLED' && account.usageData?.subscriptionInfo?.overageCapability === 'OVERAGE_CAPABLE' && (
            <div className="flex items-center justify-between text-[10px] pt-1.5 mt-1.5 border-t border-border/30">
              <span className="text-green-500 font-medium">⚡ 超额已开启{breakdown?.overageCap ? ` (上限 ${formatUsage(breakdown.overageCap)})` : ''}</span>
              {breakdown?.overageRate != null && (
                <span className="text-muted-foreground">${breakdown.overageRate}/credit</span>
              )}
            </div>
          )}
          {(account.expiresAt || nextDateReset) && (
            <div className="flex items-center justify-between text-[10px] pt-2 mt-2 border-t border-border/30 gap-2">
              {account.expiresAt && (
                <span className={`flex items-center gap-1 ${cardData.isExpired ? 'text-red-500 font-bold bg-red-500/10 px-1.5 py-0.5 rounded' : 'text-muted-foreground'}`}>
                  {cardData.isExpired && '⚠️ '}Token: {new Date(account.expiresAt.replace(/\//g, '-')).toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })}
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
            <div className="text-[10px] pt-1.5 border-t border-border/30 mt-1.5">
              <span className="text-red-500 font-medium">❌ {account.lastError}</span>
            </div>
          )}
        </div>

        <div className="mt-auto pt-2.5 border-t border-border/50 flex items-center gap-1">
          {/* 主操作：切换登录（弹窗内可选 IDE/CLI） */}
          <button
            onClick={(e) => { e.stopPropagation(); onSwitch(account) }}
            disabled={isSwitching || isUnavailable}
            className={`flex-1 h-8 px-2 rounded-md inline-flex items-center justify-center gap-1.5 text-xs font-medium transition-colors disabled:opacity-50 ${
              isCurrentAccount
                ? 'bg-primary/15 text-primary ring-1 ring-primary/30 hover:bg-primary/25'
                : 'bg-primary/10 text-primary hover:bg-primary/20'
            }`}
            title={t('accountCard.switch')}
          >
            <ArrowLeftRight size={13} className={isSwitching ? 'animate-spin' : ''} />
            {t('accountCard.switch')}
          </button>

          {/* 次操作：图标按钮组（复用 Tooltip 悬停） */}
          <div className="flex items-center gap-0.5 border-l border-border/50 pl-1 ml-0.5">
            <TipButton
              tip={t('accountCard.viewDetails')}
              onClick={(e: React.MouseEvent) => { e.stopPropagation(); onEdit(account) }}
              className="h-8 w-8 rounded-md inline-flex items-center justify-center hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
            >
              <Eye size={14} />
            </TipButton>
            <TipButton
              tip={t('accountCard.refresh')}
              onClick={(e: React.MouseEvent) => { e.stopPropagation(); onRefreshAll ? onRefreshAll(account.id) : (onRefresh(account.id), onRefreshToken?.(account.id)) }}
              disabled={isRefreshing || isRefreshingToken}
              className="h-8 w-8 rounded-md inline-flex items-center justify-center hover:bg-muted text-muted-foreground hover:text-primary transition-colors disabled:opacity-50 cursor-pointer"
            >
              <RefreshCcw size={14} className={(isRefreshing || isRefreshingToken) ? 'animate-spin' : ''} />
            </TipButton>
            <TipButton
              tip={t('accountCard.editRemark')}
              onClick={(e: React.MouseEvent) => { e.stopPropagation(); onEditLabel ? onEditLabel(account) : onEdit(account) }}
              className="h-8 w-8 rounded-md inline-flex items-center justify-center hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
            >
              <Edit2 size={14} />
            </TipButton>
            <TipButton
              tip={t('accountCard.delete')}
              onClick={(e: React.MouseEvent) => { e.stopPropagation(); onDelete(account.id) }}
              className="h-8 w-8 rounded-md inline-flex items-center justify-center hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors"
            >
              <Trash2 size={14} />
            </TipButton>
          </div>
        </div>
      </div>
    </div>
  )
})

export default AccountCard

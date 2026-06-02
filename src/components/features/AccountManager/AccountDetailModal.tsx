import { useState, useRef, useEffect, memo, useMemo } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { Copy, Check, RefreshCw, User, CreditCard, Shield, Cpu, Loader2, FileText, Image as ImageIcon, Zap, Hash, ChevronDown, X, Clock, Mail, Tag } from 'lucide-react'
import { useApp } from '../../../hooks/useApp'
import { useDialog } from '../../../contexts/DialogContext'
import { formatUsage, getAccountDisplayName, getMergedQuota } from '../../../utils/accountStats'
import { getAccountStatusMeta, isBannedStatus } from '../../../utils/accountStatus'
import { getProviderDisplayName, isGitHubProvider } from '../../../utils/accountProvider'
import {
  DialogRoot,
  DialogContent,
  DialogBody} from '../../shared/dialog'
import { Switch } from '@/components/ui/forms/switch'
import { Account } from '../../../types/account'
import ProviderBadge from '../../shared/ProviderBadge'
import React from 'react'

interface QuotaCardProps {
  title: string;
  used: number;
  quota: number;
  icon: string | React.ReactNode;
  status?: string;
  expiry?: string | null;
  colors: any;
  t: any;
}

// 配额卡片组件（优化性能）
const QuotaCard = memo(({ title, used, quota, icon, status, expiry, colors, t }: QuotaCardProps) => {
  const isActive = status === 'ACTIVE'
  const hasQuota = quota > 0
  
  return (
    <div className={`rounded-lg p-3 border transition-colors duration-200 hover:shadow-md ${
      hasQuota && isActive
        ? 'border-blue-500/30 bg-blue-500/5 shadow-blue-500/10'
        : `border-border bg-muted/30`
    }`}>
      <div className="flex items-center gap-2 mb-3">
        <div className={`w-2.5 h-2.5 rounded-full ${
          hasQuota && isActive
            ? title.includes('试用')
              ? 'bg-cyan-500 shadow-lg shadow-cyan-500/50'
              : title.includes('奖励')
                ? 'bg-purple-500 shadow-lg shadow-purple-500/50'
                : 'bg-blue-500 shadow-lg shadow-blue-500/50'
            : 'bg-gray-400'
        }`}></div>
        <span className={`text-xs font-medium uppercase tracking-wide ${
          hasQuota && isActive
            ? title.includes('试用')
              ? 'text-cyan-500'
              : title.includes('奖励')
                ? 'text-purple-500'
                : "text-muted-foreground"
            : "text-muted-foreground"
        }`}>{title}</span>
        {status && status !== 'ACTIVE' && (
          <span className={`text-xs px-2 py-0.5 rounded-md font-medium bg-muted/30 text-muted-foreground`}>
            {status}
          </span>
        )}
      </div>
      <div className={`text-2xl font-semibold text-foreground mb-1`}>
        {hasQuota ? (
          <>{formatUsage(used)} <span className={`text-base text-muted-foreground font-normal`}>/ {formatUsage(quota)}</span></>
        ) : (
          <span className={"text-muted-foreground"}>-</span>
        )}
      </div>
      {expiry && (
        <div className={`text-xs text-muted-foreground mt-2 flex items-center gap-1`}>
          <span>{icon}</span>
          <span>{expiry}</span>
        </div>
      )}
    </div>
  )
})

QuotaCard.displayName = 'QuotaCard'

interface AccountDetailModalProps {
  account: Account;
  onClose: () => void;
  onRefresh?: () => void;
}

function AccountDetailModal({ account, onClose, onRefresh }: AccountDetailModalProps) {
  const { t } = useApp()
  const { showError } = useDialog()
  const [currentAccount, setCurrentAccount] = useState<Account>(account)

  // 样式定义
  const colors = useMemo(() => ({
    inputFocus: 'focus:ring-primary/20 focus:border-primary'
  }), [])

  const initQuota = currentAccount.usageData?.usageBreakdownList?.[0]?.usageLimit ?? currentAccount.quota ?? 0
  const initUsed = currentAccount.usageData?.usageBreakdownList?.[0]?.currentUsage ?? currentAccount.used ?? 0
  
  const [form, setForm] = useState({
    email: currentAccount.email || getAccountDisplayName(currentAccount),
    label: currentAccount.label || '',
    quota: initQuota,
    used: initUsed,
    status: currentAccount.status,
    accessToken: currentAccount.accessToken || '',
    refreshToken: currentAccount.refreshToken || ''})

  const [refreshing, setRefreshing] = useState(false)
  const [copied, setCopied] = useState<string | null>(null)
  const copiedTimerRef = useRef<NodeJS.Timeout | null>(null)

  // Models 相关 state
  const [models, setModels] = useState<any[]>([])
  const [modelsLoading, setModelsLoading] = useState(false)
  const [modelsError, setModelsError] = useState<string | null>(null)
  const [modelsExpanded, setModelsExpanded] = useState(false)
  const mountedRef = useRef(true)

  // 获取可用模型
  const fetchModels = async (forceRefresh = false) => {
    if (!mountedRef.current) return
    setModelsLoading(true)
    setModelsError(null)
    try {
      console.log('[AccountDetailModal] Fetching models for account:', account.id, 'forceRefresh:', forceRefresh)
      const response = await invoke<any>('list_available_models', { 
        id: account.id, 
        forceRefresh 
      })
      console.log('[AccountDetailModal] Models response:', response)
      const modelsList = Array.isArray(response?.availableModels) ? response.availableModels : []
      console.log('[AccountDetailModal] Models list:', modelsList.length, 'models')
      if (!mountedRef.current) return
      setModels(modelsList)
    } catch (e) {
      console.error('[AccountDetailModal] Failed to fetch models:', e)
      if (mountedRef.current) {
        setModelsError(String(e))
      }
    } finally {
      if (mountedRef.current) {
        setModelsLoading(false)
      }
    }
  }

  // 清理timer
  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      if (copiedTimerRef.current) {
        clearTimeout(copiedTimerRef.current)
      }
    }
  }, [])

  // 初始化时获取模型
  useEffect(() => {
    fetchModels()
  }, [account.id])

  useEffect(() => {
    setCurrentAccount(account)
    setForm({
      email: account.email || getAccountDisplayName(account),
      label: account.label || '',
      quota: account.usageData?.usageBreakdownList?.[0]?.usageLimit ?? account.quota ?? 0,
      used: account.usageData?.usageBreakdownList?.[0]?.currentUsage ?? account.used ?? 0,
      status: account.status,
      accessToken: account.accessToken || '',
      refreshToken: account.refreshToken || ''})
  }, [account])

  const handleRefresh = async () => {
    setRefreshing(true)
    try {
      const result = await invoke<{ account: Account, warning?: string }>('sync_account', { id: account.id })
      if (!mountedRef.current) return
      const updated = result.account
      setCurrentAccount(updated)
      
      // 如果有警告，显示提示
      if (result.warning) {
        await showError('同步警告', result.warning)
      }
      
      // 封禁账号额度为 0
      const isBanned = isBannedStatus(updated)
      const quota = isBanned ? 0 : (updated.usageData?.usageBreakdownList?.[0]?.usageLimit ?? 0)
      const used = updated.usageData?.usageBreakdownList?.[0]?.currentUsage ?? 0
      setForm(prev => ({ ...prev, quota, used, status: updated.status }))
    } catch (e) {
      if (!mountedRef.current) return
      const errorMsg = String(e)
      let status = '刷新失败'
      if (errorMsg.includes('BANNED')) {
        status = 'banned'
      } else if (errorMsg.includes('AUTH_ERROR') || errorMsg.includes('401') || errorMsg.includes('invalid')) {
        status = 'invalid'
      }
      setForm(prev => ({ ...prev, status }))
      await showError(t('detail.refreshFailed'), errorMsg)
    } finally {
      if (mountedRef.current) {
        setRefreshing(false)
      }
    }
  }

  const handleCopy = (text: string, field: string) => {
    navigator.clipboard.writeText(text).catch(e => console.error('Copy failed:', e))
    if (!mountedRef.current) return
    setCopied(field)
    if (copiedTimerRef.current) {
      clearTimeout(copiedTimerRef.current)
    }
    copiedTimerRef.current = setTimeout(() => {
      if (mountedRef.current) {
        setCopied(null)
      }
    }, 1500)
  }

  // 从 usageData 读取免费试用和奖励信息
  const breakdown = currentAccount.usageData?.usageBreakdownList?.[0]
  const freeTrialInfo = breakdown?.freeTrialInfo
  const bonuses = breakdown?.bonuses || []
  const now = Date.now()
  
  // 检查试用是否过期
  const trialExpiry = freeTrialInfo?.freeTrialExpiry ? freeTrialInfo.freeTrialExpiry * 1000 : 0
  const trialActive = freeTrialInfo?.freeTrialStatus === 'ACTIVE' || (trialExpiry > now)
  const freeTrialQuota = trialActive ? (freeTrialInfo?.usageLimit || 0) : 0
  const freeTrialUsed = trialActive ? (freeTrialInfo?.currentUsage || 0) : 0
  
  // 检查每个奖励是否过期（只计入未过期且状态为 ACTIVE 的奖励）
  let bonusQuota = 0, bonusUsed = 0
  bonuses.forEach(b => {
    const expiry = b.expiresAt ? b.expiresAt * 1000 : Infinity
    if (expiry > now && b.status === 'ACTIVE') {
      bonusQuota += b.usageLimit || 0
      bonusUsed += b.currentUsage || 0
    }
  })
  
  const merged = getMergedQuota(currentAccount)
  const overageEnabled = merged.overageEnabled
  const totalQuota = merged.quota + freeTrialQuota + bonusQuota
  const totalUsed = merged.used + freeTrialUsed + bonusUsed
  const totalPercent = totalQuota > 0 ? Math.min(100, (totalUsed / totalQuota) * 100) : 0
  const statusMeta = getAccountStatusMeta({ status: form.status, usageData: currentAccount.usageData }, t)

  return createPortal(
    <DialogRoot open={true} onOpenChange={(open) => !open && onClose()}>
      <DialogContent maxWidth="800px" showClose={false}>
        {/* 顶部渐变背景 */}
        <div className="absolute top-0 left-0 right-0 h-40 bg-gradient-to-br from-blue-500/5 via-purple-500/3 to-transparent pointer-events-none rounded-t-2xl" />
        
        <div className="sticky top-0 z-20 bg-background/90 backdrop-blur-md border-b border-border px-6 pt-5 pb-4 rounded-t-2xl">
          <div className="flex items-start gap-3.5">
            <div className={`w-12 h-12 rounded-2xl flex items-center justify-center flex-shrink-0 shadow-lg text-white text-lg font-bold ${
              currentAccount.provider === 'Google'
                ? 'bg-gradient-to-br from-red-500 to-orange-500 shadow-red-500/25'
                : isGitHubProvider(currentAccount.provider)
                  ? 'bg-gradient-to-br from-gray-700 to-gray-900 shadow-black/20'
                  : 'bg-gradient-to-br from-blue-500 to-indigo-600 shadow-blue-500/25'
            }`}>
              {getAccountDisplayName(currentAccount)[0]?.toUpperCase() || <User size={22} />}
            </div>

            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2 mb-1.5 flex-wrap">
                <h2 className="text-[15px] font-semibold text-foreground truncate">
                  {currentAccount.email || getAccountDisplayName(currentAccount)}
                </h2>
                <span className={`px-2 py-0.5 rounded-md text-[11px] font-bold whitespace-nowrap shadow-sm ${
                  currentAccount.usageData?.subscriptionInfo?.subscriptionTitle?.toUpperCase()?.includes('ENTERPRISE')
                    ? 'bg-gradient-to-r from-amber-500 to-orange-500 text-white shadow-amber-500/30'
                    : currentAccount.usageData?.subscriptionInfo?.subscriptionTitle?.includes('PRO+')
                      ? 'bg-gradient-to-r from-purple-500 to-pink-500 text-white shadow-purple-500/30'
                      : currentAccount.usageData?.subscriptionInfo?.subscriptionTitle?.includes('PRO')
                        ? 'bg-gradient-to-r from-blue-500 to-indigo-500 text-white shadow-blue-500/30'
                        : currentAccount.usageData?.subscriptionInfo?.subscriptionTitle?.toUpperCase()?.includes('KIRO')
                          ? 'bg-gradient-to-r from-teal-500 to-cyan-500 text-white shadow-teal-500/30'
                          : 'bg-muted text-muted-foreground'
                }`}>
                  {currentAccount.usageData?.subscriptionInfo?.subscriptionTitle || 'Free'}
                </span>
              </div>

              <div className="flex items-center gap-2 text-xs text-muted-foreground flex-wrap">
                <ProviderBadge provider={currentAccount.provider} />
                <span className="opacity-50">{t('detail.addedAt')} {currentAccount.addedAt?.split(' ')[0]}</span>
                {currentAccount.machineId && (
                  <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-muted/40 border border-border/50">
                    <span className="text-[10px] font-medium text-muted-foreground">Machine ID</span>
                    <code className="text-[10px] font-mono text-red-400 truncate max-w-[180px]">{currentAccount.machineId}</code>
                    <button type="button" onClick={() => handleCopy(currentAccount.machineId || '', 'machineId')}
                      className="p-0.5 rounded hover:bg-muted/60 cursor-pointer transition-colors">
                      {copied === 'machineId' ? <Check size={10} className="text-green-500" /> : <Copy size={10} className="text-muted-foreground" />}
                    </button>
                  </span>
                )}
              </div>
            </div>

            <button onClick={onClose} className="p-2 rounded-xl hover:bg-destructive/10 hover:text-destructive text-muted-foreground transition-colors flex-shrink-0">
              <X size={18} />
            </button>
          </div>
        </div>
        
        {/* Body - 使用 DialogBody 的 noPadding，自己控制每个区域的 padding */}
        <DialogBody noPadding>
          {/* 配额总览 */}
          <div className="border-b border-border px-6 py-5">
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-2">
                <div className="p-1.5 rounded-lg bg-blue-500/10">
                  <CreditCard size={16} className="text-blue-500" />
                </div>
                <span className="text-sm font-semibold text-foreground">{t('detail.quotaOverview')}</span>
              </div>
              <button type="button" onClick={handleRefresh} disabled={refreshing} title={t('detail.syncQuota')}
                className="inline-flex items-center gap-1.5 px-2.5 h-8 rounded-lg text-xs font-medium bg-blue-500/15 text-blue-600 hover:bg-blue-500/25 transition-colors disabled:opacity-50 disabled:cursor-not-allowed">
                <RefreshCw size={13} className={refreshing ? 'animate-spin' : ''} />
                {t('detail.syncQuota')}
              </button>
            </div>

            <div className="rounded-2xl border border-border bg-gradient-to-br from-muted/40 to-muted/10 p-4">
              <div className="flex items-end justify-between mb-3">
                <div className="flex items-baseline gap-2">
                  <span className="text-4xl font-bold text-foreground tracking-tight">{formatUsage(totalUsed)}</span>
                  <span className="text-base text-muted-foreground">/ {formatUsage(totalQuota)}</span>
                </div>
                <div className="flex flex-col items-end gap-1">
                  <span className={`text-sm font-bold px-2.5 py-1 rounded-lg ${
                    totalPercent > 80 ? 'bg-red-500/15 text-red-500'
                    : totalPercent > 50 ? 'bg-yellow-500/15 text-yellow-600'
                    : 'bg-green-500/15 text-green-600'
                  }`}>{totalPercent.toFixed(0)}% {t('detail.used')}</span>
                  <span className="text-[11px] text-muted-foreground">剩余 {formatUsage(Math.max(0, totalQuota - totalUsed))}</span>
                </div>
              </div>
              <div className="h-3 bg-muted rounded-full overflow-hidden shadow-inner">
                <div className={`h-full rounded-full transition-all duration-500 ${
                  totalPercent > 80 ? 'bg-gradient-to-r from-red-400 to-red-500'
                  : totalPercent > 50 ? 'bg-gradient-to-r from-yellow-400 to-orange-500'
                  : 'bg-gradient-to-r from-green-400 to-emerald-500'
                }`} style={{ width: `${totalPercent}%` }} />
              </div>
              <div className="flex items-center flex-wrap gap-2 mt-3">
                {overageEnabled && (
                  <span className="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-purple-500/10 text-purple-600 border border-purple-500/20">
                    <Zap size={11} />基础 {formatUsage(merged.baseQuota)} + 超额 {formatUsage(merged.overageCap)}
                  </span>
                )}
                {currentAccount.usageData?.nextDateReset && (
                  <span className="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-background/60 border border-border/50 text-muted-foreground">
                    <Clock size={11} />{new Date(currentAccount.usageData.nextDateReset * 1000).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hour12: false })}{t('detail.reset')}
                  </span>
                )}
                {freeTrialQuota > 0 && (
                  <span className="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-cyan-500/10 text-cyan-600 border border-cyan-500/20">⏰ {t('detail.freeTrial')} {formatUsage(freeTrialUsed)}/{formatUsage(freeTrialQuota)}</span>
                )}
                {bonusQuota > 0 && (
                  <span className="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-purple-500/10 text-purple-600 border border-purple-500/20">🎁 {t('detail.bonusTotal')} {formatUsage(bonusUsed)}/{formatUsage(bonusQuota)}</span>
                )}
              </div>
            </div>

            {/* Bonuses 列表 */}
            {bonuses.length > 0 && (
              <div className="mt-6 pt-5 border-t border-border">
                <div className="flex items-center gap-2 mb-4">
                  <span className="text-lg">🎁</span>
                  <span className={`text-sm font-medium text-foreground`}>{t('detail.bonusDetails')}</span>
                  <span className={`text-xs px-2 py-0.5 rounded-full info-badge font-medium`}>{bonuses.length}</span>
                </div>
                <div className="space-y-3">
                  {bonuses.map((bonus, idx) => (
                    <div key={idx} className={`flex items-center justify-between p-4 rounded-xl border transition-colors duration-200 hover:shadow-md ${
                      bonus.status === 'ACTIVE' 
                        ? 'bg-purple-500/10 border-purple-500/30' 
                        : `bg-muted/30 border-border`
                    }`}>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-1">
                          <span className={`text-sm font-medium text-foreground`}>{bonus.displayName || bonus.bonusCode}</span>
                          <span className={`text-xs px-2 py-0.5 rounded-md font-medium ${
                            bonus.status === 'ACTIVE' 
                              ? 'bg-green-500/20 text-green-500' 
                              : bonus.status === 'EXHAUSTED' 
                                ? `bg-muted/30 text-muted-foreground` 
                                : 'bg-yellow-500/20 text-yellow-600'
                          }`}>
                            {bonus.status}
                          </span>
                        </div>
                        <div className={`text-xs text-muted-foreground leading-relaxed`}>
                          {bonus.description && <span>{bonus.description} · </span>}
                          {bonus.redeemedAt && <span>{t('detail.redeemed')}: {new Date(bonus.redeemedAt * 1000).toLocaleDateString()} · </span>}
                          {bonus.expiresAt && <span>{t('detail.expires')}: {new Date(bonus.expiresAt * 1000).toLocaleDateString()}</span>}
                        </div>
                      </div>
                      <div className="text-right ml-4 flex-shrink-0">
                        <div className={`text-base font-semibold text-foreground`}>{formatUsage(bonus.currentUsage || 0)} <span className={`text-sm text-muted-foreground font-normal`}>/ {formatUsage(bonus.usageLimit || 0)}</span></div>
                        <div className={`text-xs text-muted-foreground font-mono mt-0.5`}>{bonus.bonusCode}</div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>

          {/* 账号信息 & 订阅与超额 - 并排布局 */}
          <div className="px-6 py-5 border-b border-border">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              {/* 基本信息 */}
              <section className="space-y-3">
                <h3 className="flex items-center gap-2 font-bold text-sm text-foreground">
                  <User size={16} className="text-primary" />
                  {t('detail.basicInfo')}
                </h3>
                <div className="rounded-xl border border-border bg-gradient-to-br from-muted/50 to-muted/15 shadow-sm divide-y divide-border/40 overflow-hidden">
                  {/* 邮箱 */}
                  <div className="flex items-center gap-3 px-3.5 py-2.5 group">
                    <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-blue-500/12 text-blue-500 shrink-0"><Mail size={14} /></span>
                    <div className="min-w-0 flex-1">
                      <div className="text-[10px] font-medium text-muted-foreground">{t('detail.emailAddress')}</div>
                      <div className="text-sm font-mono text-foreground truncate select-all">{currentAccount.email || getAccountDisplayName(currentAccount)}</div>
                    </div>
                    <button onClick={() => handleCopy(currentAccount.email || '', 'email')} title="复制"
                      className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground hover:text-primary opacity-0 group-hover:opacity-100 transition-all shrink-0 cursor-pointer">
                      {copied === 'email' ? <Check size={13} className="text-green-500" /> : <Copy size={13} />}
                    </button>
                  </div>
                  {/* 备注标签 */}
                  <div className="flex items-center gap-3 px-3.5 py-2.5">
                    <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-amber-500/12 text-amber-500 shrink-0"><Tag size={14} /></span>
                    <div className="min-w-0 flex-1">
                      <div className="text-[10px] font-medium text-muted-foreground">{t('detail.remarkLabel')}</div>
                      <div className="text-sm font-medium text-foreground truncate">{currentAccount.label || '-'}</div>
                    </div>
                  </div>
                  {/* Provider */}
                  <div className="flex items-center gap-3 px-3.5 py-2.5">
                    <span className={`flex h-8 w-8 items-center justify-center rounded-lg shrink-0 ${
                      currentAccount.provider === 'Google' ? 'bg-red-500/12 text-red-500'
                      : isGitHubProvider(currentAccount.provider) ? 'bg-slate-500/12 text-slate-400'
                      : 'bg-primary/12 text-primary'
                    }`}><User size={14} /></span>
                    <div className="min-w-0 flex-1">
                      <div className="text-[10px] font-medium text-muted-foreground">Provider</div>
                      <div className="text-sm font-medium text-foreground">{getProviderDisplayName(currentAccount.provider) || '-'}</div>
                    </div>
                  </div>
                  {/* User ID */}
                  <div className="flex items-center gap-3 px-3.5 py-2.5 group">
                    <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-500/12 text-violet-500 shrink-0"><Hash size={14} /></span>
                    <div className="min-w-0 flex-1">
                      <div className="text-[10px] font-medium text-muted-foreground">User ID</div>
                      <div className="text-xs font-mono text-foreground truncate select-all" title={currentAccount.usageData?.userInfo?.userId || '-'}>{currentAccount.usageData?.userInfo?.userId || '-'}</div>
                    </div>
                    <button onClick={() => handleCopy(currentAccount.usageData?.userInfo?.userId || '', 'userId')} title="复制"
                      className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground hover:text-primary opacity-0 group-hover:opacity-100 transition-all shrink-0 cursor-pointer">
                      {copied === 'userId' ? <Check size={13} className="text-green-500" /> : <Copy size={13} />}
                    </button>
                  </div>
                </div>
              </section>

              {/* 订阅与超额 */}
              <section className="space-y-3">
                <h3 className="flex items-center gap-2 font-bold text-sm text-foreground">
                  <Shield size={16} className="text-primary" />
                  订阅与超额
                </h3>
                <div className="bg-muted/60 border border-border rounded-xl p-4 text-sm space-y-0.5 shadow-sm">
                  <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                    <span className="text-muted-foreground text-xs">订阅类型</span>
                    {(() => {
                      const st = currentAccount.usageData?.subscriptionInfo?.type || ''
                      const tone = st.includes('ENTERPRISE') ? 'bg-amber-500/12 text-amber-600'
                        : st.includes('PRO+') ? 'bg-purple-500/12 text-purple-600'
                        : st.includes('POWER') ? 'bg-violet-500/12 text-violet-600'
                        : st.includes('PRO') ? 'bg-blue-500/12 text-blue-600'
                        : 'bg-muted text-muted-foreground'
                      return <span className={`font-mono text-xs font-semibold px-1.5 py-0.5 rounded ${tone}`}>{st || '-'}</span>
                    })()}
                  </div>
                  <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                    <span className="text-muted-foreground text-xs">Region</span>
                    <span className="font-mono text-xs px-1.5 py-0.5 bg-muted rounded-md">us-east-1</span>
                  </div>
                  <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                    <span className="text-muted-foreground text-xs">Token 到期</span>
                    <span className="font-medium text-xs">{currentAccount.expiresAt || '-'}</span>
                  </div>
                  <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                    <span className="text-muted-foreground text-xs">资源类型</span>
                    <span className="font-mono text-xs font-medium">{breakdown?.resourceType || '-'}</span>
                  </div>
                  <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                    <span className="text-muted-foreground text-xs">可升级</span>
                    <span className={`text-[11px] font-semibold px-2 py-0.5 rounded-full ${currentAccount.usageData?.subscriptionInfo?.upgradeCapability === 'UPGRADE_CAPABLE' ? 'bg-green-500/15 text-green-600' : 'bg-muted text-muted-foreground'}`}>
                      {currentAccount.usageData?.subscriptionInfo?.upgradeCapability === 'UPGRADE_CAPABLE' ? '✓ YES' : 'NO'}
                    </span>
                  </div>
                  <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                    <span className="text-muted-foreground text-xs">超额能力</span>
                    <span className={`text-[11px] font-semibold px-2 py-0.5 rounded-full ${currentAccount.usageData?.subscriptionInfo?.overageCapability === 'OVERAGE_CAPABLE' ? 'bg-green-500/15 text-green-600' : 'bg-muted text-muted-foreground'}`}>
                      {currentAccount.usageData?.subscriptionInfo?.overageCapability === 'OVERAGE_CAPABLE' ? '✓ 支持' : '不支持'}
                    </span>
                  </div>
                  {currentAccount.usageData?.subscriptionInfo?.overageCapability === 'OVERAGE_CAPABLE' && (
                    <>
                      <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                        <span className="text-muted-foreground text-xs">超额状态</span>
                        <span className={`text-[11px] font-semibold px-2 py-0.5 rounded-full ${currentAccount.usageData?.overageConfiguration?.overageStatus === 'ENABLED' ? 'bg-green-500/15 text-green-600' : 'bg-muted text-muted-foreground'}`}>
                          {currentAccount.usageData?.overageConfiguration?.overageStatus === 'ENABLED' ? '✓ 已开启' : '已关闭'}
                        </span>
                      </div>
                      {breakdown?.overageRate != null && (
                        <>
                          <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                            <span className="text-muted-foreground text-xs">超额费率</span>
                            <span className="font-mono text-xs font-semibold">
                              {breakdown.currency === 'USD' ? '$' : breakdown.currency}{breakdown.overageRate}/Credit
                            </span>
                          </div>
                          <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                            <span className="text-muted-foreground text-xs">超额上限</span>
                            <span className="font-mono text-xs font-semibold">
                              {breakdown.currency === 'USD' ? '$' : breakdown.currency}{breakdown.overageCap}
                            </span>
                          </div>
                          <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors border-b border-border/30">
                            <span className="text-muted-foreground text-xs">当前超额</span>
                            <span className={`font-mono text-sm font-bold ${breakdown.currentOverages > 0 ? 'text-orange-500' : 'text-foreground'}`}>
                              {formatUsage(breakdown.currentOverages || 0)}
                            </span>
                          </div>
                          <div className="flex justify-between items-center py-1.5 px-2 -mx-2 rounded-lg hover:bg-muted/40 transition-colors">
                            <span className="text-muted-foreground text-xs">超额费用</span>
                            <span className={`font-mono text-sm font-bold ${breakdown.overageCharges > 0 ? 'text-orange-500' : 'text-foreground'}`}>
                              {breakdown.currency === 'USD' ? '$' : breakdown.currency}{breakdown.overageCharges?.toFixed(2) || '0.00'}
                            </span>
                          </div>
                        </>
                      )}
                    </>
                  )}
                </div>
              </section>
            </div>
          </div>

          {/* 账户可用模型 */}
          <div className={`px-6 py-5`}>
            <div 
              className="flex items-center gap-2 cursor-pointer select-none"
              onClick={() => setModelsExpanded(!modelsExpanded)}
            >
              <div className="p-1.5 rounded-lg bg-violet-500/10">
                <Cpu size={16} className="text-violet-500" />
              </div>
              <span className={`text-sm font-semibold text-foreground`}>{t('detail.availableModels')}</span>
              <span className={`ml-auto text-xs px-2 py-0.5 rounded-full bg-primary/10 text-primary border border-primary/20 font-medium`}>
                {models.length}
              </span>
              <button
                onClick={(e) => { e.stopPropagation(); fetchModels(true) }}
                disabled={modelsLoading}
                className="p-1.5 rounded-lg hover:bg-muted/50 transition-colors disabled:opacity-50"
                title="强制刷新模型列表"
              >
                <RefreshCw size={14} className={modelsLoading ? "animate-spin text-muted-foreground" : "text-muted-foreground"} />
              </button>
              <ChevronDown size={16} className={`text-muted-foreground transition-transform duration-200 ${modelsExpanded ? '' : '-rotate-90'}`} />
            </div>
            {modelsExpanded && (
            <div className="bg-gradient-to-br from-muted/20 to-muted/40 border rounded-xl p-4 mt-4">
              {modelsLoading ? (
                <div className="flex items-center justify-center py-8 text-muted-foreground">
                  <Loader2 size={20} className="animate-spin mr-2" />
                  <span className="text-sm">{t('detail.loadingModels')}</span>
                </div>
              ) : modelsError ? (
                <div className="text-center py-8">
                  <p className="text-red-500 text-sm">{modelsError}</p>
                </div>
              ) : models.length === 0 ? (
                <div className="text-center py-8 text-muted-foreground text-sm">
                  {t('detail.noModels')}
                </div>
              ) : (
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5 max-h-[320px] overflow-y-auto no-scrollbar">
                  {models.map((model, index) => {
                    const norm = (s: string) => (s || '').toLowerCase().replace(/[\s_-]/g, '')
                    const showName = model.modelName && norm(model.modelName) !== norm(model.modelId)
                    const isDefault = index === 0
                    const fmt = (n: number) => n >= 1000000 ? `${(n / 1000000).toFixed(0)}M` : `${(n / 1000).toFixed(0)}K`
                    return (
                      <div key={model.modelId}
                        className={`group relative flex flex-col gap-2 p-3 rounded-xl border bg-gradient-to-br from-background to-muted/20 shadow-sm hover:shadow-md hover:-translate-y-0.5 transition-all duration-200 ${
                          isDefault ? 'border-primary/40 ring-1 ring-primary/20' : 'border-border/60 hover:border-primary/30'
                        }`}>
                        <div className="flex items-start gap-2">
                          <span className={`mt-1 w-2 h-2 rounded-full shrink-0 ${isDefault ? 'bg-primary animate-pulse' : 'bg-muted-foreground/40'}`} />
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-1.5">
                              <button type="button" onClick={() => handleCopy(model.modelId, `model:${model.modelId}`)} title="点击复制模型名称"
                                className="group/copy inline-flex items-center gap-1 min-w-0 cursor-pointer">
                                <code className="text-[13px] font-bold text-primary truncate group-hover/copy:underline">{model.modelId.charAt(0).toUpperCase() + model.modelId.slice(1)}</code>
                                {copied === `model:${model.modelId}`
                                  ? <Check size={11} className="text-green-500 shrink-0" />
                                  : <Copy size={11} className="text-muted-foreground shrink-0 opacity-0 group-hover/copy:opacity-100 transition-opacity" />}
                              </button>
                              {isDefault && <span className="text-[9px] font-bold px-1.5 py-px rounded-full bg-primary/15 text-primary shrink-0">默认</span>}
                            </div>
                            {showName && <p className="text-[11px] text-primary/80 font-medium truncate mt-0.5">{model.modelName}</p>}
                            <p className="text-[11px] text-muted-foreground line-clamp-2 leading-relaxed mt-0.5">{model.description || t('detail.noDescription')}</p>
                          </div>
                          {model.rateMultiplier !== undefined && (
                            <span className="shrink-0 inline-flex items-center gap-0.5 text-[10px] font-bold px-1.5 h-5 rounded-md bg-amber-500/12 text-amber-600">
                              <Zap size={11} />{model.rateMultiplier}x
                            </span>
                          )}
                        </div>
                        <div className="flex items-center justify-between pt-2 border-t border-border/50">
                          <div className="flex items-center gap-1.5">
                            {model.supportedInputTypes?.includes('TEXT') && (
                              <span className="text-[10px] px-1.5 h-5 bg-blue-500/10 text-blue-600 rounded inline-flex items-center gap-0.5 font-medium"><FileText size={11} />Text</span>
                            )}
                            {model.supportedInputTypes?.includes('IMAGE') && (
                              <span className="text-[10px] px-1.5 h-5 bg-purple-500/10 text-purple-600 rounded inline-flex items-center gap-0.5 font-medium"><ImageIcon size={11} />Image</span>
                            )}
                          </div>
                          <div className="flex items-center gap-1 text-[10px] text-muted-foreground font-mono">
                            <Hash size={11} />
                            <span className="text-green-600">{model.tokenLimits?.maxInputTokens ? fmt(model.tokenLimits.maxInputTokens) : '-'}</span>
                            <span className="opacity-50">/</span>
                            <span className="text-orange-600">{model.tokenLimits?.maxOutputTokens ? fmt(model.tokenLimits.maxOutputTokens) : '-'}</span>
                          </div>
                        </div>
                      </div>
                    )
                  })}
                </div>
              )}
            </div>
            )}
          </div>
        </DialogBody>

        {/* 状态栏（底部简洁显示） */}
        <div className="px-6 py-3.5 border-t border-border flex items-center justify-between bg-gradient-to-r from-muted/30 via-transparent to-transparent">
          <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold ${
            statusMeta.tone === 'success' ? 'bg-green-500/12 text-green-500'
            : statusMeta.tone === 'danger' ? 'bg-red-500/12 text-red-500'
            : 'bg-orange-500/12 text-orange-500'
          }`}>
            <span className="relative flex h-2.5 w-2.5">
              <span className={`absolute inline-flex h-full w-full rounded-full opacity-60 animate-ping ${
                statusMeta.tone === 'success' ? 'bg-green-500' : statusMeta.tone === 'danger' ? 'bg-red-500' : 'bg-orange-500'
              }`} />
              <span className={`relative inline-flex h-2.5 w-2.5 rounded-full ${
                statusMeta.tone === 'success' ? 'bg-green-500' : statusMeta.tone === 'danger' ? 'bg-red-500' : 'bg-orange-500'
              }`} />
            </span>
            {statusMeta.label}
          </span>
          <ProviderBadge provider={currentAccount.provider} />
        </div>
      </DialogContent>
    </DialogRoot>,
    document.body
  )
}

export default AccountDetailModal

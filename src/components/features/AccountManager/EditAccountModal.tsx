import { useState, useEffect, useMemo, useRef } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { Copy, Check, Folder, Plus, X, RefreshCw, Loader2, CheckCircle, FileText, KeyRound, Cpu, Save, Mail } from 'lucide-react'
import { useApp } from '../../../hooks/useApp'
import { useDialog } from '../../../contexts/DialogContext'
import { setAccountTags, setAccountGroup, getGroups, addGroup } from '../../../api/groupTag'
import { getAccountDisplayName } from '../../../utils/accountStats'
import { TagSelector } from './GroupTagManager'
import {
  DialogRoot,
  DialogTitle,
  DialogDescription} from '../../shared/dialog'
import { getThemeAccent } from '../KiroConfig/themeAccent'
import { Account, GroupDefinition } from '../../../types/account'

const PRESET_COLORS = [
  '#3b82f6', '#10b981', '#f59e0b', '#ef4444', 
  '#8b5cf6', '#ec4899', '#06b6d4', '#84cc16'
]

interface GroupSelectorProps {
  groups: GroupDefinition[];
  value: string;
  onChange: (value: string) => void;
  onGroupsChange: (groups: GroupDefinition[]) => void;
}

function GroupSelector({ groups, value, onChange, onGroupsChange }: GroupSelectorProps) {
  const { t, theme } = useApp()
  const accent = useMemo(() => getThemeAccent(theme), [theme])
  const colors = useMemo(() => ({
    inputFocus: 'focus:ring-primary/20 focus:border-primary'
  }), [])

  const [newGroupName, setNewGroupName] = useState('')
  const [showInput, setShowInput] = useState(false)

  const handleAddGroup = async () => {
    const trimmed = newGroupName.trim().slice(0, 20)
    if (!trimmed) return
    if (groups.some(g => g.name === trimmed)) {
      setNewGroupName('')
      return
    }
    const color = PRESET_COLORS[Math.floor(Math.random() * PRESET_COLORS.length)]
    try {
      const newGroup = await addGroup(trimmed, color) as GroupDefinition
      onGroupsChange([...groups, newGroup])
      onChange(newGroup.id)
      setNewGroupName('')
      setShowInput(false)
    } catch (e) {
      console.error('创建分组失败:', e)
    }
  }

  if (showInput) {
    return (
      <div className="flex gap-2">
        <input
          type="text"
          value={newGroupName}
          onChange={(e) => setNewGroupName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleAddGroup()}
          placeholder={t('groups.newGroupPlaceholder') || '输入新分组名...'}
          className={`flex-1 px-4 py-2.5 border rounded-xl text-foreground bg-background border-input ${colors.inputFocus} focus:ring-2 outline-none`}
        />
        <button
          onClick={handleAddGroup}
          disabled={!newGroupName.trim()}
          className={`p-2.5 ${accent.solidBg} text-white rounded-xl ${accent.solidHoverBg} disabled:opacity-50 cursor-pointer`}
        >
          <Check size={16} />
        </button>
        <button
          onClick={() => { setShowInput(false); setNewGroupName('') }}
          className={`p-2.5 rounded-xl hover:bg-muted/50 cursor-pointer`}
        >
          <X size={16} />
        </button>
      </div>
    )
  }

  return (
    <div className="flex gap-2">
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className={`flex-1 px-4 py-2.5 border rounded-xl text-foreground bg-background border-input ${colors.inputFocus} focus:ring-2 outline-none`}
      >
        <option value="">{t('groups.noGroup') || '无分组'}</option>
        {groups.map(g => (
          <option key={g.id} value={g.id}>{g.name}</option>
        ))}
      </select>
      <button
        onClick={() => setShowInput(true)}
        className={`p-2.5 ${accent.solidBg} text-white rounded-xl ${accent.solidHoverBg} cursor-pointer`}
      >
        <Plus size={16} />
      </button>
    </div>
  )
}

interface EditAccountModalProps {
  account: Account;
  onClose: () => void;
  onSuccess?: (account: Account) => void;
}

interface VerifyAccountResponse {
  usageData: any;
  accessToken: string;
  refreshToken: string;
}

function EditAccountModal({ account, onClose, onSuccess }: EditAccountModalProps) {
  const { t, theme } = useApp()
  const { showError } = useDialog()
  const accent = useMemo(() => getThemeAccent(theme), [theme])
  const colors = useMemo(() => ({
    inputFocus: 'focus:ring-primary/20 focus:border-primary'
  }), [])

  const isIdCAccount = account.provider === 'BuilderId' || account.provider === 'Enterprise'
  const accountDisplayName = useMemo(() => getAccountDisplayName(account), [account])

  const [form, setForm] = useState({
    label: account.label || '',
    accessToken: account.accessToken || '',
    refreshToken: account.refreshToken || '',
    clientId: account.clientId || '',
    clientSecret: account.clientSecret || '',
    machineId: account.machineId || ''})

  const [selectedTagIds, setSelectedTagIds] = useState((account.tagLinks || []).map(link => link.tagId))
  const [selectedGroupId, setSelectedGroupId] = useState(account.groupId || '')
  const [groups, setGroups] = useState<GroupDefinition[]>([])
  const [saving, setSaving] = useState(false)
  const [verifying, setVerifying] = useState(false)
  const [copiedField, setCopiedField] = useState<string | null>(null)
  const mountedRef = useRef(true)
  const copiedTimerRef = useRef<NodeJS.Timeout | null>(null)
  
  // 账号信息状态（验证后更新）
  const [accountInfo, setAccountInfo] = useState<{
    email: string;
    subscriptionType: string;
    usage: { current: number; limit: number };
    daysRemaining?: number;
  } | null>(null)

  useEffect(() => {
    let cancelled = false
    getGroups()
      .then(groupData => {
        if (!cancelled && mountedRef.current) {
          setGroups(groupData)
        }
      })
      .catch((err) => {
        console.error('加载编辑账号分组失败:', err)
      })
    
    // 初始化账号信息
    if (account.usageData) {
      const usageData = account.usageData
      const userInfo = usageData.userInfo || {}
      const subscriptionInfo = usageData.subscriptionInfo
      const breakdown = usageData.usageBreakdownList?.[0]
      const nextReset = usageData.nextDateReset
      
      // 计算剩余天数
      let daysRemaining: number | undefined
      if (nextReset) {
        const resetDate = new Date(typeof nextReset === 'string' ? nextReset : (nextReset < 1e12 ? nextReset * 1000 : nextReset))
        daysRemaining = Math.max(0, Math.ceil((resetDate.getTime() - Date.now()) / (1000 * 60 * 60 * 24)))
      }
      
      if (!cancelled && mountedRef.current) setAccountInfo({
        email: account.email || userInfo.email || '',
        subscriptionType: subscriptionInfo?.subscriptionTitle || subscriptionInfo?.type || 'Free',
        usage: {
          current: breakdown?.currentUsage ?? 0,
          limit: breakdown?.usageLimit ?? 0
        },
        daysRemaining
      })
    }

    return () => {
      cancelled = true
    }
  }, [account])

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      if (copiedTimerRef.current) {
        clearTimeout(copiedTimerRef.current)
      }
    }
  }, [])

  const handleCopy = async (text: string, field: string) => {
    try {
      await navigator.clipboard.writeText(text)
      if (!mountedRef.current) return
      if (copiedTimerRef.current) {
        clearTimeout(copiedTimerRef.current)
      }
      setCopiedField(field)
      copiedTimerRef.current = setTimeout(() => {
        if (!mountedRef.current) return
        setCopiedField(null)
        copiedTimerRef.current = null
      }, 2000)
    } catch (e) {
      console.error('复制失败:', e)
    }
  }

  const handleVerifyAndRefresh = async () => {
    if (!form.refreshToken) {
      await showError(t('editAccount.verifyFailed'), '请填写 Refresh Token')
      return
    }
    if (isIdCAccount && (!form.clientId || !form.clientSecret)) {
      await showError(t('editAccount.verifyFailed'), '请填写 Client ID 和 Client Secret')
      return
    }

    setVerifying(true)
    try {
      const result = await invoke<VerifyAccountResponse>('verify_account', {
        params: {
          accessToken: form.accessToken,
          refreshToken: form.refreshToken,
          provider: account.provider,
          clientId: isIdCAccount ? form.clientId : null,
          clientSecret: isIdCAccount ? form.clientSecret : null,
          region: null
        }
      })

      if (!mountedRef.current) return

      // 更新表单中的 token
      setForm(prev => ({
        ...prev,
        accessToken: result.accessToken,
        refreshToken: result.refreshToken
      }))

      // 更新账号信息显示
      const usageData = result.usageData
      const userInfo = usageData.userInfo || {}
      const subscriptionInfo = usageData.subscriptionInfo
      const verifyBreakdown = usageData.usageBreakdownList?.[0]
      const verifyNextReset = usageData.nextDateReset
      
      let verifyDaysRemaining: number | undefined
      if (verifyNextReset) {
        const resetDate = new Date(typeof verifyNextReset === 'string' ? verifyNextReset : (verifyNextReset < 1e12 ? verifyNextReset * 1000 : verifyNextReset))
        verifyDaysRemaining = Math.max(0, Math.ceil((resetDate.getTime() - Date.now()) / (1000 * 60 * 60 * 24)))
      }
      
      setAccountInfo({
        email: userInfo.email || '',
        subscriptionType: subscriptionInfo?.subscriptionTitle || subscriptionInfo?.type || 'Free',
        usage: {
          current: verifyBreakdown?.currentUsage ?? 0,
          limit: verifyBreakdown?.usageLimit ?? 0
        },
        daysRemaining: verifyDaysRemaining
      })
    } catch (e) {
      if (mountedRef.current) {
        await showError(t('editAccount.verifyFailed'), String(e))
      }
    } finally {
      if (mountedRef.current) {
        setVerifying(false)
      }
    }
  }

  const handleSave = async () => {
    setSaving(true)
    try {
      const params: any = {
        id: account.id,
        label: form.label || null,
        accessToken: form.accessToken || null,
        refreshToken: form.refreshToken || null,
        machineId: form.machineId || null}
      if (isIdCAccount) {
        params.clientId = form.clientId || null
        params.clientSecret = form.clientSecret || null
      }
      const updatedAccount = await invoke<Account>('update_account', { params })
      await setAccountGroup(account.id, selectedGroupId || null)
      await setAccountTags(account.id, selectedTagIds)
      if (!mountedRef.current) return
      onSuccess?.(updatedAccount)
      onClose()
    } catch (e) {
      if (mountedRef.current) {
        await showError(t('editAccount.saveFailed'), String(e))
      }
    } finally {
      if (mountedRef.current) {
        setSaving(false)
      }
    }
  }

  const renderAccountStatus = () => {
    if (!accountInfo) return null;
    const usagePercent = accountInfo.usage.limit > 0 ? Math.round((accountInfo.usage.current / accountInfo.usage.limit) * 100) : 0;
    return (
      <div className={`p-5 rounded-2xl border ${accent.borderSoft} bg-gradient-to-b ${accent.bgSoft || 'from-primary/5'} to-transparent space-y-4 h-full flex flex-col justify-between`}>
        <div className="space-y-4">
          <div className="flex items-center gap-3">
            <div className={`w-11 h-11 rounded-2xl flex items-center justify-center shadow-inner ${accent.bg || 'bg-primary/10'}`}><CheckCircle className={`w-5.5 h-5.5 animate-pulse ${accent.text || 'text-primary'}`} /></div>
            <div className="min-w-0 flex-1">
              <span className="text-[11px] font-semibold text-muted-foreground uppercase tracking-wider block">当前状态</span>
              <span className="font-semibold text-foreground/90 text-sm truncate block" title={accountInfo.email}>{accountInfo.email}</span>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div className="p-3 rounded-xl bg-background/50 border border-border/40 backdrop-blur-sm">
              <span className="text-[11px] text-muted-foreground block mb-0.5">订阅计划</span>
              <span className="font-bold text-foreground text-xs">{accountInfo.subscriptionType}</span>
            </div>
            <div className="p-3 rounded-xl bg-background/50 border border-border/40 backdrop-blur-sm">
              <span className="text-[11px] text-muted-foreground block mb-0.5">剩余有效期</span>
              <span className="font-bold text-foreground text-xs">{accountInfo.daysRemaining ?? '-'} 天</span>
            </div>
          </div>
          <div className="p-3.5 rounded-xl bg-background/60 border border-border/50 space-y-1.5">
            <div className="flex justify-between items-center text-xs"><span className="text-muted-foreground text-[11px]">配额使用率</span><span className="font-mono font-semibold text-foreground">{usagePercent}%</span></div>
            <div className="h-1.5 w-full bg-muted rounded-full overflow-hidden">
              <div className={`h-full rounded-full bg-gradient-to-r ${accent.gradientFrom || 'from-blue-500'} ${accent.gradientTo || 'to-indigo-500'} transition-all duration-500`} style={{ width: `${Math.min(100, usagePercent)}%` }} />
            </div>
            <div className="flex justify-between text-[10px] text-muted-foreground font-mono"><span>{accountInfo.usage.current.toLocaleString()}</span><span>{accountInfo.usage.limit.toLocaleString()}</span></div>
          </div>
        </div>
        <button onClick={handleVerifyAndRefresh} disabled={verifying || !form.refreshToken || (isIdCAccount && (!form.clientId || !form.clientSecret))} className={`w-full py-2.5 px-4 rounded-xl font-medium text-xs flex items-center justify-center gap-2 cursor-pointer transition-all duration-200 hover:-translate-y-px active:translate-y-0 disabled:opacity-50 disabled:cursor-not-allowed border ${accent.solidBg || 'bg-primary'} text-white shadow-sm hover:opacity-90 active:scale-95`}>
          {verifying ? ( <><Loader2 size={13} className="animate-spin" />正在同步...</> ) : ( <><RefreshCw size={13} className="transition-transform hover:rotate-180 duration-500" />同步验证凭证 & 额度</> )}
        </button>
      </div>
    );
  };

  const renderFormFields = () => {
    return (
      <div className="space-y-4">
        {/* 备注 */}
        <div className="p-3.5 rounded-xl border border-border bg-card/45 space-y-1.5 transition-all hover:border-primary/20">
          <label className="text-xs font-semibold text-muted-foreground uppercase flex items-center gap-1.5"><FileText size={13} />{t('accounts.remark')}</label>
          <input type="text" placeholder={t('editAccount.labelPlaceholder')} value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} className={`w-full px-3 py-2 border rounded-xl text-sm text-foreground bg-background border-input focus:ring-primary/20 focus:border-primary focus:ring-2 outline-none transition-all`} />
        </div>
        {/* Refresh Token */}
        <div className="p-3.5 rounded-xl border border-border bg-card/45 space-y-1.5 transition-all hover:border-primary/20">
          <label className="text-xs font-semibold text-muted-foreground uppercase flex items-center gap-1.5"><KeyRound size={13} />Refresh Token {isIdCAccount && <span className="text-destructive">*</span>}</label>
          <div className="relative">
            <textarea placeholder="aorAAAAA..." value={form.refreshToken} onChange={(e) => setForm({ ...form, refreshToken: e.target.value })} rows={2} className={`w-full px-3 py-2 pr-10 border rounded-xl text-sm text-foreground bg-background border-input focus:ring-primary/20 focus:border-primary focus:ring-2 resize-none outline-none font-mono transition-all no-scrollbar`} />
            <button onClick={() => handleCopy(form.refreshToken, 'refreshToken')} className="absolute right-2.5 top-2.5 p-1.5 rounded-lg hover:bg-muted/50 cursor-pointer" title={copiedField === 'refreshToken' ? '已复制' : '复制'}>
              {copiedField === 'refreshToken' ? <Check size={13} className="text-green-500" /> : <Copy size={13} className="text-muted-foreground" />}
            </button>
          </div>
        </div>
        {/* 机器码 */}
        <div className="p-3.5 rounded-xl border border-border bg-card/45 space-y-1.5 transition-all hover:border-primary/20">
          <label className="text-xs font-semibold text-muted-foreground uppercase flex items-center gap-1.5"><Cpu size={13} />{t('addAccount.machineId')}</label>
          <div className="relative">
            <input type="text" placeholder={t('addAccount.machineIdPlaceholder')} value={form.machineId} onChange={(e) => setForm({ ...form, machineId: e.target.value })} className={`w-full px-3 py-2 pr-10 border rounded-xl text-sm text-foreground bg-background border-input focus:ring-primary/20 focus:border-primary focus:ring-2 outline-none transition-all`} />
            <button onClick={() => handleCopy(form.machineId, 'machineId')} className="absolute right-2.5 top-1/2 -translate-y-1/2 p-1.5 rounded-lg hover:bg-muted/50 cursor-pointer" title={copiedField === 'machineId' ? '已复制' : '复制'}>
              {copiedField === 'machineId' ? <Check size={13} className="text-green-500" /> : <Copy size={13} className="text-muted-foreground" />}
            </button>
          </div>
        </div>
        {/* IDC 专属字段 */}
        {isIdCAccount && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 p-3.5 rounded-xl border border-border bg-card/45">
            <div className="space-y-1.5">
              <label className="text-xs font-semibold text-muted-foreground uppercase block">Client ID <span className="text-destructive">*</span></label>
              <div className="relative">
                <input type="text" placeholder="刷新 Token" value={form.clientId} onChange={(e) => setForm({ ...form, clientId: e.target.value })} className={`w-full px-3 py-2 pr-10 border rounded-xl text-sm text-foreground bg-background border-input focus:ring-primary/20 focus:border-primary focus:ring-2 outline-none font-mono transition-all`} />
                <button onClick={() => handleCopy(form.clientId, 'clientId')} className="absolute right-2.5 top-1/2 -translate-y-1/2 p-1.5 rounded-lg hover:bg-muted/50 cursor-pointer" title={copiedField === 'clientId' ? '已复制' : '复制'}>
                  {copiedField === 'clientId' ? <Check size={13} className="text-green-500" /> : <Copy size={13} className="text-muted-foreground" />}
                </button>
              </div>
            </div>
            <div className="space-y-1.5">
              <label className="text-xs font-semibold text-muted-foreground uppercase block">Client Secret <span className="text-destructive">*</span></label>
              <div className="relative">
                <input type="text" placeholder="刷新 Secret" value={form.clientSecret} onChange={(e) => setForm({ ...form, clientSecret: e.target.value })} className={`w-full px-3 py-2 pr-10 border rounded-xl text-sm text-foreground bg-background border-input focus:ring-primary/20 focus:border-primary focus:ring-2 outline-none font-mono transition-all`} />
                <button onClick={() => handleCopy(form.clientSecret, 'clientSecret')} className="absolute right-2.5 top-1/2 -translate-y-1/2 p-1.5 rounded-lg hover:bg-muted/50 cursor-pointer" title={copiedField === 'clientSecret' ? '已复制' : '复制'}>
                  {copiedField === 'clientSecret' ? <Check size={13} className="text-green-500" /> : <Copy size={13} className="text-muted-foreground" />}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    );
  };

  const renderGroupAndTagSelectors = () => {
    return (
      <div className="space-y-4 pt-4 border-t border-border/40">
        {/* 分组 */}
        <div className="space-y-2">
          <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
            <Folder size={14} />
            {t('groups.title') || '分组'}
          </div>
          <GroupSelector
            groups={groups}
            value={selectedGroupId}
            onChange={setSelectedGroupId}
            onGroupsChange={setGroups}
          />
        </div>

        {/* 标签 */}
        <div className="pt-2">
          <TagSelector
            selectedTagIds={selectedTagIds}
            onChange={setSelectedTagIds}
          />
        </div>
      </div>
    );
  };

  const dialogContent = (
    <DialogRoot open={true} onOpenChange={(open) => !open && onClose()}>
      <div className="fixed inset-0 z-50 flex items-center justify-center">
        <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={onClose} />

        <div className="relative w-full max-w-4xl max-h-[90vh] overflow-hidden bg-background rounded-2xl shadow-2xl z-10 animate-in zoom-in-95 duration-200 flex flex-col">
          {/* Sticky Header */}
          <div className="sticky top-0 overflow-hidden bg-background/95 backdrop-blur-sm z-20 border-b border-border">
            <div className={`absolute inset-x-0 top-0 h-0.5 bg-gradient-to-r ${accent.gradientFrom} ${accent.gradientTo}`} />
            <div className="px-6 py-4 pr-14">
              <div className="flex items-center gap-3">
                <div className={`w-11 h-11 rounded-2xl flex-shrink-0 flex items-center justify-center shadow-md ring-1 ring-primary/15 ${accent.iconBadgeBg || 'bg-primary/10'}`}>
                  <Folder size={20} className={accent.text || 'text-primary'} />
                </div>
                <div className="flex flex-col gap-1.5 min-w-0">
                  <DialogTitle>{t('editAccount.title')}</DialogTitle>
                  <DialogDescription className="mt-0">
                    <span
                      className="inline-flex max-w-full items-center gap-1.5 rounded-full border border-primary/20 bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary"
                      title={accountDisplayName}
                    >
                      <Mail size={12} className="shrink-0" />
                      <span className="truncate">{accountDisplayName}</span>
                    </span>
                  </DialogDescription>
                </div>
              </div>
            </div>
            <button
              onClick={onClose}
              className="absolute top-4 right-4 p-2 rounded-lg hover:bg-muted/50 transition-colors cursor-pointer"
              aria-label="Close"
            >
              <X size={20} className="text-muted-foreground" />
            </button>
          </div>

          {/* Scrollable Body - Two Column Layout */}
          <div className="flex-1 overflow-y-auto flex flex-col md:flex-row divide-y md:divide-y-0 md:divide-x divide-border/50 min-h-0">
            {/* Left Column: Account Profile & Quotas */}
            {accountInfo ? (
              <div className="w-full md:w-5/12 p-6 flex flex-col bg-muted/5">
                {renderAccountStatus()}
              </div>
            ) : (
              <div className="w-full md:w-5/12 p-6 flex flex-col items-center justify-center text-muted-foreground text-sm bg-muted/5">
                暂无账号状态数据，请在右侧填写并同步
              </div>
            )}

            {/* Right Column: Fields & Settings */}
            <div className="w-full md:w-7/12 p-6 overflow-y-auto space-y-6">
              {renderFormFields()}
              {renderGroupAndTagSelectors()}
            </div>
          </div>

          {/* Sticky Footer */}
          <div className="sticky bottom-0 bg-background/95 backdrop-blur-sm p-4 border-t border-border flex justify-end gap-3 z-20">
            <button
              onClick={onClose}
              className="px-5 h-10 font-medium text-sm rounded-xl border border-input bg-background hover:bg-muted/50 text-foreground cursor-pointer transition-all duration-200 active:scale-95 flex items-center gap-1.5"
            >
              <X size={14} />
              {t('common.cancel')}
            </button>
            <button
              onClick={handleSave}
              disabled={saving}
              className="px-5 h-10 font-medium text-sm rounded-xl text-white bg-gradient-to-r from-emerald-500 via-teal-500 to-emerald-600 shadow-md shadow-emerald-500/20 hover:shadow-lg hover:shadow-emerald-500/30 active:scale-95 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer transition-all duration-200 flex items-center gap-1.5"
            >
              {saving ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <Save size={14} />
              )}
              {t('common.save')}
            </button>
          </div>
        </div>
      </div>
    </DialogRoot>
  )

  return createPortal(dialogContent, document.body)
}

export default EditAccountModal

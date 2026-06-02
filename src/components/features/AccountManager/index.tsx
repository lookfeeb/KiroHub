import { useState, useCallback, useMemo, useEffect, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Upload } from 'lucide-react'
import { useApp } from '../../../hooks/useApp'
import { useDialog } from '../../../contexts/DialogContext'
import { useAccounts } from './hooks/useAccounts'
import { useSwitchAccount } from './hooks/useSwitchAccount'
import { getTags, getGroups } from '../../../api/groupTag'
import { applyFilters } from './utils/filterUtils'
import { cn } from '../../../utils/cn'
import { showSuccess, showError } from '../../../utils/toast'
import { getAccountDisplayName } from '../../../utils/accountStats'
import { normalizeAccountStatus } from '../../../utils/accountStatus'
import { normalizeAccountForUi } from './utils/accountRuntime'
import AccountHeader from './AccountHeader'
import AccountTable from './AccountTable'
import AccountListView from './AccountListView'
import ImportAccountModal from './ImportAccountModal'
import AccountDetailModal from './AccountDetailModal'
import EditAccountModal from './EditAccountModal'
import BatchEditModal from './BatchEditModal'
import ConfirmModal from './ConfirmModal'
import { AccountListSkeleton, AccountTableSkeleton } from '../../shared/Skeleton'
import { getThemeAccent } from '../KiroConfig/themeAccent'
import React from 'react'

interface AccountManagerProps {
  onNavigate: (path: string) => void;
}

function AccountManager({ onNavigate }: AccountManagerProps) {
  const { t, theme } = useApp()
  const accent = useMemo(() => getThemeAccent(theme), [theme])
  const { showConfirm } = useDialog()
  
  const [searchTerm, setSearchTerm] = useState('')
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  
  // 优化：将 selectedIds 转为 Set，提升查找性能（O(1) vs O(n)）
  const selectedIdsSet = useMemo(() => new Set(selectedIds), [selectedIds])
  const [editingAccount, setEditingAccount] = useState<any>(null)
  const [editingLabelAccount, setEditingLabelAccount] = useState<any>(null)
  const [showImportModal, setShowImportModal] = useState(false)
  const [showBatchEditModal, setShowBatchEditModal] = useState(false)
  const [copiedId, setCopiedId] = useState<string | null>(null)
  const [selectedTag, setSelectedTag] = useState<string | null>(null)
  const [selectedStatus, setSelectedStatus] = useState<string | null>(null)
  const [viewMode] = useState('card')
  const [tagDefinitions, setTagDefinitions] = useState<any[]>([])
  const [groupDefinitions, setGroupDefinitions] = useState<any[]>([])
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null)
  const [availableModelsById, setAvailableModelsById] = useState<Record<string, any>>({})
  const [availableModelsLoadingById, setAvailableModelsLoadingById] = useState<Record<string, boolean>>({})
  const [availableModelsErrorById, setAvailableModelsErrorById] = useState<Record<string, string>>({})
  const [advancedFilters, setAdvancedFilters] = useState<any>({
    subscriptions: [],
    statuses: [],
    providers: [],
    usageRange: null
  })
  const [sortBy, setSortBy] = useState('trialAsc')
  const [refreshingTokenId, setRefreshingTokenId] = useState<string | null>(null)
  const [togglingOverageId, setTogglingOverageId] = useState<string | null>(null)

  // 当前登录的本地 token（IDE）
  const [localToken, setLocalToken] = useState<any>(null)
  // 当前 CLI 登录账号的 id
  const [cliToken, setCliToken] = useState<string | null>(null)
  const mountedRef = useRef(true)

  // 读取 CLI 当前账号标识
  const loadCliToken = useCallback(async () => {
    try {
      const id = await invoke<string | null>('get_current_cli_account_id')
      if (mountedRef.current) {
        setCliToken(id || null)
      }
    } catch {
      if (mountedRef.current) {
        setCliToken(null)
      }
    }
  }, [])

  // 用于管理复制提示的timer
  const copiedTimerRef = useRef<NodeJS.Timeout | null>(null)
  
  // 切换账号 hook
  const {
    switchingId,
    switchDialog,
    setSwitchDialog,
    handleSwitchAccount,
    confirmSwitch,
    closeSwitchDialog} = useSwitchAccount(setLocalToken, loadCliToken)

  // 点击切换时按当前 IDE/CLI 登录态预勾选目标
  const handleSwitchWithTargets = useCallback((account: any) => {
    const targets: string[] = []
    if (localToken?.refreshToken && account.refreshToken === localToken.refreshToken) targets.push('ide')
    if (cliToken && account.id === cliToken) targets.push('cli')
    handleSwitchAccount(account, targets.length ? targets : ['ide'])
  }, [localToken, cliToken, handleSwitchAccount])
  
  useEffect(() => {
    mountedRef.current = true
    invoke<any>('get_kiro_local_token')
      .then(token => {
        if (mountedRef.current) setLocalToken(token)
      })
      .catch(() => {
        if (mountedRef.current) setLocalToken(null)
      })
    loadCliToken()
  }, [loadCliToken])

  // 加载标签定义
  const loadTagDefinitions = useCallback(() => {
    getTags()
      .then(tags => {
        if (mountedRef.current) {
          setTagDefinitions(tags as any[])
        }
      })
      .catch(() => {
        console.error('加载标签定义失败')
      })
  }, [])

  // 加载分组定义
  const loadGroupDefinitions = useCallback(() => {
    getGroups()
      .then(groups => {
        if (mountedRef.current) {
          setGroupDefinitions(groups)
        }
      })
      .catch((err) => {
        console.error('加载分组定义失败:', err)
      })
  }, [])

  useEffect(() => {
    loadTagDefinitions()
    loadGroupDefinitions()
  }, [loadTagDefinitions, loadGroupDefinitions])

  // 清理timer
  useEffect(() => {
    return () => {
      mountedRef.current = false
      if (copiedTimerRef.current) {
        clearTimeout(copiedTimerRef.current)
      }
    }
  }, [])

  const {
    accounts,
    setAccounts,
    loading,
    loadAccounts,
    autoRefreshing,
    refreshProgress,
    lastRefreshTime,
    refreshingId,
    batchRefreshAccounts,
    handleRefreshStatus,
    handleExport} = useAccounts()

  const clearAvailableModelsState = useCallback((id: string) => {
    setAvailableModelsById(prev => {
      if (!(id in prev)) return prev
      const next = { ...prev }
      delete next[id]
      return next
    })
    setAvailableModelsLoadingById(prev => {
      if (!(id in prev)) return prev
      const next = { ...prev }
      delete next[id]
      return next
    })
    setAvailableModelsErrorById(prev => {
      if (!(id in prev)) return prev
      const next = { ...prev }
      delete next[id]
      return next
    })
  }, [])

  const removeAccountsLocally = useCallback((ids: string[]) => {
    const idSet = new Set(ids)
    setAccounts(prev => prev.filter(account => !idSet.has(account.id)))
    setSelectedIds(prev => prev.filter(id => !idSet.has(id)))
    ids.forEach(clearAvailableModelsState)
  }, [clearAvailableModelsState, setAccounts])

  const patchAccountLocally = useCallback((updatedAccount: any) => {
    if (!updatedAccount?.id) return
    const normalizedAccount = normalizeAccountForUi(updatedAccount)
    setAccounts(prev => prev.map(account => account.id === normalizedAccount.id ? normalizedAccount : account))
  }, [setAccounts])

  const handleLoadAvailableModels = useCallback(async (id: string, options: any = {}) => {
    const { forceRefresh = false } = options
    setAvailableModelsLoadingById(prev => ({ ...prev, [id]: true }))
    setAvailableModelsErrorById(prev => {
      if (!(id in prev)) return prev
      const next = { ...prev }
      delete next[id]
      return next
    })

    try {
      const response = await invoke<any>('list_available_models', { id, forceRefresh })
      if (!mountedRef.current) return response
      const models = Array.isArray(response?.availableModels) ? response.availableModels : []
      setAvailableModelsById(prev => ({ ...prev, [id]: models }))
      setAccounts(prev => prev.map(account => (
        account.id === id
          ? {
              ...account,
              availableModelsCache: {
                response,
                cachedAt: Math.floor(Date.now() / 1000),
                modelProvider: null}}
          : account
      )))
      return response
    } catch (e) {
      const message = String(e)
      if (mountedRef.current) {
        setAvailableModelsErrorById(prev => ({ ...prev, [id]: message }))
      }
      throw e
    } finally {
      if (mountedRef.current) {
        setAvailableModelsLoadingById(prev => ({ ...prev, [id]: false }))
      }
    }
  }, [setAccounts])

  // 包装刷新函数，添加 toast 通知
  const handleRefreshWithNotify = useCallback(async (id: string) => {
    const result = await handleRefreshStatus(id)
    if (result.success) {
      clearAvailableModelsState(id)
      showSuccess(t('accounts.refreshSuccess'))
    } else if (result.error) {
      const errorMsg = result.error
      if (errorMsg.includes('BANNED')) {
        showError(t('accounts.accountBanned'))
      } else if (errorMsg.includes('AUTH_ERROR')) {
        // AUTH_ERROR: 静默处理，不弹窗
        console.log('[Sync] Token 已失效，已自动标记账号状态')
      } else if (errorMsg.includes('401') || errorMsg.includes('invalid')) {
        showError(t('accounts.tokenInvalid'))
      } else if (errorMsg.includes('error sending request') || errorMsg.includes('connection') || errorMsg.includes('network') || errorMsg.includes('timeout')) {
        showError('❌ 网络连接失败\n\n可能原因：\n• 网络不稳定\n• 代理设置有误\n• 防火墙拦截\n\n解决方法：\n1. 检查网络连接\n2. 检查代理设置\n3. 关闭防火墙或添加白名单')
      } else {
        showError(errorMsg.slice(0, 100))
      }
    }
    return result
  }, [clearAvailableModelsState, handleRefreshStatus, t])

  // 刷新 Token
  const handleRefreshToken = useCallback(async (id: string) => {
    setRefreshingTokenId(id)
    try {
      const account = await invoke<any>('refresh_account_token', { id })
      if (!mountedRef.current) return { success: true, account }
      patchAccountLocally(account)
      clearAvailableModelsState(id)
      showSuccess('Token 刷新成功')
      return { success: true, account }
    } catch (e) {
      const errorMsg = String(e)
      if (errorMsg.includes('BANNED')) {
        showError('账号已封禁')
      } else if (errorMsg.includes('AUTH_ERROR')) {
        // AUTH_ERROR: 静默处理，不弹窗（账号已自动标记为 invalid）
        console.log('[Refresh] Token 已失效，已自动标记账号状态')
      } else if (errorMsg.includes('401') || errorMsg.includes('invalid')) {
        showError('Token 无效，刷新失败')
      } else if (errorMsg.includes('error sending request') || errorMsg.includes('connection') || errorMsg.includes('network') || errorMsg.includes('timeout')) {
        showError('❌ 网络连接失败\n\n可能原因：\n• 网络不稳定\n• 代理设置有误\n• 防火墙拦截\n\n解决方法：\n1. 检查网络连接\n2. 检查代理设置\n3. 关闭防火墙或添加白名单')
      } else {
        showError(errorMsg.slice(0, 100))
      }
      return { success: false, error: errorMsg }
    } finally {
      if (mountedRef.current) {
        setRefreshingTokenId(null)
      }
    }
  }, [clearAvailableModelsState, patchAccountLocally])

  // 合并刷新：并行刷新配额与 Token，只弹一个提示
  const handleRefreshAll = useCallback(async (id: string) => {
    setRefreshingTokenId(id)
    try {
      const [quotaRes, tokenRes] = await Promise.allSettled([
        handleRefreshStatus(id),
        invoke<any>('refresh_account_token', { id }),
      ])
      if (!mountedRef.current) return

      const quotaAccount = quotaRes.status === 'fulfilled' && (quotaRes.value as any)?.success
        ? (quotaRes.value as any).data
        : null
      if (quotaAccount?.id) {
        patchAccountLocally(quotaAccount)
      }

      if (tokenRes.status === 'fulfilled' && tokenRes.value?.id) {
        const tok = normalizeAccountForUi(tokenRes.value)
        setAccounts(prev => prev.map(a => a.id === tok.id
          ? {
              ...tok,
              usageData: quotaAccount?.usageData ?? a.usageData ?? tok.usageData,
              status: quotaAccount?.status ?? a.status,
              enabled: quotaAccount?.enabled ?? a.enabled,
            }
          : a))
      }
      clearAvailableModelsState(id)

      const quotaOk = quotaRes.status === 'fulfilled' && (quotaRes.value as any)?.success
      const tokenOk = tokenRes.status === 'fulfilled'
      if (quotaOk || tokenOk) {
        showSuccess(t('accounts.refreshSuccess'))
        return
      }
      const errMsg = tokenRes.status === 'rejected'
        ? String((tokenRes as PromiseRejectedResult).reason)
        : String((quotaRes.status === 'fulfilled' ? (quotaRes.value as any)?.error : '') || '')
      if (errMsg.includes('BANNED')) showError(t('accounts.accountBanned'))
      else if (errMsg.includes('AUTH_ERROR')) { /* 静默 */ }
      else if (errMsg.includes('401') || errMsg.includes('invalid')) showError(t('accounts.tokenInvalid'))
      else if (errMsg) showError(errMsg.slice(0, 100))
    } finally {
      if (mountedRef.current) {
        setRefreshingTokenId(null)
      }
    }
  }, [handleRefreshStatus, patchAccountLocally, clearAvailableModelsState, setAccounts, t])

  // 获取所有标签（从标签定义中获取）
  const allTags = useMemo(() => {
    // 收集账号中使用的标签 ID（从 tagLinks 中提取）
    const usedTagIds = new Set<string>()
    accounts.forEach(a => {
      if (a.tagLinks) a.tagLinks.forEach((link: any) => usedTagIds.add(link.tagId))
    })
    // 返回被使用的标签定义
    return tagDefinitions.filter(t => usedTagIds.has(t.id))
  }, [accounts, tagDefinitions])

  // 当选中的标签不存在时，重置筛选（排除 __none__ 和 __has__ 特殊值）
  useEffect(() => {
    if (selectedTag && selectedTag !== '__none__' && selectedTag !== '__has__' && !allTags.find(t => t.id === selectedTag)) {
      setSelectedTag(null)
    }
  }, [allTags, selectedTag])


  // 优化：将 tagDefinitions 转为 Map，提升查找性能
  const tagDefinitionsMap = useMemo(() => {
    return new Map(tagDefinitions.map(t => [t.id, t]))
  }, [tagDefinitions])

  const normalizedAccounts = useMemo(() => accounts.map((account) => {
    const tagIds = (account.tagLinks || []).map((link: any) => link.tagId)
    const displayName = getAccountDisplayName(account).toLowerCase()
    const label = String(account.label || '').toLowerCase()
    const tagNames = tagIds
      .map(tagId => String(tagDefinitionsMap.get(tagId)?.name || ''))
      .filter(Boolean)
      .join(' ')
      .toLowerCase()

    return {
      account,
      tagIds,
      tagNames,
      label,
      displayName}
  }), [accounts, tagDefinitionsMap])

  const getTrialExpiry = useCallback((account: any) => {
    const expiry = account.usageData?.usageBreakdownList?.[0]?.freeTrialInfo?.freeTrialExpiry
    if (!expiry) return Number.POSITIVE_INFINITY
    return Number(expiry)
  }, [])

  const getUsagePercent = useCallback((account: any) => {
    const breakdown = account.usageData?.usageBreakdownList?.[0]
    if (!breakdown) return 0

    const mainUsed = Number(breakdown.currentUsage || 0)
    const mainLimit = Number(breakdown.usageLimit || 0)
    const trialUsed = Number(breakdown.freeTrialInfo?.currentUsage || 0)
    const trialLimit = Number(breakdown.freeTrialInfo?.usageLimit || 0)
    const bonusTotals = (breakdown.bonuses || []).reduce((sum: any, bonus: any) => ({
      used: sum.used + Number(bonus.currentUsage || 0),
      limit: sum.limit + Number(bonus.usageLimit || 0)}), { used: 0, limit: 0 })

    const used = mainUsed + trialUsed + bonusTotals.used
    const limit = mainLimit + trialLimit + bonusTotals.limit
    return limit > 0 ? (used / limit) * 100 : 0
  }, [])

  const filteredAccounts = useMemo(() => {
    const term = searchTerm.toLowerCase().trim()
    let result = normalizedAccounts.filter(({ account, tagIds, tagNames, label, displayName }) => {
      const matchSearch = !term || displayName.includes(term) || label.includes(term) || tagNames.includes(term)
      const matchGroup = !selectedGroup ||
        (selectedGroup === '__none__' ? !account.groupId :
         selectedGroup === '__has__' ? !!account.groupId :
         account.groupId === selectedGroup)
      const matchTag = !selectedTag ||
        (selectedTag === '__none__' ? tagIds.length === 0 :
         selectedTag === '__has__' ? tagIds.length > 0 :
         tagIds.includes(selectedTag))
      const matchStatus = !selectedStatus ||
        normalizeAccountStatus(account) === selectedStatus
      return matchSearch && matchGroup && matchTag && matchStatus
    })

    const hasAdvancedFilters = Boolean(
      advancedFilters?.usageRange ||
      advancedFilters?.subscriptions?.length ||
      advancedFilters?.statuses?.length ||
      advancedFilters?.providers?.length
    )

    if (hasAdvancedFilters) {
      const filteredIds = new Set(
        applyFilters(result.map(({ account }) => account), advancedFilters).map(account => account.id)
      )
      result = result.filter(({ account }) => filteredIds.has(account.id))
    }

    const sorted = [...result].sort((a, b) => {
      const accountA = a.account
      const accountB = b.account
      // 禁用账号永远排在最后，正常账号在前
      const enabledA = accountA.enabled !== false
      const enabledB = accountB.enabled !== false
      if (enabledA !== enabledB) return enabledA ? -1 : 1
      switch (sortBy) {
        case 'usageAsc':
          return getUsagePercent(accountA) - getUsagePercent(accountB)
        case 'usageDesc':
          return getUsagePercent(accountB) - getUsagePercent(accountA)
        case 'trialAsc':
          return getTrialExpiry(accountA) - getTrialExpiry(accountB)
        case 'trialDesc':
          return getTrialExpiry(accountB) - getTrialExpiry(accountA)
        case 'addedAsc':
          return new Date(accountA.addedAt || 0).getTime() - new Date(accountB.addedAt || 0).getTime()
        case 'addedDesc':
          return new Date(accountB.addedAt || 0).getTime() - new Date(accountA.addedAt || 0).getTime()
        default:
          return 0
      }
    })

    return sorted.map(({ account }) => account)
  }, [advancedFilters, getTrialExpiry, getUsagePercent, normalizedAccounts, searchTerm, selectedGroup, selectedStatus, selectedTag, sortBy])

  const accountRowStateById = useMemo(() => {
    const result: Record<string, any> = {}
    for (const account of filteredAccounts) {
      const id = account.id
      result[id] = {
        isRefreshing: refreshingId === id,
        isRefreshingToken: refreshingTokenId === id,
        isSwitching: switchingId === id,
        isTogglingOverage: togglingOverageId === id,
        isCopied: copiedId === id,
        availableModels: availableModelsById[id] ?? null,
        availableModelsLoading: Boolean(availableModelsLoadingById[id]),
        availableModelsError: availableModelsErrorById[id] ?? ''}
    }
    return result
  }, [filteredAccounts, refreshingId, refreshingTokenId, switchingId, togglingOverageId, copiedId, availableModelsById, availableModelsLoadingById, availableModelsErrorById])


  const handleSearchChange = useCallback((term: string) => { setSearchTerm(term) }, [])
  const handleGroupFilter = useCallback((group: string | null) => { setSelectedGroup(group) }, [])
  const handleTagFilter = useCallback((tag: string | null) => { setSelectedTag(tag) }, [])
  const handleStatusFilter = useCallback((status: string | null) => { setSelectedStatus(status) }, [])
  const handleSelectAll = useCallback((checked: boolean) => {
    setSelectedIds(checked ? filteredAccounts.map(a => a.id) : [])
  }, [filteredAccounts])

  const handleSelectOne = useCallback((id: string, checked: boolean) => {
    setSelectedIds(prev => checked ? [...prev, id] : prev.filter(i => i !== id))
  }, [])
  const handleCopy = useCallback((text: string, id: string) => { 
    navigator.clipboard.writeText(text).catch(e => console.error('Copy failed:', e))
    if (!mountedRef.current) return
    setCopiedId(id)
    if (copiedTimerRef.current) {
      clearTimeout(copiedTimerRef.current)
    }
    copiedTimerRef.current = setTimeout(() => {
      if (mountedRef.current) {
        setCopiedId(null)
      }
    }, 1500)
  }, [])
  
  // 切换账号启用/禁用
  const handleToggleEnabled = useCallback(async (account: any, enabled: boolean) => {
    // 乐观更新：立即更新本地状态
    patchAccountLocally({ ...account, enabled })

    try {
      const updated = await invoke<any>('update_account', { params: { id: account.id, enabled } })
      if (!mountedRef.current) return
      patchAccountLocally(updated)
    } catch (e) {
      if (!mountedRef.current) return
      // 失败时回滚
      patchAccountLocally({ ...account, enabled: !enabled })
      console.error('Toggle enabled failed:', e)
      showError('启用/禁用切换失败', String(e))
    }
  }, [patchAccountLocally])

  // 切换超额开关
  const handleToggleOverage = useCallback(async (account: any, enabled: boolean) => {
    setTogglingOverageId(account.id)

    // 乐观更新：立即更新本地状态
    patchAccountLocally({
      ...account,
      usageData: {
        ...account.usageData,
        overageConfiguration: {
          ...account.usageData?.overageConfiguration,
          overageStatus: enabled ? 'ENABLED' : 'DISABLED'
        }
      }
    })

    try {
      await invoke('set_overage_status', { id: account.id, enabled })
      // API 成功后，再次同步确保数据一致
      const result = await invoke<any>('sync_account', { id: account.id })
      if (!mountedRef.current) return
      if (result?.account) {
        patchAccountLocally(result.account)
      }
    } catch (e) {
      if (!mountedRef.current) return
      // 失败时回滚状态
      patchAccountLocally({
        ...account,
        usageData: {
          ...account.usageData,
          overageConfiguration: {
            ...account.usageData?.overageConfiguration,
            overageStatus: !enabled ? 'ENABLED' : 'DISABLED'
          }
        }
      })
      console.error('Failed to toggle overage:', e)
      showError('超额开关切换失败', String(e))
    } finally {
      if (mountedRef.current) {
        setTogglingOverageId(null)
      }
    }
  }, [patchAccountLocally])

  // 删除单个账号
  const handleDelete = useCallback(async (id: string) => {
    // 防呆：检查是否是当前账号
    const account = accounts.find(a => a.id === id)
    const isCurrent = localToken?.refreshToken && account?.refreshToken === localToken.refreshToken
    
    if (isCurrent) {
      const confirmed = await showConfirm(
        '⚠️ 删除当前账号',
        '您正在删除当前使用的账号！\n\n删除后 Kiro IDE 将无法使用，需要重新登录。\n\n确定要删除吗？'
      )
      if (!confirmed) return
    } else {
      const confirmed = await showConfirm(t('accounts.delete'), t('accounts.confirmDelete'))
      if (!confirmed) return
    }
    
    await invoke('delete_account', { id })
    if (!mountedRef.current) return
    removeAccountsLocally([id])
  }, [accounts, localToken, removeAccountsLocally, showConfirm, t])

  // 远程删除账号（从 AWS 服务端注销）
  const handleDeleteRemote = useCallback(async (account: any) => {
    const confirmed = await showConfirm(
      '⚠️ ' + t('accountCard.deleteRemote'),
      '远程删除将从 AWS 服务端注销此账号！\n\n此操作不可恢复，账号将永久失效。\n\n' + t('accountCard.deleteRemoteConfirm')
    )
    if (confirmed) {
      try {
        await invoke('delete_account_remote', { id: account.id, deleteLocal: true })
        if (!mountedRef.current) return
        removeAccountsLocally([account.id])
      } catch (e) {
        // 错误已通过 showError 显示
      }
    }
  }, [removeAccountsLocally, showConfirm, t])

  // 批量删除
  const onBatchDelete = useCallback(async () => {
    if (selectedIds.length === 0) return

    // 防呆：检查是否包含当前账号
    const currentAccount = accounts.find(a => localToken?.refreshToken && a.refreshToken === localToken.refreshToken)
    const includesCurrent = currentAccount && selectedIds.includes(currentAccount.id)

    if (includesCurrent) {
      const confirmed = await showConfirm(
        '⚠️ 批量删除包含当前账号',
        `您选择了 ${selectedIds.length} 个账号，其中包含当前使用的账号！\n\n删除后 Kiro IDE 将无法使用，需要重新登录。\n\n确定要删除吗？`
      )
      if (!confirmed) return
    } else {
      const confirmed = await showConfirm(t('accounts.batchDelete'), t('accounts.confirmDeleteMultiple', { count: selectedIds.length }))
      if (!confirmed) return
    }

    await invoke('delete_accounts', { ids: selectedIds })
    if (!mountedRef.current) return
    removeAccountsLocally(selectedIds)
    setSelectedIds([]) // 清除选中状态
  }, [accounts, selectedIds, localToken, removeAccountsLocally, showConfirm, t])

  return (
    <div className={cn('h-full flex flex-col', "glass-main")}>
      <div className="flex-1 flex flex-col min-h-0">
      <AccountHeader
        searchTerm={searchTerm}
        onSearchChange={handleSearchChange}
        selectedCount={selectedIds.length}
        onBatchDelete={onBatchDelete}
        onBatchEdit={() => setShowBatchEditModal(true)}
        onImport={() => setShowImportModal(true)}
        onExport={async () => {
          if (selectedIds.length === 0) {
            showError(t('accounts.exportSelectFirst') || '请先选择要导出的账号')
            return
          }
          await handleExport(selectedIds)
          setSelectedIds([]) // 清除选中状态
        }}
        onRefresh={() => loadAccounts(true)}
        onRefreshAll={async () => {
          if (selectedIds.length === 0) {
            showError(t('accounts.refreshSelectFirst') || '请先选择要刷新的账号')
            return
          }
          await batchRefreshAccounts(selectedIds, accounts)
          setSelectedIds([]) // 清除选中状态
        }}
        autoRefreshing={autoRefreshing}
        refreshProgress={refreshProgress}
        allGroups={groupDefinitions}
        selectedGroup={selectedGroup}
        onGroupFilter={handleGroupFilter}
        allTags={allTags}
        selectedTag={selectedTag}
        onTagFilter={handleTagFilter}
        selectedStatus={selectedStatus}
        onStatusFilter={handleStatusFilter}
        sortBy={sortBy}
        onSortChange={setSortBy}
        viewMode={viewMode}
        onViewModeChange={() => {}}
        advancedFilters={advancedFilters}
        onAdvancedFiltersChange={setAdvancedFilters}
        totalCount={filteredAccounts.length}
        onSelectAll={handleSelectAll}
        onDeselectAll={() => setSelectedIds([])}
      />
      <div className="flex-1 flex flex-col min-h-0">
      {loading ? (
        viewMode === 'card' ? <AccountListSkeleton count={8} /> : <AccountTableSkeleton count={8} />
      ) : filteredAccounts.length === 0 ? (
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center max-w-md px-6 py-10">
            <div className={`w-16 h-16 mx-auto mb-4 rounded-2xl bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} flex items-center justify-center shadow-md ring-1 ring-primary/20`}>
              <svg width="32" height="32" viewBox="0 0 40 40" fill="none">
                <path d="M20 4C12 4 6 10 6 18C6 22 8 25 8 25C8 25 7 28 7 30C7 32 8 34 10 34C11 34 12 33 13 32C14 33 16 34 20 34C24 34 26 33 27 32C28 33 29 34 30 34C32 34 33 32 33 30C33 28 32 25 32 25C32 25 34 22 34 18C34 10 28 4 20 4ZM14 20C12.5 20 11 18.5 11 17C11 15.5 12.5 14 14 14C15.5 14 17 15.5 17 17C17 18.5 15.5 20 14 20ZM26 20C24.5 20 23 18.5 23 17C23 15.5 24.5 14 26 14C27.5 14 29 15.5 29 17C29 18.5 27.5 20 26 20Z" fill="white" />
              </svg>
            </div>
            <h3 className="text-base font-semibold text-foreground mb-1.5">
              {searchTerm || selectedGroup || selectedTag || selectedStatus ? '没有找到匹配的账号' : '还没有账号'}
            </h3>
            <p className="text-xs text-muted-foreground mb-5">
              {searchTerm || selectedGroup || selectedTag || selectedStatus
                ? '试试调整筛选条件或搜索关键词'
                : '导入账号开始管理你的 Kiro IDE 账户'}
            </p>
            {!searchTerm && !selectedGroup && !selectedTag && !selectedStatus && (
              <button
                onClick={() => setShowImportModal(true)}
                className={`px-4 py-2 rounded-lg text-sm font-medium text-white bg-gradient-to-r ${accent.gradientFrom} ${accent.gradientTo} shadow-md hover:shadow-lg transition-all duration-200 inline-flex items-center gap-1.5 cursor-pointer`}
              >
                <Upload size={14} />
                导入账号
              </button>
            )}
          </div>
        </div>
      ) : viewMode === 'card' ? (
        <AccountTable
          accounts={filteredAccounts}
          totalCount={accounts.length}
          selectedIds={selectedIds}
          onSelectAll={handleSelectAll}
          onSelectOne={handleSelectOne}
          copiedId={copiedId}
          onCopy={handleCopy}
          onSwitch={handleSwitchWithTargets}
          onRefresh={handleRefreshWithNotify}
          onRefreshToken={handleRefreshToken}
          onRefreshAll={handleRefreshAll}
          onEdit={setEditingAccount}
          onEditLabel={setEditingLabelAccount}
          onToggleEnabled={handleToggleEnabled}
          onToggleOverage={handleToggleOverage}
          onDelete={handleDelete}
          onDeleteRemote={handleDeleteRemote}
          onAdd={() => setShowImportModal(true)}
          localToken={localToken}
          cliToken={cliToken}
          tagDefinitions={tagDefinitions}
          groupDefinitions={groupDefinitions}
          accountRowStateById={accountRowStateById}
          onLoadAvailableModels={handleLoadAvailableModels}
        />
      ) : (
        <AccountListView
          accounts={filteredAccounts}
          totalCount={accounts.length}
          selectedIds={selectedIds}
          selectedIdsSet={selectedIdsSet}
          onSelectAll={handleSelectAll}
          onSelectOne={handleSelectOne}
          onCopy={handleCopy}
          onSwitch={handleSwitchWithTargets}
          onRefresh={handleRefreshWithNotify}
          onRefreshToken={handleRefreshToken}
          onEdit={setEditingAccount}
          onEditLabel={setEditingLabelAccount}
          onToggleEnabled={handleToggleEnabled}
          onToggleOverage={handleToggleOverage}
          onDelete={handleDelete}
          onDeleteRemote={handleDeleteRemote}
          onAdd={() => setShowImportModal(true)}
          localToken={localToken}
          tagDefinitions={tagDefinitions}
          groupDefinitions={groupDefinitions}
          accountRowStateById={accountRowStateById}
          onLoadAvailableModels={handleLoadAvailableModels}
          sortBy={sortBy}
          onSortChange={setSortBy}
          onDeselectAll={() => setSelectedIds([])}
        />

      )}
      </div>
      {editingAccount && (
        <AccountDetailModal
          account={editingAccount}
          onClose={() => setEditingAccount(null)}
          onRefresh={loadAccounts}
        />
      )}
      {editingLabelAccount && (
        <EditAccountModal
          account={editingLabelAccount}
          onClose={() => setEditingLabelAccount(null)}
          onSuccess={(updatedAccount: any) => {
            setEditingLabelAccount(null)
            if (updatedAccount) {
              patchAccountLocally(updatedAccount)
            }
            loadTagDefinitions()
            loadGroupDefinitions()
          }}
        />
      )}
      {showImportModal && (
        <ImportAccountModal
          onClose={() => setShowImportModal(false)}
          onSuccess={({ added = [], updated = [] }) => {
            setShowImportModal(false)
            setAccounts(prev => {
              const next = [...prev]
              const upsert = (entry: any) => {
                const account = normalizeAccountForUi(entry?.account)
                if (!account?.id) return
                const index = next.findIndex(item => item.id === account.id)
                if (index >= 0) next[index] = account
                else next.unshift(account)
              }
              added.forEach(upsert)
              updated.forEach(upsert)
              return next
            })
          }}
          onNavigate={onNavigate}
        />
      )}
      {showBatchEditModal && (
        <BatchEditModal
          accountIds={selectedIds}
          accounts={accounts}
          onClose={() => setShowBatchEditModal(false)}
          onSuccess={({ accountIds: updatedIds, selectedTagIds, selectedGroupId }) => {
            setShowBatchEditModal(false)
            setAccounts(prev => prev.map(account => {
              if (!updatedIds.includes(account.id)) return account
              const nextTagLinks = Array.isArray(selectedTagIds)
                ? selectedTagIds.map(tagId => ({ tagId }))
                : account.tagLinks
              return { 
                ...account, 
                tagLinks: nextTagLinks,
                groupId: selectedGroupId
              }
            }))
            loadTagDefinitions()
            setSelectedIds([])
          }}
        />
      )}

      
      {/* 切换账号弹窗 */}
      {switchDialog && (
        <ConfirmModal
          type={switchDialog.type}
          title={switchDialog.title}
          message={switchDialog.message}
          onConfirm={switchDialog.type === 'confirm' ? confirmSwitch : closeSwitchDialog}
          onCancel={closeSwitchDialog}
          confirmText={switchDialog.type === 'confirm' ? t('switch.confirmBtn') : t('common.ok')}
          customContent={switchDialog.type === 'confirm' ? (
            <div className="flex items-center gap-3 mt-3 p-3 rounded-xl bg-muted/30 border border-border">
              <span className="text-xs text-muted-foreground font-medium shrink-0">切换目标</span>
              <div className="flex gap-1.5 flex-1">
                {(['ide', 'cli'] as const).map(target => {
                  const targets: string[] = (switchDialog as any).switchTargets || []
                  const active = targets.includes(target)
                  return (
                    <button
                      key={target}
                      onClick={() => {
                        const next = active ? targets.filter(x => x !== target) : [...targets, target]
                        setSwitchDialog({ ...switchDialog, switchTargets: next })
                      }}
                      className={`flex-1 px-3 py-1.5 rounded-lg text-xs font-semibold transition-all duration-200 ${
                        active
                          ? 'bg-primary text-primary-foreground shadow-sm'
                          : 'bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground'
                      }`}
                    >
                      {active ? '✓ ' : ''}{target === 'ide' ? '🖥 IDE' : '⌨ CLI'}
                    </button>
                  )
                })}
              </div>
            </div>
          ) : null}
        />
      )}
      </div>
    </div>
  )
}

export default AccountManager

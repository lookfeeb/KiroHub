import { useState, useEffect, useCallback, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen, emit, UnlistenFn } from '@tauri-apps/api/event'
import { isUnavailableStatus } from '../../../../utils/accountStatus'
import { normalizeAccountForUi, getSafeAccountDisplayName } from '../utils/accountRuntime'
import { Account } from '../../../../types/account'

export interface RefreshResult {
    email: string;
    success: boolean;
    message: string;
}

export interface RefreshProgress {
    current: number;
    total: number;
    currentEmail: string;
    results: RefreshResult[];
}

export function useAccounts() {
  const [accounts, setAccounts] = useState<Account[]>([])
  const [loading, setLoading] = useState(true)
  const [autoRefreshing, setAutoRefreshing] = useState(false)
  const [refreshProgress, setRefreshProgress] = useState<RefreshProgress>({ 
    current: 0, 
    total: 0, 
    currentEmail: '', 
    results: [] 
  })
  const [lastRefreshTime, setLastRefreshTime] = useState<string | null>(null)
  const [refreshingId, setRefreshingId] = useState<string | null>(null)
  const mountedRef = useRef(true)
  const refreshTimerRef = useRef<NodeJS.Timeout | null>(null)

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

  // 判断账号是否即将过期（5分钟内）
  const isExpiringSoon = useCallback((account: Account) => {
    if (isUnavailableStatus(account)) return false
    if (!account.expiresAt) return false
    try {
      const expiresAt = new Date(account.expiresAt.replace(/\//g, '-'))
      if (isNaN(expiresAt.getTime())) return false
      return expiresAt.getTime() - Date.now() < 5 * 60 * 1000
    } catch {
      return false
    }
  }, [])

  const loadAccounts = useCallback(async (silent = false) => {
    try {
      if (!silent) setLoading(true)
      const loadedAccounts = await invoke<any[]>('get_accounts')
      const normalizedAccounts = Array.isArray(loadedAccounts)
        ? loadedAccounts.map(normalizeAccountForUi)
        : []
      if (!mountedRef.current) return
      setAccounts(normalizedAccounts)
    } catch (e) {
      console.error('加载账号失败:', e)
    } finally {
      if (!silent && mountedRef.current) setLoading(false)
    }
  }, [])

  // 批量刷新账号
  const batchRefreshAccounts = useCallback(async (accountIds: string[], accountList: Account[]) => {
    if (autoRefreshing || accountList.length === 0) return
    
    const accountsToRefresh = accountIds.length > 0
      ? accountList.filter(acc => accountIds.includes(acc.id))
      : accountList.filter(acc => !isUnavailableStatus(acc)).filter(isExpiringSoon)
    
    if (accountsToRefresh.length === 0) return

    const count = accountsToRefresh.length
    const concurrency = Math.min(20, Math.max(3, Math.ceil(count / 10)))

    setAutoRefreshing(true)
    setRefreshProgress({ current: 0, total: accountsToRefresh.length, currentEmail: '', results: [] })

    const updatedAccounts = [...accountList]
    const results: RefreshResult[] = []
    let completed = 0

    const refreshOne = async (account: Account) => {
      let success = false, message = ''
      try {
        const syncResult = await invoke<{ account: any }>('sync_account', { id: account.id })
        const updated = normalizeAccountForUi(syncResult.account)
        const idx = updatedAccounts.findIndex(a => a.id === account.id)
        if (idx !== -1) updatedAccounts[idx] = updated
        success = true
        message = '同步成功'
      } catch (e) {
        const errorMsg = String(e)
        const idx = updatedAccounts.findIndex(a => a.id === account.id)
        if (errorMsg.includes('BANNED')) {
          message = '账号已封禁'
          if (idx !== -1) updatedAccounts[idx] = { ...updatedAccounts[idx], status: 'banned', enabled: false }
        } else if (errorMsg.includes('AUTH_ERROR') || errorMsg.includes('401') || errorMsg.includes('invalid')) {
          message = '账号已失效'
          if (idx !== -1) updatedAccounts[idx] = { ...updatedAccounts[idx], status: 'invalid', enabled: false }
        } else {
          message = errorMsg.slice(0, 30)
        }
      }
      completed++
      results.push({ email: getSafeAccountDisplayName(account), success, message })
      if (mountedRef.current) {
        setRefreshProgress({ current: completed, total: accountsToRefresh.length, currentEmail: '', results: [...results] })
      }
      return { account, success, message }
    }

    for (let i = 0; i < accountsToRefresh.length; i += concurrency) {
      const batch = accountsToRefresh.slice(i, i + concurrency)
      if (!mountedRef.current) return
      setRefreshProgress(prev => ({
        ...prev,
        currentEmail: batch.map(a => getSafeAccountDisplayName(a).split('@')[0]).join(', ')
      }))
      await Promise.all(batch.map(refreshOne))
    }

    if (!mountedRef.current) return
    setAccounts(updatedAccounts)
    setLastRefreshTime(new Date().toLocaleTimeString())
    emit('accounts-updated')
    if (refreshTimerRef.current) {
      clearTimeout(refreshTimerRef.current)
    }
    refreshTimerRef.current = setTimeout(() => {
      if (!mountedRef.current) return
      setAutoRefreshing(false)
      setRefreshProgress({ current: 0, total: 0, currentEmail: '', results: [] })
    }, 1500)
  }, [autoRefreshing, isExpiringSoon])

  const handleRefreshStatus = useCallback(async (id: string) => {
    setRefreshingId(id)
    try {
      const syncResult = await invoke<{ account: any }>('sync_account', { id })
      const updated = normalizeAccountForUi(syncResult.account)
      if (mountedRef.current) {
        setAccounts(prev => prev.map(a => a.id === id ? updated : a))
      }
      return { success: true, data: updated }
    } catch (e) {
      const errorMsg = String(e)
      if (errorMsg.includes('BANNED')) {
        try {
          await invoke('update_account', { params: { id, status: 'banned', enabled: false } })
          if (mountedRef.current) {
            setAccounts(prev => prev.map(a => a.id === id ? { ...a, status: 'banned', enabled: false } : a))
          }
        } catch (updateErr) {
          console.error('持久化封禁账号状态失败:', updateErr)
        }
      } else if (errorMsg.includes('AUTH_ERROR') || errorMsg.includes('401') || errorMsg.includes('invalid')) {
        try {
          await invoke('update_account', { params: { id, status: 'invalid', enabled: false } })
          if (mountedRef.current) {
            setAccounts(prev => prev.map(a => a.id === id ? { ...a, status: 'invalid', enabled: false } : a))
          }
        } catch (updateErr) {
          console.error('持久化失效账号状态失败:', updateErr)
        }
      }
      return { success: false, error: errorMsg }
    } finally {
      if (mountedRef.current) {
        setRefreshingId(null)
      }
    }
  }, [])

  const handleExport = useCallback(async (selectedIds: string[] = []) => {
    try {
      if (selectedIds.length === 0) return
      
      const { save } = await import('@tauri-apps/plugin-dialog')
      const { writeTextFile } = await import('@tauri-apps/plugin-fs')
      const { downloadDir } = await import('@tauri-apps/api/path')
      
      const defaultName = `kiro-accounts-${selectedIds.length}-${new Date().toISOString().slice(0, 10)}.json`
      const defaultDir = await downloadDir()
      const sep = defaultDir.includes('\\') ? '\\' : '/'
      
      const filePath = await save({
        defaultPath: `${defaultDir}${sep}${defaultName}`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
        title: '导出账号数据'
      })
      
      if (!mountedRef.current || !filePath) return
      
      const json = await invoke<string>('export_accounts', { ids: selectedIds })
      if (!mountedRef.current) return
      await writeTextFile(filePath, json)
    } catch (e) {
      console.error('导出账号失败:', e)
    }
  }, [])

  useEffect(() => {
    let unlistenLoginSuccess: UnlistenFn | null = null
    let unlistenAccountsUpdated: UnlistenFn | null = null
    let unlistenKiroLoginData: UnlistenFn | null = null
    let mounted = true

    const setUnlisten = (setter: (fn: UnlistenFn) => void) => (fn: UnlistenFn) => {
      if (mounted) {
        setter(fn)
      } else {
        fn()
      }
    }

    const setupListeners = async () => {
      listen('login-success', () => {
        if (mounted) loadAccounts()
      }).then(setUnlisten(fn => { unlistenLoginSuccess = fn }))

      listen('accounts-updated', () => {
        if (mounted) loadAccounts()
      }).then(setUnlisten(fn => { unlistenAccountsUpdated = fn }))

      listen<any>('kiro-login-data', async (event) => {
        if (!mounted) return
        try {
          const data = typeof event.payload === 'string' ? JSON.parse(event.payload) : event.payload
          if (data?.refreshToken) {
            await invoke('add_account_by_social', {
              refreshToken: data.refreshToken,
              provider: data.idp || data.provider || null
            })
            if (mounted) loadAccounts()
          }
        } catch (e) {
          console.error('导入登录事件账号失败:', e)
        }
      }).then(setUnlisten(fn => { unlistenKiroLoginData = fn }))
    }

    loadAccounts()
    setupListeners()

    return () => {
      mounted = false
      if (unlistenLoginSuccess) unlistenLoginSuccess()
      if (unlistenAccountsUpdated) unlistenAccountsUpdated()
      if (unlistenKiroLoginData) unlistenKiroLoginData()
    }
  }, [loadAccounts])

  useEffect(() => {
    return () => {
      if (refreshTimerRef.current) {
        clearTimeout(refreshTimerRef.current)
      }
    }
  }, [])

  return {
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
    handleExport}
}

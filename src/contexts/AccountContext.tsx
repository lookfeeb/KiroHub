import { createContext, useContext, useState, useEffect, useMemo, useCallback, useRef, ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen, UnlistenFn } from '@tauri-apps/api/event'
import { calcAccountStats, getQuota, getUsed } from '../utils/accountStats'
import { Account } from '../types/account'

interface LocalToken {
    refreshToken?: string;
    accessToken?: string;
}

interface AccountStats {
    total: number;
    active: number;
    unavailable: number;
    pro: number;
    proPlus: number;
    usagePercent: number;
    totalQuota: number;
    totalUsed: number;
    remaining: number;
    totalQuotaStr: string;
    totalUsedStr: string;
    remainingStr: string;
}

interface QuotaInfo {
    quota: number;
    used: number;
    percent: number;
}

interface AccountContextValue {
    accounts: Account[];
    localToken: LocalToken | null;
    loading: boolean;
    refreshing: boolean;
    error: any;
    stats: AccountStats;
    currentAccount: Account | null | undefined;
    currentQuotaInfo: QuotaInfo;
    refresh: () => Promise<void>;
    refreshAccount: (id: string) => Promise<void>;
}

const AccountContext = createContext<AccountContextValue | null>(null)

export function AccountProvider({ children }: { children: ReactNode }) {
  const [accounts, setAccounts] = useState<Account[]>([])
  const [localToken, setLocalToken] = useState<LocalToken | null>(null)
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState<any>(null)

  // 用 ref 存储定时器，以便在组件卸载时清理
  const refreshTimerRef = useRef<NodeJS.Timeout | null>(null)
  const mountedRef = useRef(true)

  // 加载数据
  const loadData = useCallback(async () => {
    try {
      setError(null)
      const [accountsData, localData] = await Promise.all([
        invoke<Account[]>('get_accounts'),
        invoke<LocalToken>('get_kiro_local_token').catch((err) => {
          console.error('读取 Kiro 本地 token 失败:', err)
          return null
        })
      ])
      if (!mountedRef.current) return
      setAccounts(accountsData || [])
      setLocalToken(localData as LocalToken | null)
    } catch (e) {
      if (mountedRef.current) {
        setError(e)
      }
    }
  }, [])

  // 初始加载 + 监听事件
  useEffect(() => {
    let unlistenLogin: UnlistenFn | null = null
    let unlistenAccounts: UnlistenFn | null = null
    let mounted = true
    mountedRef.current = true

    const keepUnlisten = (setter: (fn: UnlistenFn) => void) => (fn: UnlistenFn) => {
      if (mounted) {
        setter(fn)
      } else {
        fn()
      }
    }

    const setup = async () => {
      // 监听登录成功事件，刷新数据
      listen('login-success', () => {
        if (!mounted) return
        loadData()
      }).then(keepUnlisten(fn => { unlistenLogin = fn }))
        .catch(err => console.error('注册 login-success 监听失败:', err))

      // 监听账号数据变化
      listen('accounts-updated', () => {
        if (!mounted) return
        loadData()
      }).then(keepUnlisten(fn => { unlistenAccounts = fn }))
        .catch(err => console.error('注册 accounts-updated 监听失败:', err))
    }

    loadData().finally(() => {
      if (mounted) setLoading(false)
    })
    setup()

    return () => {
      mounted = false
      mountedRef.current = false
      if (unlistenLogin) unlistenLogin()
      if (unlistenAccounts) unlistenAccounts()
      if (refreshTimerRef.current) clearTimeout(refreshTimerRef.current)
    }
  }, [loadData])

  // 刷新所有数据
  const refresh = useCallback(async () => {
    if (!mountedRef.current) return
    setRefreshing(true)
    try {
      await loadData()
    } finally {
      if (!mountedRef.current) return
      // 清除之前的定时器
      if (refreshTimerRef.current) {
        clearTimeout(refreshTimerRef.current)
      }
      // 设置新的定时器
      refreshTimerRef.current = setTimeout(() => {
        if (mountedRef.current) {
          setRefreshing(false)
        }
      }, 300)
    }
  }, [loadData])

  // 刷新单个账号
  const refreshAccount = useCallback(async (id: string) => {
    try {
      const result = await invoke<{ warning?: string }>('sync_account', { id })
      if (result.warning) {
        console.warn(`[账号同步警告] ${result.warning}`)
      }
    } catch (e) {
      console.error('刷新账号失败:', e)
    }
    if (mountedRef.current) {
      await loadData()
    }
  }, [loadData])

  // 缓存统计数据
  const stats = useMemo(() => calcAccountStats(accounts), [accounts])

  // 缓存当前账号匹配
  const currentAccount = useMemo(() => {
    if (!localToken) return null
    return accounts.find(a =>
      (localToken.refreshToken && a.refreshToken === localToken.refreshToken) ||
      (localToken.accessToken && a.accessToken === localToken.accessToken)
    )
  }, [accounts, localToken])

  // 当前账号配额信息
  const currentQuotaInfo = useMemo<QuotaInfo>(() => {
    if (!currentAccount) return { quota: 0, used: 0, percent: 0 }
    const quota = getQuota(currentAccount)
    const used = getUsed(currentAccount)
    const percent = quota > 0 ? Number((used / quota * 100).toFixed(1)) : 0
    return { quota, used, percent }
  }, [currentAccount])

  const value: AccountContextValue = {
    accounts,
    localToken,
    loading,
    refreshing,
    error,
    stats,
    currentAccount,
    currentQuotaInfo,
    refresh,
    refreshAccount
  }

  return (
    <AccountContext.Provider value={value}>
      {children}
    </AccountContext.Provider>
  )
}

export function useAccount() {
  const ctx = useContext(AccountContext)
  if (!ctx) throw new Error('useAccount must be used within AccountProvider')
  return ctx
}

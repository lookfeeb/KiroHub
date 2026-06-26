import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { McpClient, McpOAuthStatus, McpServerItem, McpStats } from './types'
import { CLIENTS, authKey, copyKey, isRemoteType, refreshKey } from './utils'

interface UseMcpToolsOptions {
  activeClient: McpClient;
  showConfirm: (
    title: string,
    message: string,
    options?: { confirmText?: string; cancelText?: string },
  ) => Promise<boolean>;
}

export function useMcpTools({ activeClient, showConfirm }: UseMcpToolsOptions) {
  const [stats, setStats] = useState<McpStats | null>(null)
  const [serversByClient, setServersByClient] = useState<Record<McpClient, McpServerItem[]>>({
    codex: [],
    kiro: [],
    'claude-cli': [],
  })
  const [loading, setLoading] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [auth, setAuth] = useState<Record<string, McpOAuthStatus>>({})
  const [busyMap, setBusyMap] = useState<Record<string, boolean>>({})
  const [refreshed, setRefreshed] = useState(false)
  const [refreshOk, setRefreshOk] = useState<Record<string, boolean>>({})
  const [copyOk, setCopyOk] = useState<Record<string, boolean>>({})
  const mountedRef = useRef(true)
  const loadSeqRef = useRef(0)
  const serversByClientRef = useRef(serversByClient)
  const feedbackTimersRef = useRef<Map<string, NodeJS.Timeout>>(new Map())

  const clearFeedbackTimer = useCallback((key: string) => {
    const timer = feedbackTimersRef.current.get(key)
    if (timer) {
      clearTimeout(timer)
      feedbackTimersRef.current.delete(key)
    }
  }, [])

  const setTimedFeedback = useCallback((key: string, clear: () => void, delay = 1500) => {
    clearFeedbackTimer(key)
    const timer = setTimeout(() => {
      if (!mountedRef.current) return
      clear()
      feedbackTimersRef.current.delete(key)
    }, delay)
    feedbackTimersRef.current.set(key, timer)
  }, [clearFeedbackTimer])

  useEffect(() => {
    mountedRef.current = true

    return () => {
      mountedRef.current = false
      feedbackTimersRef.current.forEach(timer => clearTimeout(timer))
      feedbackTimersRef.current.clear()
    }
  }, [])

  useEffect(() => {
    serversByClientRef.current = serversByClient
  }, [serversByClient])

  const setBusy = useCallback((key: string, value: boolean) => {
    if (!mountedRef.current) return
    setBusyMap(prev => ({ ...prev, [key]: value }))
  }, [])

  const servers = serversByClient[activeClient] ?? []

  const loadAuth = useCallback((items: McpServerItem[]) => {
    items.filter(s => isRemoteType(s.type)).forEach(s => {
      invoke<McpOAuthStatus>('mcp_oauth_status_for_client', { client: s.client, serverName: s.name })
        .then(st => {
          if (mountedRef.current) {
            setAuth(prev => ({ ...prev, [authKey(s.client, s.name)]: st }))
          }
        })
        .catch((err) => {
          console.error(`加载 MCP OAuth 状态失败: ${s.client}/${s.name}`, err)
        })
    })
  }, [])

  const sortServers = useCallback((list: McpServerItem[]) => {
    return [...list].sort((a, b) => {
      if (isRemoteType(a.type) !== isRemoteType(b.type)) return isRemoteType(a.type) ? -1 : 1
      return a.name.localeCompare(b.name)
    })
  }, [])

  const load = useCallback(() => {
    const seq = ++loadSeqRef.current
    setLoading(true)
    setLoadError(null)
    return Promise.all([
      invoke<McpStats>('get_mcp_clients_overview'),
      ...CLIENTS.map(c => invoke<McpServerItem[]>('get_mcp_config_by_client', { client: c.key })),
    ] as const)
      .then(([overview, ...clientLists]) => {
        if (!mountedRef.current || seq !== loadSeqRef.current) return
        setStats(overview)
        const nextServersByClient = CLIENTS.reduce((acc, client, index) => {
          acc[client.key] = sortServers(clientLists[index] ?? [])
          return acc
        }, {} as Record<McpClient, McpServerItem[]>)
        setServersByClient(nextServersByClient)
        loadAuth(Object.values(nextServersByClient).flat())
      })
      .catch((e) => {
        if (!mountedRef.current || seq !== loadSeqRef.current) return
        const message = String(e)
        console.error('加载 MCP 配置失败', e)
        setLoadError(message)
      })
      .finally(() => {
        if (mountedRef.current && seq === loadSeqRef.current) {
          setLoading(false)
        }
      })
  }, [loadAuth, sortServers])

  const reloadAuthForCurrentServers = useCallback(() => {
    loadAuth(Object.values(serversByClientRef.current).flat())
  }, [loadAuth])

  const refreshAll = useCallback(async () => {
    setLoading(true)
    try {
      await Promise.allSettled([
        invoke('discover_and_import_mcp_servers'),
        ...CLIENTS.map(c => invoke<McpServerItem[]>('get_mcp_config_by_client', { client: c.key })
          .then(items => Promise.allSettled(
            items.filter(s => isRemoteType(s.type))
              .map(s => invoke('mcp_oauth_refresh_for_client', { client: c.key, serverName: s.name }))
          ))),
      ])
      await load()
      if (!mountedRef.current) return
      setRefreshed(true)
      setTimedFeedback('refresh-all', () => setRefreshed(false))
    } finally {
      if (mountedRef.current) {
        setLoading(false)
      }
    }
  }, [load])

  const authorize = useCallback(async (server: McpServerItem) => {
    const key = authKey(server.client, server.name)
    setBusy(key, true)
    try {
      await invoke('mcp_oauth_authorize_for_client', { client: server.client, serverName: server.name })
      const st = await invoke<McpOAuthStatus>('mcp_oauth_status_for_client', { client: server.client, serverName: server.name })
      if (!mountedRef.current) return
      setAuth(prev => ({ ...prev, [key]: st }))
      await load()
    } catch (e) {
      if (!String(e).includes('授权已取消')) {
        console.error('授权失败', e)
      }
    } finally {
      setBusy(key, false)
    }
  }, [load, setBusy])

  const cancelAuthorize = useCallback(async (server: McpServerItem) => {
    const key = authKey(server.client, server.name)
    try {
      await invoke('mcp_oauth_cancel_authorize_for_client', { client: server.client, serverName: server.name })
    } catch (e) {
      console.error('取消授权失败', e)
      setBusy(key, false)
    }
  }, [setBusy])

  const refreshOne = useCallback(async (server: McpServerItem) => {
    const key = authKey(server.client, server.name)
    const tokenRefreshKey = refreshKey(server.client, server.name)
    setBusy(tokenRefreshKey, true)
    setRefreshOk(prev => ({ ...prev, [tokenRefreshKey]: false }))
    try {
      const st = await invoke<McpOAuthStatus>('mcp_oauth_refresh_for_client', { client: server.client, serverName: server.name })
      if (!mountedRef.current) return
      setAuth(prev => ({ ...prev, [key]: st }))
      setRefreshOk(prev => ({ ...prev, [tokenRefreshKey]: true }))
      setTimedFeedback(tokenRefreshKey, () => {
        setRefreshOk(prev => ({ ...prev, [tokenRefreshKey]: false }))
      })
    } catch (e) {
      console.error('刷新 OAuth 失败', e)
      const st = await invoke<McpOAuthStatus>('mcp_oauth_status_for_client', { client: server.client, serverName: server.name })
      if (mountedRef.current) {
        setAuth(prev => ({ ...prev, [key]: st }))
      }
    } finally {
      setBusy(tokenRefreshKey, false)
    }
  }, [setBusy])

  const revoke = useCallback(async (server: McpServerItem) => {
    const ok = await showConfirm('撤销授权', `确定要撤销 ${server.name} 在 ${server.client} 的授权吗？其它客户端绑定不会受影响。`, { confirmText: '撤销', cancelText: '取消' })
    if (!ok) return
    const key = authKey(server.client, server.name)
    setBusy(key, true)
    try {
      await invoke('mcp_oauth_revoke_for_client', { client: server.client, serverName: server.name })
      if (!mountedRef.current) return
      setAuth(prev => ({ ...prev, [key]: { authorized: false, expiresAt: 0, expiringSoon: false, expired: false, refreshFailed: false, needsReauth: false } }))
      await load()
    } catch (e) {
      console.error('撤销失败', e)
    } finally {
      setBusy(key, false)
    }
  }, [load, setBusy, showConfirm])

  const deleteServer = useCallback(async (server: McpServerItem) => {
    const ok = await showConfirm('删除 MCP 服务器', `确定要从 ${server.client} 删除 ${server.name} 吗？`, { confirmText: '删除', cancelText: '取消' })
    if (!ok) return
    const key = authKey(server.client, server.name)
    setBusy(key, true)
    try {
      await invoke('delete_mcp_server_by_client', { client: server.client, name: server.name })
      if (!mountedRef.current) return
      await load()
    } catch (e) {
      console.error('删除失败', e)
    } finally {
      setBusy(key, false)
    }
  }, [load, setBusy, showConfirm])

  const copyTo = useCallback(async (server: McpServerItem, toClient: McpClient) => {
    const key = copyKey(server.client, server.name, toClient)
    setBusy(key, true)
    setCopyOk(prev => ({ ...prev, [key]: false }))
    try {
      await invoke('copy_mcp_server_to_client', {
        fromClient: server.client,
        toClient,
        name: server.name,
        overwrite: true,
      })
      await load()
      if (!mountedRef.current) return
      setCopyOk(prev => ({ ...prev, [key]: true }))
      setTimedFeedback(key, () => {
        setCopyOk(prev => ({ ...prev, [key]: false }))
      })
    } catch (e) {
      console.error('复制 MCP 失败', e)
    } finally {
      setBusy(key, false)
    }
  }, [load, setBusy])

  const activeStats = useMemo(() => {
    const enabledServers = servers.filter(s => !s.disabled).length
    return {
      totalServers: servers.length,
      enabledServers,
      estimatedTools: enabledServers * 7,
    }
  }, [servers])

  return {
    stats,
    servers,
    loading,
    loadError,
    auth,
    busyMap,
    refreshed,
    refreshOk,
    copyOk,
    activeStats,
    load,
    refreshAll,
    reloadAuthForCurrentServers,
    authorize,
    cancelAuthorize,
    refreshOne,
    revoke,
    deleteServer,
    copyTo,
  }
}

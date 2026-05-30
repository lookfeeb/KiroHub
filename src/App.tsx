import { useState, useEffect, Suspense, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen, UnlistenFn } from '@tauri-apps/api/event'
import { Toaster } from 'react-hot-toast'
import Sidebar from './components/features/Layout'
import UpdateChecker from './components/shared/UpdateChecker'
import { dismissBootSplash } from './utils/bootSplash'

import { useApp } from './hooks/useApp'
import { useAppSettings } from './contexts/AppSettingsContext'
import { useDialog } from './contexts/DialogContext'
import { AccountProvider } from './contexts/AccountContext'
import { PrivacyProvider } from './contexts/PrivacyContext'
import { routes, internalRoutes } from './routes'
import { getMountedRouteIds, shouldPersistRoute } from './utils/routePersistence'

// 构建路由映射
const routeMap = Object.fromEntries(routes.map(r => [r.id, r.component]))
const allRoutes = { ...routeMap, ...internalRoutes }

// 页面加载骨架屏
function PageLoading() {
  return (
    <div className="h-full flex items-center justify-center glass-main">
      <div className="animate-pulse text-muted-foreground">加载中...</div>
    </div>
  )
}

function App() {
  const [user, setUser] = useState<any>(null)
  const [activeMenu, setActiveMenu] = useState<string>(() => {
    return localStorage.getItem('activeMenu') || 'home'
  })
  const [mountedRouteIds, setMountedRouteIds] = useState<string[]>(() => 
    getMountedRouteIds([], localStorage.getItem('activeMenu') || 'home')
  )
  const { t } = useApp()
  const { settings: appSettings, loading: settingsLoading } = useAppSettings()
  const { showError, showInfo } = useDialog()

  // 保存当前页面到 localStorage
  useEffect(() => {
    if (activeMenu && activeMenu !== 'callback') {
      localStorage.setItem('activeMenu', activeMenu)
    }
  }, [activeMenu])

  useEffect(() => {
    setMountedRouteIds(prev => getMountedRouteIds(prev, activeMenu))
  }, [activeMenu])

  useEffect(() => {
    dismissBootSplash()
  }, [])

  useEffect(() => {
    checkAuth()

    const url = new URL(window.location.href)
    if (url.pathname === '/callback' && (url.searchParams.has('code') || url.searchParams.has('state'))) {
      setActiveMenu('callback')
      return
    }

    let unlisten: UnlistenFn | null = null
    let unlistenBanned: UnlistenFn | null = null
    let unlistenTokenInvalid: UnlistenFn | null = null
    let unlistenNetworkError: UnlistenFn | null = null
    let mounted = true

    const setupListeners = async () => {
      unlisten = await listen('login-success', () => {
        if (!mounted) return
        checkAuth()
        setActiveMenu('accounts')
      })
      
      // 后端自动运行，前端无需监听 settings-changed 和 app-settings-changed
      
      unlistenBanned = await listen<{ email: string }>('account-banned', (event) => {
        if (!mounted) return
        showError('账号已封禁', `账号 ${event.payload.email} 已被封禁，无法继续使用`)
      })

      unlistenTokenInvalid = await listen<{ email: string }>('account-token-invalid', (event) => {
        if (!mounted) return
        showInfo('Token 已失效', `账号 ${event.payload.email} 的 Token 已失效，请重新登录`)
      })

      unlistenNetworkError = await listen<{ count: number, total: number }>('sync-network-error', (event) => {
        if (!mounted) return
        showError('网络错误', `${event.payload.count}/${event.payload.total} 个账号同步失败，请检查网络连接`)
      })
    }

    setupListeners()
    
    return () => { 
      mounted = false
      if (unlisten) unlisten()
      if (unlistenBanned) unlistenBanned()
      if (unlistenTokenInvalid) unlistenTokenInvalid()
      if (unlistenNetworkError) unlistenNetworkError()
    }
  }, [])

  const checkAuth = async () => {
    try {
      const currentUser = await invoke<any>('get_current_user')
      setUser(currentUser)
    } catch (e) {
      console.error('Auth check failed:', e)
    }
  }

  const handleLogin = () => {
    checkAuth()
  }

  const handleLogout = async () => {
    await invoke('logout')
    setUser(null)
  }

  const routeProps = useMemo<Record<string, any>>(() => ({
    home: { onNavigate: setActiveMenu },
    desktopOAuth: { onLogin: () => { handleLogin(); setActiveMenu('accounts') } },
    accounts: { onNavigate: setActiveMenu }
  }), [])

  const renderContent = () => {
    if (!shouldPersistRoute(activeMenu)) {
      const RouteComponent = allRoutes[activeMenu] || allRoutes.home
      return <RouteComponent {...(routeProps[activeMenu] || {})} />
    }

    return mountedRouteIds.map((routeId) => {
      const RouteComponent = allRoutes[routeId]
      if (!RouteComponent) return null

      return (
        <section
          key={routeId}
          className="h-full w-full"
          style={{ display: routeId === activeMenu ? 'block' : 'none' }}
          aria-hidden={routeId === activeMenu ? 'false' : 'true'}
        >
          <RouteComponent {...(routeProps[routeId] || {})} />
        </section>
      )
    })
  }

  return (
    <PrivacyProvider>
      <AccountProvider>
        <div className="flex h-screen w-full bg-transparent overflow-hidden">
          <Sidebar 
            activeMenu={activeMenu} 
            onMenuChange={setActiveMenu}
            onLogout={handleLogout}
          />
          <main className="flex-1 overflow-hidden glass-main">
            <div className="h-full w-full">
              <Suspense fallback={<PageLoading />}>
                {renderContent()}
              </Suspense>
            </div>
          </main>
          
          <UpdateChecker />
          <Toaster 
            position="top-center"
            toastOptions={{
              style: {
                marginTop: '80px'
              }
            }}
          />
        </div>
      </AccountProvider>
    </PrivacyProvider>
  )
}

export default App

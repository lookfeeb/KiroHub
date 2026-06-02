import { createContext, useContext, useState, useEffect, useRef, useCallback, ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen, UnlistenFn } from '@tauri-apps/api/event'

export interface AppSettings {
  lockModel: boolean;
  lockedModel: string | null;
  autoRefresh: boolean;
  autoRefreshInterval: number;
  autoChangeMachineId: boolean;
  bindMachineIdToAccount: boolean;
  browserPath: string;
  privacyMode: boolean;
  autoSwitchEnabled: boolean;
  autoSwitchThreshold: number;
  autoSwitchInterval: number;
  switchTarget: 'ide' | 'cli' | 'both';
  enableCodebaseIndexing: boolean;
  enableTabAutocomplete: boolean;
  usageSummary: boolean;
  enableDebugLogs: boolean;
  notifyActionRequired: boolean;
  notifyFailure: boolean;
  notifySuccess: boolean;
  notifyBilling: boolean;
  trustedTools: string[];
  referenceTracker: boolean;
  configureMcp: 'Enabled' | 'Disabled' | string;
  telemetryContentCollection: boolean;
  telemetryUsageAnalytics: boolean;
  telemetryEditStats: boolean;
  telemetryFeedback: boolean;
  cliLaunchModel: string;
  cliLaunchTrustAllTools: boolean;
  cliLaunchAgent: string;
  cliLaunchExtraArgs: string;
}

interface AppSettingsContextValue {
  settings: AppSettings | null;
  loading: boolean;
  updateSettings: (updates: Partial<AppSettings>) => Promise<AppSettings | null>;
  reload: () => Promise<void>;
}

const AppSettingsContext = createContext<AppSettingsContextValue | null>(null)

// 默认设置
const DEFAULT_SETTINGS: AppSettings = {
  lockModel: false,
  lockedModel: null,
  autoRefresh: true,
  autoRefreshInterval: 50,
  autoChangeMachineId: true,
  bindMachineIdToAccount: true,
  browserPath: '',
  privacyMode: true,
  autoSwitchEnabled: false,
  autoSwitchThreshold: 1,
  autoSwitchInterval: 5,
  switchTarget: 'ide',
  enableCodebaseIndexing: true,
  enableTabAutocomplete: true,
  usageSummary: true,
  enableDebugLogs: false,
  notifyActionRequired: true,
  notifyFailure: true,
  notifySuccess: true,
  notifyBilling: true,
  trustedTools: [],
  referenceTracker: false,
  configureMcp: 'Enabled',
  telemetryContentCollection: false,
  telemetryUsageAnalytics: false,
  telemetryEditStats: false,
  telemetryFeedback: false,
  cliLaunchModel: 'claude-sonnet-4.5',
  cliLaunchTrustAllTools: false,
  cliLaunchAgent: '',
  cliLaunchExtraArgs: ''
}

export function AppSettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings | null>(null)
  const [loading, setLoading] = useState(true)
  const mountedRef = useRef(true)

  // 加载设置
  const loadSettings = useCallback(async () => {
    try {
      const appSettings = await invoke<AppSettings>('get_app_settings')
      if (mountedRef.current) {
        setSettings(appSettings || DEFAULT_SETTINGS)
      }
    } catch (err) {
      console.error('[AppSettings] 加载失败:', err)
      if (mountedRef.current) {
        setSettings(DEFAULT_SETTINGS)
      }
    } finally {
      if (mountedRef.current) {
        setLoading(false)
      }
    }
  }, [])

  // 更新设置
  const updateSettings = async (updates: Partial<AppSettings>) => {
    try {
      await invoke('save_app_settings', { settings: updates })
      let nextSettings: AppSettings | null = null
      setSettings(prev => {
        nextSettings = { ...(prev || DEFAULT_SETTINGS), ...updates }
        return nextSettings
      })
      return nextSettings
    } catch (err) {
      console.error('[AppSettings] 保存失败:', err)
      return null
    }
  }

  useEffect(() => {
    mountedRef.current = true
    loadSettings()

    let unlisten: UnlistenFn | null = null
    let mounted = true

    const setupListener = async () => {
      listen<AppSettings | null>('app-settings-changed', (event) => {
        if (!mounted) return
        if (event.payload) {
          setSettings(event.payload)
        } else {
          loadSettings()
        }
      }).then(fn => {
        if (mounted) {
          unlisten = fn
        } else {
          fn()
        }
      })
    }

    setupListener()

    return () => {
      mounted = false
      mountedRef.current = false
      if (unlisten) unlisten()
    }
  }, [loadSettings])

  return (
    <AppSettingsContext.Provider value={{ settings, loading, updateSettings, reload: loadSettings }}>
      {children}
    </AppSettingsContext.Provider>
  )
}

export function useAppSettings() {
  const context = useContext(AppSettingsContext)
  if (context === null) {
    throw new Error('useAppSettings must be used within AppSettingsProvider')
  }
  return context
}

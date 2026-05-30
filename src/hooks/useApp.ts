import { useTheme } from 'next-themes'
import { useDialog } from '../contexts/DialogContext'
import { useAppSettings } from '../contexts/AppSettingsContext'
import { t, type TFunc } from '../i18n'

interface UseAppReturn {
    t: TFunc;
    theme: string;
    resolvedTheme: string | undefined;
    setTheme: (theme: string) => void;
    settings: any; // 暂定为 any，后续由 AppSettingsContext 深度推导
    updateSettings: (updates: any) => Promise<any>;
    settingsLoading: boolean;
    dialog: any;
}

/**
 * 核心 Hook：整合全站最常用的工具
 * 为迁移 TS，增加了基础类型定义
 */
export function useApp() {
  const { theme, setTheme, resolvedTheme } = useTheme()
  const dialog = useDialog()
  const { settings, updateSettings, loading: settingsLoading } = useAppSettings()

  return {
    // 基础
    t,
    
    // 主题
    theme: theme || 'dark',
    resolvedTheme,
    setTheme,
    
    // 设置
    settings,
    updateSettings,
    settingsLoading,
    
    // 弹窗
    dialog
  }
}

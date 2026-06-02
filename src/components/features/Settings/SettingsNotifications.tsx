import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Bell, ShieldQuestion } from 'lucide-react'
import { useApp } from '../../../hooks/useApp'
import { useDialog } from '../../../contexts/DialogContext'
import { useAppSettings } from '../../../contexts/AppSettingsContext'
import { NOTIFICATION_SETTINGS_FIELD_MAP } from './settingsConstants'
import SectionCard from './SectionCard'
import ToggleRow from './ToggleRow'

interface NotificationState {
  notifyActionRequired: boolean
  notifyFailure: boolean
  notifySuccess: boolean
  notifyBilling: boolean
}

interface TelemetryState {
  telemetryContentCollection: boolean
  telemetryUsageAnalytics: boolean
  telemetryEditStats: boolean
  telemetryFeedback: boolean
}

const DEFAULT_NOTIFICATIONS: NotificationState = {
  notifyActionRequired: true,
  notifyFailure: true,
  notifySuccess: true,
  notifyBilling: true,
}

const DEFAULT_TELEMETRY: TelemetryState = {
  telemetryContentCollection: false,
  telemetryUsageAnalytics: false,
  telemetryEditStats: false,
  telemetryFeedback: false,
}

function SettingsNotifications() {
  const { t } = useApp()
  const { showError } = useDialog()
  const { updateSettings: updateAppSettings } = useAppSettings()

  const [notifications, setNotifications] = useState<NotificationState>(DEFAULT_NOTIFICATIONS)
  const [telemetry, setTelemetry] = useState<TelemetryState>(DEFAULT_TELEMETRY)
  const mountedRef = useRef(true)

  useEffect(() => {
    let cancelled = false
    mountedRef.current = true

    invoke<any>('get_kiro_settings').then(s => {
      if (cancelled || !s) return
      setNotifications({
        notifyActionRequired: s.notifyActionRequired ?? true,
        notifyFailure: s.notifyFailure ?? true,
        notifySuccess: s.notifySuccess ?? true,
        notifyBilling: s.notifyBilling ?? true,
      })
      setTelemetry({
        telemetryContentCollection: s.telemetryContentCollection ?? false,
        telemetryUsageAnalytics: s.telemetryUsageAnalytics ?? false,
        telemetryEditStats: s.telemetryEditStats ?? false,
        telemetryFeedback: s.telemetryFeedback ?? false,
      })
    }).catch((err) => {
      console.error('加载通知设置失败:', err)
    })
    return () => {
      cancelled = true
      mountedRef.current = false
    }
  }, [])

  const showSaveError = async (err: unknown) => {
    if (mountedRef.current) {
      await showError(t('settings.saveFailed'), `${t('settings.saveFailed')}: ${err}`)
    }
  }

  const handleNotificationChange = async (key: string, checked: boolean, field: keyof NotificationState) => {
    const previous = notifications[field]
    setNotifications(prev => ({ ...prev, [field]: checked }))
    try {
      await invoke('set_kiro_notification', { key, enabled: checked })
      const appField = (NOTIFICATION_SETTINGS_FIELD_MAP as any)[key]
      if (appField) {
        await updateAppSettings({ [appField]: checked })
      }
    } catch (err) {
      if (mountedRef.current) {
        setNotifications(prev => ({ ...prev, [field]: previous }))
      }
      await showSaveError(err)
    }
  }

  const handleTelemetryChange = async (ideKey: string, checked: boolean, field: keyof TelemetryState) => {
    const previous = telemetry[field]
    setTelemetry(prev => ({ ...prev, [field]: checked }))
    try {
      await invoke('set_kiro_telemetry', { key: ideKey, enabled: checked })
      await updateAppSettings({ [field]: checked })
    } catch (err) {
      if (mountedRef.current) {
        setTelemetry(prev => ({ ...prev, [field]: previous }))
      }
      await showSaveError(err)
    }
  }

  const notificationRows: { key: string; label: string; field: keyof NotificationState }[] = [
    { key: 'kiroAgent.notifications.agent.actionRequired', label: 'settings.notifyActionRequired', field: 'notifyActionRequired' },
    { key: 'kiroAgent.notifications.agent.failure', label: 'settings.notifyFailure', field: 'notifyFailure' },
    { key: 'kiroAgent.notifications.agent.success', label: 'settings.notifySuccess', field: 'notifySuccess' },
    { key: 'kiroAgent.notifications.billing', label: 'settings.notifyBilling', field: 'notifyBilling' },
  ]

  const telemetryRows: { ideKey: string; label: string; field: keyof TelemetryState }[] = [
    { ideKey: 'telemetry.dataSharingAndPromptLogging.contentCollectionForServiceImprovement', label: 'settings.telemetryContentCollection', field: 'telemetryContentCollection' },
    { ideKey: 'telemetry.dataSharingAndPromptLogging.usageAnalyticsAndPerformanceMetrics', label: 'settings.telemetryUsageAnalytics', field: 'telemetryUsageAnalytics' },
    { ideKey: 'telemetry.editStats.enabled', label: 'settings.telemetryEditStats', field: 'telemetryEditStats' },
    { ideKey: 'telemetry.feedback.enabled', label: 'settings.telemetryFeedback', field: 'telemetryFeedback' },
  ]

  return (
    <div className="space-y-3">
      {/* 通知设置 */}
      <SectionCard
        title={t('settings.notifications')}
        accent="blue"
        icon={<Bell size={14} className="text-blue-500" />}
        desc={t('settings.notificationsDesc')}
      >
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          {notificationRows.map(row => (
            <ToggleRow
              key={row.key}
              checked={notifications[row.field]}
              onChange={checked => handleNotificationChange(row.key, checked, row.field)}
              label={t(row.label)}
            />
          ))}
        </div>
      </SectionCard>

      {/* 遥测与隐私 */}
      <SectionCard
        title={t('settings.telemetry')}
        accent="orange"
        icon={<ShieldQuestion size={14} className="text-orange-500" />}
        desc={t('settings.telemetryDesc')}
      >
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          {telemetryRows.map(row => (
            <ToggleRow
              key={row.ideKey}
              checked={telemetry[row.field]}
              onChange={checked => handleTelemetryChange(row.ideKey, checked, row.field)}
              label={t(row.label)}
            />
          ))}
        </div>
      </SectionCard>
    </div>
  )
}

export default SettingsNotifications

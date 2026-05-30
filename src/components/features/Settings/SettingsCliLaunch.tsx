import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Terminal, Check, RefreshCw, Cpu, ShieldCheck } from 'lucide-react'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../../ui/select'
import SectionCard from './SectionCard'
import CliCommandPreview from './CliCommandPreview'
import { useAccount } from '../../../contexts/AccountContext'

interface Props {
  model: string
  trustAllTools: boolean
  onChange: (updates: Record<string, any>) => void
  t: (key: string) => string
}

function SettingsCliLaunch({ model, trustAllTools, onChange, t }: Props) {
  const { currentAccount } = useAccount()
  const [models, setModels] = useState<{ modelId: string; modelName?: string }[]>([])
  const [loading, setLoading] = useState(false)
  const [refreshed, setRefreshed] = useState(false)

  const fetchModels = async (forceRefresh = false) => {
    if (!currentAccount?.id) return
    setLoading(true)
    const startedAt = Date.now()
    try {
      const resp = await invoke<any>('list_available_models', { id: currentAccount.id, forceRefresh })
      setModels(Array.isArray(resp?.availableModels) ? resp.availableModels : [])
      if (forceRefresh) {
        setRefreshed(true)
        setTimeout(() => setRefreshed(false), 1600)
      }
    } catch (e) { console.error('拉取模型列表失败:', e) } finally {
      const elapsed = Date.now() - startedAt
      setTimeout(() => setLoading(false), Math.max(0, 500 - elapsed))
    }
  }

  useEffect(() => { fetchModels(false) }, [currentAccount?.id])

  const currentModelName = models.find((m) => m.modelId === model)?.modelName

  return (
    <SectionCard icon={<Terminal size={16} className="text-cyan-500" />} title={t('settings.cliLaunch')}>
      <p className="text-xs text-muted-foreground mb-4">{t('settings.cliLaunchDesc')}</p>

      {/* 模型卡片 */}
      <div className="rounded-xl border border-border bg-gradient-to-br from-cyan-500/[0.06] to-transparent p-3.5 mb-3 transition-colors hover:border-cyan-500/30">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-cyan-500/12 text-cyan-500">
            <Cpu size={18} />
          </div>
          <div className="flex min-w-0 flex-1 flex-col">
            <span className="text-sm font-semibold text-foreground">{t('settings.cliModel')}</span>
            <span className="truncate text-[11px] font-mono text-muted-foreground">{model || '—'}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <Select value={model} onValueChange={(v) => onChange({ cliLaunchModel: v })}>
              <SelectTrigger className="h-8 w-[200px] text-xs">
                <SelectValue placeholder={currentModelName || model || '—'} />
              </SelectTrigger>
              <SelectContent>
                {models.length === 0 ? (
                  <div className="px-2 py-1.5 text-xs text-muted-foreground">
                    {currentAccount?.id ? t('settings.cliNoModels') : t('settings.cliNoAccount')}
                  </div>
                ) : models.map((m) => (
                  <SelectItem key={m.modelId} value={m.modelId}>{m.modelName || m.modelId}</SelectItem>
                ))}
              </SelectContent>
            </Select>
            <button
              onClick={() => fetchModels(true)}
              disabled={loading || !currentAccount?.id}
              title={t('settings.cliRefreshModels')}
              className={`cursor-pointer shrink-0 flex h-8 w-8 items-center justify-center rounded-lg border transition-all active:scale-90 disabled:opacity-50 disabled:cursor-not-allowed ${
                refreshed
                  ? 'border-green-500/40 bg-green-500/15 text-green-500'
                  : 'border-border text-muted-foreground hover:border-cyan-500/40 hover:bg-cyan-500/10 hover:text-cyan-500'
              }`}
            >
              {refreshed ? <Check size={14} className="animate-in zoom-in duration-200" /> : <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />}
            </button>
          </div>
        </div>
      </div>

      {/* 信任所有工具 */}
      <button
        type="button"
        onClick={() => onChange({ cliLaunchTrustAllTools: !trustAllTools })}
        className={`mb-3 flex w-full items-center gap-3 rounded-xl border p-3.5 text-left transition-all ${
          trustAllTools ? 'border-amber-500/40 bg-amber-500/[0.06]' : 'border-border hover:border-border/80 hover:bg-muted/30'
        }`}
      >
        <div className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-lg transition-colors ${trustAllTools ? 'bg-amber-500/15 text-amber-500' : 'bg-muted text-muted-foreground'}`}>
          <ShieldCheck size={18} />
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="text-sm font-semibold text-foreground">{t('settings.cliTrustAllTools')}</span>
          <span className="text-[11px] text-muted-foreground">{t('settings.cliTrustAllToolsDesc')}</span>
        </div>
        <div className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${trustAllTools ? 'bg-amber-500' : 'bg-input'}`}>
          <span className={`absolute top-0.5 h-4 w-4 rounded-full bg-background shadow-sm transition-transform ${trustAllTools ? 'translate-x-4' : 'translate-x-0.5'}`} />
        </div>
      </button>

      {/* 命令预览 */}
      <CliCommandPreview />
    </SectionCard>
  )
}

export default SettingsCliLaunch

import { useMemo, useState } from 'react'
import { Copy, Check } from 'lucide-react'
import { useAppSettings } from '../../../contexts/AppSettingsContext'
import { useApp } from '../../../hooks/useApp'

function CliCommandPreview({ className = '' }: { className?: string }) {
  const { settings } = useAppSettings()
  const { t } = useApp()
  const [copied, setCopied] = useState(false)

  const model = settings.cliLaunchModel || ''
  const trustAllTools = !!settings.cliLaunchTrustAllTools

  const command = useMemo(() => {
    let cmd = 'kiro-cli chat'
    if (model) cmd += ` --model ${model}`
    if (trustAllTools) cmd += ' --trust-all-tools'
    return cmd
  }, [model, trustAllTools])

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch { /* ignore */ }
  }

  return (
    <div className={`overflow-hidden rounded-xl border border-border bg-muted/40 ${className}`}>
      <div className="flex items-center gap-1.5 border-b border-border px-3 py-1.5">
        <span className="h-2.5 w-2.5 rounded-full bg-red-400/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-yellow-400/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-green-400/70" />
        <span className="ml-1.5 text-[10px] font-medium text-muted-foreground">kiro-cli</span>
        <button
          onClick={copy}
          className={`ml-auto cursor-pointer flex items-center gap-1 rounded px-2 py-0.5 text-[11px] font-medium transition-all active:scale-90 ${
            copied ? 'bg-green-500/20 text-green-600 dark:text-green-400' : 'bg-cyan-500/15 text-cyan-600 dark:text-cyan-400 hover:bg-cyan-500/25'
          }`}
        >
          {copied ? <Check size={12} className="animate-in zoom-in duration-200" /> : <Copy size={12} />}
          {copied ? t('common.copied') : t('common.copy')}
        </button>
      </div>
      <div className="px-3 py-2.5">
        <code className="text-xs font-mono break-all leading-relaxed">
          <span className="text-green-600 dark:text-green-400">$ </span>
          <span className="text-foreground">kiro-cli chat</span>
          {model && <span className="text-cyan-600 dark:text-cyan-400"> --model </span>}{model && <span className="text-amber-600 dark:text-amber-300">{model}</span>}
          {trustAllTools && <span className="text-cyan-600 dark:text-cyan-400"> --trust-all-tools</span>}
        </code>
      </div>
    </div>
  )
}

export default CliCommandPreview

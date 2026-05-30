import type React from 'react'
import { Switch } from '../../ui/switch'

interface SwitchRowProps {
  checked: boolean
  onCheckedChange: (v: boolean) => void
  icon?: React.ReactNode
  label: string
  hint?: string
  trailing?: React.ReactNode
  title?: string
}

/**
 * 紧凑开关行：左侧 switch + 图标 + 标签，右侧可选附加控件（select / 按钮）。
 * 比 ToggleRow 多支持图标、副标题、尾控件，用于配置项较丰富的场景。
 */
function SwitchRow({
  checked,
  onCheckedChange,
  icon,
  label,
  hint,
  trailing,
  title,
}: SwitchRowProps) {
  return (
    <div
      className={`group/row relative flex items-center gap-2.5 rounded-xl border px-3 py-2.5 transition-all duration-200 ${
        checked
          ? 'border-primary/30 bg-primary/[0.06] hover:bg-primary/[0.09]'
          : 'border-border bg-card hover:border-border/80 hover:bg-muted/40'
      }`}
      title={title}
    >
      {checked && <span className="absolute left-0 top-1/2 h-4 w-0.5 -translate-y-1/2 rounded-full bg-primary" />}
      <Switch checked={checked} onCheckedChange={onCheckedChange} />
      {icon && (
        <span className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-lg transition-colors ${
          checked ? 'bg-primary/15 text-primary' : 'bg-muted text-muted-foreground group-hover/row:text-foreground'
        }`}>{icon}</span>
      )}
      <span className="text-sm font-medium text-foreground">{label}</span>
      {hint && <span className="text-xs text-muted-foreground ml-0.5">{hint}</span>}
      {trailing && <div className="ml-auto flex items-center gap-2">{trailing}</div>}
    </div>
  )
}

export default SwitchRow

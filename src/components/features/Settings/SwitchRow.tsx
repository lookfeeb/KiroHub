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
 * 配置开关行：左侧图标徽章 + 标签/说明，右侧可选附加控件 + 开关。
 * 启用态以柔和主色高亮整行。
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
      className={`group/row flex items-center gap-3 rounded-xl border px-3 py-2.5 transition-all duration-200 ${
        checked
          ? 'border-primary/25 bg-primary/[0.05] hover:bg-primary/[0.08]'
          : 'border-border bg-card hover:border-border/80 hover:bg-muted/40'
      }`}
      title={title}
    >
      {icon && (
        <span className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-lg transition-colors ${
          checked ? 'bg-primary/15 text-primary' : 'bg-muted text-muted-foreground group-hover/row:text-foreground'
        }`}>{icon}</span>
      )}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-sm font-medium text-foreground">{label}</span>
          {hint && <span className="text-[11px] text-muted-foreground">{hint}</span>}
        </div>
      </div>
      {trailing && <div className="flex items-center gap-2 shrink-0">{trailing}</div>}
      <Switch checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  )
}

export default SwitchRow

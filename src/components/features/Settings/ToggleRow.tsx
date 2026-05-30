import { Switch } from '../../ui/switch'

interface ToggleRowProps {
  checked: boolean
  onChange: (v: boolean) => Promise<void> | void
  label: string
}

/**
 * 紧凑布尔配置行：标签左、开关右，启用态柔和主色高亮。与 SwitchRow 风格统一。
 */
function ToggleRow({ checked, onChange, label }: ToggleRowProps) {
  return (
    <label className={`group/row flex items-center gap-2.5 cursor-pointer px-3 py-2.5 rounded-xl border transition-all duration-200 ${
      checked
        ? 'border-primary/25 bg-primary/[0.05] hover:bg-primary/[0.08]'
        : 'border-border bg-card hover:border-border/80 hover:bg-muted/40'
    }`}>
      <span className={`flex-1 min-w-0 text-xs ${checked ? 'text-foreground font-medium' : 'text-foreground'}`}>{label}</span>
      <Switch checked={checked} onCheckedChange={onChange} />
    </label>
  )
}

export default ToggleRow

import { Card } from '@/components/ui/data-display/card'
import { LucideIcon } from 'lucide-react'

interface StatCardProps {
  icon: LucideIcon
  iconBg: string
  iconColor: string
  value: string | number
  label: string
  delay: string
  onClick?: () => void
  warning?: boolean
}

/** 按既有 iconBg 推断该卡的语义强调色，驱动图标 / 光晕 / hover 边框统一。 */
const ACCENTS: Record<string, { icon: string; glow: string; shadow: string; dot: string }> = {
  info: { icon: 'text-blue-500', glow: 'from-blue-500/15', shadow: 'group-hover/stat:shadow-blue-500/25', dot: 'bg-blue-500' },
  success: { icon: 'text-emerald-500', glow: 'from-emerald-500/15', shadow: 'group-hover/stat:shadow-emerald-500/25', dot: 'bg-emerald-500' },
  purple: { icon: 'text-purple-500', glow: 'from-purple-500/15', shadow: 'group-hover/stat:shadow-purple-500/25', dot: 'bg-purple-500' },
  warning: { icon: 'text-orange-500', glow: 'from-orange-500/15', shadow: 'group-hover/stat:shadow-orange-500/25', dot: 'bg-orange-500' },
  cyan: { icon: 'text-cyan-500', glow: 'from-cyan-500/15', shadow: 'group-hover/stat:shadow-cyan-500/25', dot: 'bg-cyan-500' },
}

function resolveAccent(iconBg: string) {
  if (iconBg.includes('info')) return ACCENTS.info
  if (iconBg.includes('success')) return ACCENTS.success
  if (iconBg.includes('warning')) return ACCENTS.warning
  if (iconBg.includes('purple')) return ACCENTS.purple
  if (iconBg.includes('cyan')) return ACCENTS.cyan
  return ACCENTS.info
}

/** 统计卡片：渐变光晕 + 语义色徽章 + 平滑 hover。 */
function StatCard({ icon: Icon, iconBg, value, label, delay, onClick, warning }: StatCardProps) {
  const a = warning ? ACCENTS.warning : resolveAccent(iconBg)
  return (
    <Card
      onClick={onClick}
      className={`group/stat relative isolate overflow-hidden card-glow animate-scale-in ${delay} rounded-xl border border-border/40 shadow-sm transition-all duration-300 ease-out hover:-translate-y-1 hover:shadow-lg ${a.shadow} ${onClick ? 'cursor-pointer active:translate-y-0' : ''} ${warning ? 'border-orange-500/30' : ''}`}
    >
      {/* 渐变光晕：hover 时增强 */}
      <div className={`pointer-events-none absolute inset-0 -z-10 bg-gradient-to-br ${a.glow} via-transparent to-transparent opacity-60 transition-opacity duration-300 group-hover/stat:opacity-100`} />
      {/* 顶部高光描边 */}
      <div className="pointer-events-none absolute inset-x-0 top-0 -z-10 h-px bg-gradient-to-r from-transparent via-white/25 to-transparent" />

      <div className="flex items-center gap-3 p-3.5">
        <div className={`relative w-10 h-10 ${iconBg} rounded-xl flex items-center justify-center flex-shrink-0 shadow-sm ring-1 ring-inset ring-white/10 transition-all duration-300 group-hover/stat:scale-110 group-hover/stat:shadow-md`}>
          <Icon size={17} className={`${a.icon} transition-transform duration-300 group-hover/stat:-rotate-6`} />
          {warning && (
            <span className={`absolute -top-1 -right-1 w-2.5 h-2.5 ${a.dot} rounded-full animate-pulse ring-2 ring-background`} />
          )}
        </div>
        <div className="flex flex-col min-w-0">
          <div className="text-lg font-bold stat-number text-foreground leading-none tabular-nums tracking-tight transition-colors duration-300">{value}</div>
          <div className="mt-1 text-[11px] font-medium text-muted-foreground truncate">{label}</div>
        </div>
      </div>
    </Card>
  )
}

export default StatCard

import type React from 'react'
import { Card, CardContent } from '../../ui/card'

export type SectionAccent = 'primary' | 'orange' | 'violet' | 'blue' | 'green' | 'red' | 'amber'

const ACCENT_CLASS: Record<SectionAccent, string> = {
  primary: 'bg-primary',
  orange: 'bg-orange-500',
  violet: 'bg-violet-500',
  blue: 'bg-blue-500',
  green: 'bg-emerald-500',
  red: 'bg-red-500',
  amber: 'bg-amber-500',
}

interface SectionCardProps {
  title: string
  icon?: React.ReactNode
  badge?: React.ReactNode
  desc?: string
  accent?: SectionAccent
  className?: string
  children: React.ReactNode
}

/**
 * 紧凑分组卡片：彩色短竖条 + 可选图标 + 标题 + 可选 badge / 描述。
 * 用于 Settings 各 tab 的统一分组容器。
 */
function SectionCard({
  title,
  icon,
  badge,
  desc,
  accent = 'primary',
  className = '',
  children,
}: SectionCardProps) {
  return (
    <Card className={`card-glow border-border/70 transition-all duration-200 hover:border-border ${className}`}>
      <CardContent className="p-4 space-y-3">
        <div className="flex items-center gap-2 border-b border-border/50 pb-2.5">
          <div className={`w-1 h-4 ${ACCENT_CLASS[accent]} rounded-full`} />
          {icon}
          <h2 className="text-sm font-semibold text-foreground">{title}</h2>
          {badge && <span className="ml-auto flex items-center">{badge}</span>}
        </div>
        {desc && <p className="text-xs text-muted-foreground -mt-1">{desc}</p>}
        {children}
      </CardContent>
    </Card>
  )
}

export default SectionCard

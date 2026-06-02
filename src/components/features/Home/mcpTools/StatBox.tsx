import type { LucideIcon } from 'lucide-react'

interface StatBoxProps {
  icon: LucideIcon;
  label: string;
  value: number;
  color: string;
}

function StatBox({ icon: Icon, label, value, color }: StatBoxProps) {
  return (
    <div className="flex items-center gap-2.5 rounded-xl border border-border/60 bg-gradient-to-br from-muted/40 to-muted/10 p-2.5">
      <span className={`flex h-8 w-8 items-center justify-center rounded-lg ${color}`}><Icon size={15} /></span>
      <div className="min-w-0">
        <div className="text-lg font-bold text-foreground leading-none">{value}</div>
        <div className="text-[10px] text-muted-foreground mt-1 truncate">{label}</div>
      </div>
    </div>
  )
}

export default StatBox

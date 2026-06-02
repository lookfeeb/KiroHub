import { Progress } from '@/components/ui/feedback/progress'
import React from 'react'

interface MetricBarProps {
  label: string
  count: number
  percent: string
  className?: string
  isError?: boolean
}

export function MetricBar({ label, count, percent, className = '', isError = false }: MetricBarProps) {
  return (
    <div className={`flex items-center gap-2 ${className}`}>
      <span className="text-xs truncate flex-1">{label}</span>
      <span className="text-xs font-mono text-muted-foreground shrink-0">{count}</span>
      <span className="text-xs font-semibold shrink-0 w-12 text-right">{percent}</span>
      <Progress
        value={parseFloat(percent)}
        className={`h-1 w-20 shrink-0 ${isError ? '[&>div]:bg-red-600' : ''}`}
      />
    </div>
  )
}

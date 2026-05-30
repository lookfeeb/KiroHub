import { getProviderDisplayName, isGitHubProvider } from '../../utils/accountProvider'

// 统一的 Provider 配色（一处维护）
export function getProviderTone(provider?: string): string {
  if (provider === 'Google') return 'text-red-500'
  if (provider && isGitHubProvider(provider)) return 'text-foreground'
  if (provider === 'BuilderId') return 'text-orange-500'
  if (provider === 'Enterprise') return 'text-amber-500'
  return 'text-muted-foreground'
}

/** 统一的 Provider 徽章：彩色圆点 + 名称，颜色按来源区分 */
function ProviderBadge({ provider, className = '' }: { provider?: string; className?: string }) {
  return (
    <span className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-muted/50 border border-border/50 text-[11px] font-medium ${getProviderTone(provider)} ${className}`}>
      <span className="w-1.5 h-1.5 rounded-full bg-current" />
      {getProviderDisplayName(provider) || '未知'}
    </span>
  )
}

export default ProviderBadge

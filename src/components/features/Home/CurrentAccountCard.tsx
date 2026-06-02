import { Card, CardContent, CardHeader } from '@/components/ui/data-display/card'
import { Badge } from '@/components/ui/data-display/badge'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/overlays/tooltip'
import { TooltipIconButton } from '@/components/ui/actions/tooltip-icon-button'
import { RefreshCw, Users, Clock } from 'lucide-react'
import { useApp } from '../../../hooks/useApp'
import { useMemo } from 'react'
import { getThemeAccent } from '../KiroConfig/themeAccent'
import { getProviderDisplayName, isGitHubProvider } from '../../../utils/accountProvider'
import React from 'react'

interface CurrentAccountCardProps {
  localToken: any;
  refreshing: boolean;
  handleRefresh: () => void;
  colors: any;
  t: any;
}

// 当前账号卡片
function CurrentAccountCard({ 
  localToken, 
  refreshing, 
  handleRefresh, 
  colors, 
  t 
}: CurrentAccountCardProps) {
  const { theme } = useApp()
  const accent = useMemo(() => getThemeAccent(theme), [theme])

  return (
    <Card className="card-glow animate-scale-in delay-300">
      <CardHeader className={`flex flex-row items-center justify-between space-y-0 pb-3 border-b border-border`}>
        <span className={`font-semibold text-foreground`}>{t('home.currentAccount')}</span>
        <TooltipIconButton
          onClick={handleRefresh}
          disabled={refreshing}
          tooltip={t('common.refresh')}
          className={`group/button inline-flex size-8 shrink-0 items-center justify-center rounded-lg border border-transparent bg-clip-padding text-sm font-medium whitespace-nowrap transition-all outline-none select-none hover:bg-muted hover:text-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 active:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-50 aria-expanded:bg-muted aria-expanded:text-foreground dark:hover:bg-muted/50 ${refreshing ? 'spinning' : ''}`}
        >
          <RefreshCw size={16} className={"text-muted-foreground"} />
        </TooltipIconButton>
      </CardHeader>

      <CardContent className="pt-6">
        {localToken ? (
          <div className="flex items-center gap-4 group relative">
            <div className={`w-14 h-14 rounded-2xl flex items-center justify-center text-white font-bold text-xl shadow-lg transition-transform hover:scale-105 flex-shrink-0 ${
              localToken.provider === 'Google' ? 'bg-gradient-to-br from-red-500 to-orange-500 shadow-red-500/25' :
              isGitHubProvider(localToken.provider) ? 'bg-gradient-to-br from-gray-700 to-gray-900 shadow-gray-500/25' :
              `bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} ${accent.shadow}`
            }`}>
              {localToken.provider?.[0] || 'K'}
            </div>

            <div className="flex flex-col gap-1 flex-1">
              <div className="flex items-center gap-2">
                <span className={`font-semibold text-lg text-foreground`}>
                  {getProviderDisplayName(localToken.provider) || t('home.unknown')}
                </span>
                <Badge variant="default" className="pulse-ring bg-green-500/10 text-green-600 dark:text-green-400">
                  {t('home.loggedIn')}
                </Badge>
              </div>
              <span className={`text-sm text-muted-foreground`}>{localToken.authMethod || 'social'}</span>
            </div>

            {/* Hover 显示 Token 详情 */}
            <TokenDetailPopover localToken={localToken} colors={colors} t={t} />
          </div>
        ) : (
          <div className="flex flex-col items-center gap-2 py-8">
            <div className={`w-16 h-16 rounded-full flex items-center justify-center animate-float bg-muted/30`}>
              <Users size={28} className={"text-muted-foreground"} />
            </div>
            <span className={`text-muted-foreground font-medium`}>{t('home.notLoggedIn')}</span>
            <span className={`text-sm text-muted-foreground`}>{t('home.clickToSwitch')}</span>
          </div>
        )}
      </CardContent>
    </Card>
  )
}

interface TokenDetailPopoverProps {
  localToken: any;
  colors: any;
  t: any;
}

// Token 详情悬浮框
function TokenDetailPopover({ localToken, colors, t }: TokenDetailPopoverProps) {
  return (
    <Card className="absolute left-16 top-0 w-72 z-50 opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all duration-200 pointer-events-none shadow-xl">
      <CardContent className="p-3 space-y-2">
        <div className="flex justify-between items-center">
          <span className={`text-xs text-muted-foreground`}>Access Token</span>
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className={`text-xs font-mono truncate text-muted-foreground max-w-[140px] cursor-help`}>
                  {localToken.accessToken?.substring(0, 12)}...
                </span>
              </TooltipTrigger>
              <TooltipContent>
                <p className="font-mono text-xs">{localToken.accessToken}</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>

        <div className="flex justify-between items-center">
          <span className={`text-xs text-muted-foreground`}>Refresh Token</span>
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className={`text-xs font-mono truncate text-muted-foreground max-w-[140px] cursor-help`}>
                  {localToken.refreshToken?.substring(0, 12)}...
                </span>
              </TooltipTrigger>
              <TooltipContent>
                <p className="font-mono text-xs">{localToken.refreshToken}</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>

        {localToken.authMethod === 'IdC' ? (
          <>
            <div className="flex justify-between items-center">
              <span className={`text-xs text-muted-foreground`}>Client ID Hash</span>
              <span className={`text-xs font-mono truncate text-muted-foreground max-w-[140px]`}>
                {localToken.clientIdHash || '-'}
              </span>
            </div>
            <div className="flex justify-between items-center">
              <span className={`text-xs text-muted-foreground`}>Region</span>
              <span className={`text-xs font-mono text-muted-foreground`}>{localToken.region || '-'}</span>
            </div>
          </>
        ) : (
          <div className="flex justify-between items-center">
            <span className={`text-xs text-muted-foreground`}>Profile ARN</span>
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className={`text-xs font-mono truncate text-muted-foreground max-w-[140px] cursor-help`}>
                    {localToken.profileArn || '-'}
                  </span>
                </TooltipTrigger>
                <TooltipContent>
                  <p className="font-mono text-xs">{localToken.profileArn}</p>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          </div>
        )}

        <div className="flex justify-between items-center">
          <span className={`text-xs text-muted-foreground`}>{t('home.expiresAt')}</span>
          <div className="flex items-center gap-1">
            <Clock size={10} />
            <span className={`text-xs text-foreground`}>
              {localToken.expiresAt ? new Date(localToken.expiresAt).toLocaleString() : t('home.unknown')}
            </span>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}

export default CurrentAccountCard

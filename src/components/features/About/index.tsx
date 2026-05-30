import { useState, useEffect, useMemo, useCallback } from 'react'
import { Code2, Palette, Cpu, RefreshCw } from 'lucide-react'
import { getVersion } from '@tauri-apps/api/app'
import { check } from '@tauri-apps/plugin-updater'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { useApp } from '../../../hooks/useApp'
import { useDialog } from '../../../contexts/DialogContext'
import { getThemeAccent } from '../KiroConfig/themeAccent'
import ChangelogDialog from '../../shared/ChangelogDialog'

// Logo（带光晕与高光环）
const AppLogo = ({ accent }: { accent: any }) => (
  <div className="relative shrink-0">
    <div className={`absolute inset-0 bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} rounded-2xl blur-lg opacity-60`} />
    <div className={`relative w-16 h-16 bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} rounded-2xl flex items-center justify-center shadow-lg ring-1 ring-white/20`}>
      <svg width="32" height="32" viewBox="0 0 40 40" fill="none">
        <path d="M20 4C12 4 6 10 6 18C6 22 8 25 8 25C8 25 7 28 7 30C7 32 8 34 10 34C11 34 12 33 13 32C14 33 16 34 20 34C24 34 26 33 27 32C28 33 29 34 30 34C32 34 33 32 33 30C33 28 32 25 32 25C32 25 34 22 34 18C34 10 28 4 20 4ZM14 20C12.5 20 11 18.5 11 17C11 15.5 12.5 14 14 14C15.5 14 17 15.5 17 17C17 18.5 15.5 20 14 20ZM26 20C24.5 20 23 18.5 23 17C23 15.5 24.5 14 26 14C27.5 14 29 15.5 29 17C29 18.5 27.5 20 26 20Z" fill="white" />
      </svg>
    </div>
  </div>
)

function About() {
  const { t, theme } = useApp()
  const { showInfo, showUpdate } = useDialog()
  const [version, setVersion] = useState('')
  const [checking, setChecking] = useState(false)
  const [changelog, setChangelog] = useState<{ version: string; body: string } | null>(null)

  const accent = useMemo(() => getThemeAccent(theme), [theme])

  const techStack = useMemo(() => [
    { icon: Code2, value: 'React + Vite', color: 'text-cyan-500 border-cyan-500/25 bg-cyan-500/10' },
    { icon: Palette, value: 'TailwindCSS', color: 'text-sky-500 border-sky-500/25 bg-sky-500/10' },
    { icon: Cpu, value: 'Tauri + Rust', color: 'text-orange-500 border-orange-500/25 bg-orange-500/10' },
  ], [])

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(''))
  }, [])

  const checkUpdate = useCallback(async () => {
    setChecking(true)
    try {
      const update = await check()
      if (update?.available) {
        showUpdate({ version: update.version, body: update.body || '' }, update)
      } else {
        showInfo('检查更新', '当前已是最新版本')
      }
    } catch (e) {
      showInfo('检查更新', '检查更新失败：' + e)
    } finally {
      setChecking(false)
    }
  }, [showUpdate, showInfo])

  const showChangelog = useCallback(async () => {
    if (!version) return
    try {
      const res = await fetch(`https://api.github.com/repos/lookfeeb/KiroHub/releases/tags/v${version}`)
      const data = await res.json()
      setChangelog({ version, body: data?.body || '暂无更新说明' })
    } catch {
      setChangelog({ version, body: '获取更新内容失败' })
    }
  }, [version])

  return (
    <div className="h-full glass-main overflow-auto p-6">
      <div className="space-y-3">
        {/* === 应用介绍卡（横向布局：logo 左，标题/版本/技术栈右）=== */}
        <Card className="card-glow relative overflow-hidden">
          {/* 右上角主题色装饰光晕 */}
          <div className={`pointer-events-none absolute -top-20 -right-16 h-48 w-48 rounded-full bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} opacity-15 blur-3xl`} />
          <CardContent className="relative p-5">
            <div className="flex items-start gap-4">
              <AppLogo accent={accent} />
              <div className="flex-1 min-w-0 space-y-3">
                <div className="flex items-center gap-2 flex-wrap">
                  <h1 className="text-lg font-bold tracking-tight text-foreground">{t('about.appName')}</h1>
                  <Badge
                    variant="default"
                    onClick={showChangelog}
                    className="px-2 py-0 h-5 text-[11px] font-mono cursor-pointer hover:opacity-80"
                  >
                    v{version || '...'}
                  </Badge>
                  <Button
                    onClick={checkUpdate}
                    disabled={checking}
                    variant="outline"
                    size="sm"
                    className="ml-auto h-7 text-xs gap-1"
                  >
                    <RefreshCw size={12} className={checking ? 'animate-spin' : ''} />
                    {checking ? '检查中...' : '检查更新'}
                  </Button>
                </div>

                <p className="text-xs text-muted-foreground leading-relaxed">
                  {t('about.appDesc')}
                </p>

                <div className="flex items-center gap-1.5 flex-wrap pt-0.5">
                  {techStack.map(({ icon: Icon, value, color }) => (
                    <span
                      key={value}
                      className={`inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-[11px] font-medium border transition-transform hover:-translate-y-0.5 ${color}`}
                    >
                      <Icon size={11} />
                      {value}
                    </span>
                  ))}
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {changelog && (
        <ChangelogDialog
          version={changelog.version}
          body={changelog.body}
          onClose={() => setChangelog(null)}
        />
      )}
    </div>
  )
}

export default About

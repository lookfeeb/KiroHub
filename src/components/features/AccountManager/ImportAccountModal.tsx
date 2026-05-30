import { useState, useEffect, useRef, useMemo } from 'react'
import { Tabs, TabsContent, TabsList } from '@/components/ui/tabs'
import { Stack, Group } from '@/components/shared/layout'

import { Progress } from '@/components/ui/progress'
import { Alert } from '@/components/ui/alert'
import { Upload, FileJson, AlertCircle, CheckCircle, Loader2, Database, RefreshCw, LogIn } from 'lucide-react'
import Login from '../Login'
import { invoke } from '@tauri-apps/api/core'
import { useApp } from '../../../hooks/useApp'

import { getConcurrency } from '../../../utils/concurrency'
import { getAccountDisplayName } from '../../../utils/accountStats'
import { isBannedStatus } from '../../../utils/accountStatus'
import { getProviderDisplayName, isGitHubProvider, normalizeProviderId } from '../../../utils/accountProvider'
import {
  DialogRoot,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogBody,
  DialogFooter} from '../../shared/dialog'
import { Button } from '../../shared/button'
import { getThemeAccent } from '../KiroConfig/themeAccent'

interface ImportAccountModalProps {
  onClose: () => void;
  onSuccess?: (data: { added: any[]; updated: any[] }) => void;
  onNavigate?: (path: string) => void;
}

function LegacyButton({ color, leftSection, className = '', children, ...props }: any) {
  const colorClass = color === 'red'
    ? 'text-red-600 hover:text-red-700'
    : color === 'blue'
      ? 'text-blue-600 hover:text-blue-700'
      : color === 'violet' || color === 'grape'
        ? 'text-purple-600 hover:text-purple-700'
        : ''
  return (
    <Button {...props} variant="secondary" size={props.size === 'xs' || props.size === 'sm' ? 'sm' : 'default'} className={`${colorClass} ${className}`.trim()}>
      {leftSection}
      {children}
    </Button>
  )
}

function FileButton({ onChange, accept, children }: any) {
  const inputRef = useRef<HTMLInputElement>(null)
  const triggerProps = { onClick: () => inputRef.current?.click() }
  const handleChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0] || null
    if (file) {
      await onChange(file)
    }
    event.target.value = ''
  }
  return (
    <>
      <input ref={inputRef} type="file" accept={accept} className="hidden" onChange={handleChange} />
      {children(triggerProps)}
    </>
  )
}

function validateAccount(item: any, index: number) {
  const errors = []
  const refreshToken = item.refreshToken
  if (!refreshToken) {
    errors.push(`第 ${index + 1} 条: 缺少 refreshToken`)
    return { valid: false, errors, type: null }
  }

  if (!refreshToken.startsWith('aor')) {
    errors.push(`第 ${index + 1} 条: refreshToken 格式无效（应以 aor 开头）`)
    return { valid: false, errors, type: null }
  }

  const hasClientCredentials = item.clientId && item.clientSecret
  const isIdC = hasClientCredentials
  const isSocial = !hasClientCredentials

  let provider = item.provider
  if (!provider) {
    if (isSocial) {
      // Social 账号必须明确指定 provider
      errors.push(`第 ${index + 1} 条: Social 账号必须指定 provider (Google/Github)`)
      return { valid: false, errors, type: null }
    } else {
      // IdC 账号：通过 startUrl 判断是 Enterprise 还是 BuilderId
      provider = item.startUrl ? 'Enterprise' : 'BuilderId'
    }
  }

  const normalizedProvider = normalizeProviderId(provider)
  const validProviders = ['Google', 'Github', 'BuilderId', 'Enterprise']
  if (!validProviders.includes(normalizedProvider)) {
    errors.push(`第 ${index + 1} 条: provider 必须是 ${validProviders.join('/')}`)
    return { valid: false, errors, type: null }
  }

  if (isSocial && !(normalizedProvider === 'Google' || isGitHubProvider(normalizedProvider))) {
    errors.push(`第 ${index + 1} 条: Social 账号的 provider 应为 Google/Github`)
    return { valid: false, errors, type: null }
  }

  if (isIdC && !['BuilderId', 'Enterprise'].includes(normalizedProvider)) {
    errors.push(`第 ${index + 1} 条: IdC 账号的 provider 应为 BuilderId/Enterprise`)
    return { valid: false, errors, type: null }
  }

  // Enterprise 账号必须提供 region 和 startUrl
  if (normalizedProvider === 'Enterprise') {
    if (!item.region || !item.region.trim()) {
      errors.push(`第 ${index + 1} 条: Enterprise 账号必须提供 region 字段`)
      return { valid: false, errors, type: null }
    }
    if (!item.startUrl || !item.startUrl.trim()) {
      errors.push(`第 ${index + 1} 条: Enterprise 账号必须提供 startUrl 字段`)
      return { valid: false, errors, type: null }
    }
  }

  return { valid: true, errors: [] as string[], type: isSocial ? 'social' : 'idc', inferredProvider: normalizedProvider }
}

function ImportAccountModal({ onClose, onSuccess, onNavigate }: ImportAccountModalProps) {
  const { t, theme } = useApp()
  const accent = useMemo(() => getThemeAccent(theme), [theme])
  const colors = useMemo(() => ({
    inputFocus: 'focus:ring-primary/20 focus:border-primary',
    ringColor: theme === 'dark' ? 'ring-primary/40' : 'ring-primary/20'
  }), [theme])
  const [activeTab, setActiveTab] = useState('json')
  const [, setOsType] = useState('')
  const [jsonText, setJsonText] = useState('')
  const [parseResult, setParseResult] = useState<any>(null)
  const [importing, setImporting] = useState(false)
  const [importProgress, setImportProgress] = useState({ current: 0, total: 0 })
  const [importResult, setImportResult] = useState<any>(null)

  // 从 Kiro 导入相关状态
  const [kiroAccounts, setKiroAccounts] = useState<any[]>([])
  const [kiroLoading, setKiroLoading] = useState(false)
  const [kiroError, setKiroError] = useState<string | null>(null)
  const [kiroImporting, setKiroImporting] = useState(false)
  const [kiroProgress, setKiroProgress] = useState({ current: 0, total: 0 })
  const [kiroResult, setKiroResult] = useState<any>(null)
  const kiroDetectedRef = useRef(false)

  // 从 kiro-cli 导入相关状态
  // 从 kiro-cli 导入相关状态
  const [kiroCliDbPath, setKiroCliDbPath] = useState('')
  const [kiroCliDetected, setKiroCliDetected] = useState(false)
  const [kiroCliDetecting, setKiroCliDetecting] = useState(false)
  const [kiroCliImporting, setKiroCliImporting] = useState(false)
  const [kiroCliResult, setKiroCliResult] = useState<any>(null)

  useEffect(() => {
    let isMounted = true

    const detectOsType = async () => {
      try {
        const info = await invoke<any>('get_system_machine_guid')
        if (isMounted && info?.osType) {
          setOsType(info.osType)
          return
        }
      } catch (_) {
        // ignore and fallback to userAgent detection
      }

      if (!isMounted) return
      const userAgent = (navigator.userAgent || '').toLowerCase()
      if (userAgent.includes('windows')) {
        setOsType('windows')
      } else if (userAgent.includes('mac os') || userAgent.includes('macos')) {
        setOsType('macos')
      } else if (userAgent.includes('linux')) {
        setOsType('linux')
      }
    }

    detectOsType()

    return () => {
      isMounted = false
    }
  }, [])

  // 自动检测 kiro-cli 数据库路径
  useEffect(() => {
    if (activeTab === 'kiro-cli' && !kiroCliDbPath) {
      detectKiroCliPath()
    }
  }, [activeTab, kiroCliDbPath])

  const detectKiroCliPath = async () => {
    setKiroCliDetecting(true)
    try {
      const defaultPath = await invoke<string>('get_kiro_cli_default_path')
      if (defaultPath) {
        setKiroCliDbPath(defaultPath)
        setKiroCliDetected(true)
      } else {
        setKiroCliDetected(false)
      }
    } catch (e) {
      console.error('获取默认路径失败:', e)
      setKiroCliDetected(false)
    } finally {
      setKiroCliDetecting(false)
    }
  }

  // 自动检测 Kiro 账号
  useEffect(() => {
    if (activeTab === 'kiro' && !kiroDetectedRef.current) {
      kiroDetectedRef.current = true
      detectKiroAccounts()
    }
  }, [activeTab])

  const detectKiroAccounts = async () => {
    setKiroLoading(true)
    setKiroError(null)
    try {
      const accounts = await invoke<any[]>('read_kiro_accounts')
      setKiroAccounts(accounts)
    } catch (e) {
      setKiroError(String(e))
      setKiroAccounts([])
    } finally {
      setKiroLoading(false)
    }
  }

  const parseJson = (text: string) => {
    if (!text.trim()) {
      setParseResult(null)
      return
    }

    try {
      let data = JSON.parse(text)
      if (!Array.isArray(data)) data = [data]

      const valid: any[] = []
      const invalid: any[] = []
      const errors: string[] = []

      data.forEach((item, index) => {
        const result = validateAccount(item, index)
        if (result.valid) {
          valid.push({ ...item, _type: result.type, _index: index, _inferredProvider: result.inferredProvider })
        } else {
          invalid.push({ ...item, _index: index })
          errors.push(...result.errors)
        }
      })

      setParseResult({ valid, invalid, errors })
    } catch (e: any) {
      setParseResult({ valid: [], invalid: [], errors: [`JSON 解析失败: ${e.message}`] })
    }
  }

  const handleFileSelect = async (file: File) => {
    if (!file) return
    const text = await file.text()
    setJsonText(text)
    parseJson(text)
  }

  const runConcurrent = async (items: any[], handler: any, onProgress: any) => {
    const results = []
    let completed = 0
    const concurrency = getConcurrency(items.length)

    for (let i = 0; i < items.length; i += concurrency) {
      const batch = items.slice(i, i + concurrency)
      const batchResults = await Promise.all(
        batch.map(async (item) => {
          const result = await handler(item)
          completed++
          onProgress(completed)
          return result
        })
      )
      results.push(...batchResults)
    }
    return results
  }

  const handleJsonImport = async () => {
    if (!parseResult?.valid.length) return

    setImporting(true)
    setImportProgress({ current: 0, total: parseResult.valid.length })

    const added: any[] = []
    const updated: any[] = []
    const failed: any[] = []

    const importOne = async (item: any) => {
      try {
        let result: any
        const provider = item._inferredProvider || item.provider
        if (item._type === 'social') {
          result = await invoke('add_account_by_social', {
            refreshToken: item.refreshToken,
            provider: provider,
            machineId: item.machineId || null,
            accessToken: item.accessToken || null
          })
        } else {
          // IdC 账号：统一调用 add_account_by_idc
          const params = {
            provider,  // BuilderId 或 Enterprise
            refreshToken: item.refreshToken,
            clientId: item.clientId,
            clientSecret: item.clientSecret,
            region: item.region || null,
            machineId: item.machineId || null,
            accessToken: item.accessToken || null,
            password: item.password || null,
            startUrl: item.startUrl || null,  // Enterprise 可能需要
            clientIdHash: item.clientIdHash || null  // Enterprise 可用 clientIdHash 替代 startUrl
          }

          result = await invoke('add_account_by_idc', params)
        }

        const account = result.account
        if (isBannedStatus(account.status)) {
          return { success: true, index: item._index + 1, email: getAccountDisplayName(account), account, isNew: result.isNew, banned: true }
        }
        return { success: true, index: item._index + 1, email: getAccountDisplayName(account), account, isNew: result.isNew }
      } catch (e) {
        const errorMsg = String(e)
        if (errorMsg.includes('BANNED')) {
          return { success: false, index: item._index + 1, error: '账号已封禁', banned: true }
        }
        return { success: false, index: item._index + 1, error: errorMsg.slice(0, 50) }
      }
    }

    const results = await runConcurrent(
      parseResult.valid,
      importOne,
      (completed: number) => setImportProgress({ current: completed, total: parseResult.valid.length })
    )

    results.forEach(r => {
      if (r.success) {
        if (r.isNew) {
          added.push({ index: r.index, email: r.email, account: r.account })
        } else {
          updated.push({ index: r.index, email: r.email, account: r.account })
        }
      } else {
        failed.push({ index: r.index, error: r.error })
      }
    })

    setImportResult({ added, updated, failed })
    setImporting(false)
    if (added.length > 0 || updated.length > 0) onSuccess?.({ added, updated })
  }

  const handleKiroImport = async () => {
    if (kiroAccounts.length === 0) return

    setKiroImporting(true)
    setKiroProgress({ current: 0, total: kiroAccounts.length })

    const added: any[] = []
    const updated: any[] = []
    const failed: any[] = []

    const importOne = async (account: any) => {
      try {
        let result: any
        if (account.authMethod === 'IdC') {
          // IdC 账号：统一调用 add_account_by_idc
          const params = {
            provider: account.provider,  // BuilderId 或 Enterprise
            refreshToken: account.refreshToken,
            clientId: account.clientId,
            clientSecret: account.clientSecret,
            region: account.region || null,
            machineId: null,
            accessToken: account.accessToken || null,
            password: null,
            startUrl: null,  // 从 Kiro 导入时不需要 startUrl（使用 clientIdHash）
            clientIdHash: account.clientIdHash || null  // 使用 Kiro 提供的 clientIdHash
          }

          result = await invoke('add_account_by_idc', params)
        } else {
          result = await invoke('add_account_by_social', {
            refreshToken: account.refreshToken,
            provider: account.provider,
            machineId: null,
            accessToken: account.accessToken || null
          })
        }

        const acc = result.account
        if (isBannedStatus(acc.status)) {
          return { success: true, email: acc.email, account: acc, isNew: result.isNew, banned: true }
        }
        return { success: true, email: acc.email, account: acc, isNew: result.isNew }
      } catch (e) {
        const errorMsg = String(e)
        if (errorMsg.includes('BANNED')) {
          return { success: false, error: '账号已封禁', banned: true }
        }
        return { success: false, error: errorMsg.slice(0, 80) }
      }
    }

    const results = await runConcurrent(
      kiroAccounts,
      importOne,
      (completed: number) => setKiroProgress({ current: completed, total: kiroAccounts.length })
    )

    results.forEach(r => {
      if (r.success) {
        if (r.isNew) {
          added.push({ email: r.email, account: r.account })
        } else {
          updated.push({ email: r.email, account: r.account })
        }
      } else {
        failed.push({ error: r.error })
      }
    })

    setKiroResult({ added, updated, failed })
    setKiroImporting(false)

    if (added.length > 0 || updated.length > 0) {
      onSuccess?.({ added, updated })
    }
  }

  const handleKiroCliImport = async () => {
    if (!kiroCliDbPath) return

    setKiroCliImporting(true)
    setKiroCliResult(null)

    try {
      const result = await invoke<any>('import_from_kiro_cli', {
        dbPath: kiroCliDbPath
      })

      if (result.success) {
        setKiroCliResult({
          success: true,
          isNew: result.is_new,
          email: result.account?.email || result.account?.userId || '未知账号'
        })

        onSuccess?.({
          added: result.is_new ? [{ email: result.account?.email || result.account?.userId || '未知账号', account: result.account }] : [],
          updated: result.is_new ? [] : [{ email: result.account?.email || result.account?.userId || '未知账号', account: result.account }]})
      } else {
        setKiroCliResult({
          success: false,
          error: result.error || '导入失败'
        })
      }
    } catch (e) {
      setKiroCliResult({
        success: false,
        error: String(e)
      })
    } finally {
      setKiroCliImporting(false)
    }
  }

  const renderResult = (result: any) => (
  <Stack gap="md" p="sm">
    {result.added && result.added.length > 0 && (
      <Alert variant="success">
        <CheckCircle size={20} />
        <div className={`font-medium text-foreground`}>✅ 新增 {result.added.length} 个账号</div>
        {result.added.length > 0 && (
          <div className={`text-sm mt-2 text-foreground`}>{result.added.map((s: any) => s.email).join(', ')}</div>
        )}
      </Alert>
    )}

    {result.updated && result.updated.length > 0 && (
      <Alert variant="info">
        <CheckCircle size={20} />
        <div className={`font-medium text-foreground`}>📝 更新 {result.updated.length} 个账号</div>
        {result.updated.length > 0 && (
          <div className={`text-sm mt-2 text-foreground`}>{result.updated.map((s: any) => s.email).join(', ')}</div>
        )}
      </Alert>
    )}

    {result.failed && result.failed.length > 0 && (
      <Alert variant="destructive">
        <AlertCircle size={20} />
        <div className={`font-medium text-foreground`}>❌ 失败 {result.failed.length} 个</div>
        <Stack gap={4} mt="xs" p={0}>
          {result.failed.map((f: any, i: number) => (
            <div key={i} className={`text-sm text-foreground`}>{f.error}</div>
          ))}
        </Stack>
      </Alert>
    )}
  </Stack>
)

return (
  <DialogRoot open={true} onOpenChange={(open) => !open && onClose()}>
    <DialogContent maxWidth="700px">
      <DialogHeader icon={Upload} iconColor="text-white" iconBg={`bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} ${accent.shadow}`}>
        <DialogTitle>{t('import.title')}</DialogTitle>
        <DialogDescription>{t('import.subtitle') || '批量导入账号数据'}</DialogDescription>
      </DialogHeader>

      <DialogBody noPadding>
        {importResult || kiroResult || kiroCliResult ? (
          <div className="px-6 py-4">
            {importResult && renderResult(importResult)}
            {kiroResult && renderResult(kiroResult)}
            {kiroCliResult && (
              <Alert variant={kiroCliResult.success ? "success" : "destructive"}>
                {kiroCliResult.success ? <CheckCircle size={16} /> : <AlertCircle size={16} />}
                <div className={`text-sm font-medium text-foreground`}>
                  {kiroCliResult.success
                    ? (kiroCliResult.isNew
                      ? `✅ 新增账号: ${kiroCliResult.email}`
                      : `📝 更新账号: ${kiroCliResult.email}`)
                    : '❌ 导入失败'}
                </div>
                {kiroCliResult.error && (
                  <div className={`text-xs mt-1 text-muted-foreground`}>{kiroCliResult.error}</div>
                )}
              </Alert>
            )}
          </div>
        ) : importing || kiroImporting || kiroCliImporting ? (
          <div className="px-6 py-6">
            <div className={`p-5 rounded-xl bg-muted/30 border border-border`}>
              <div className="flex items-center gap-4 mb-4">
                <div className={`w-10 h-10 rounded-xl flex items-center justify-center bg-muted/30`}>
                  <Loader2 size={20} className={"text-primary animate-spin"} />
                </div>
                <div>
                  <div className={`font-medium text-foreground`}>
                    {importing ? t('import.importing') : kiroImporting ? '正在从 Kiro 导入...' : '正在从 kiro-cli 导入...'}
                  </div>
                  <div className={`text-sm text-muted-foreground`}>
                    {kiroCliImporting ? '请稍候...' : `${(importing ? importProgress : kiroProgress).current}/${(importing ? importProgress : kiroProgress).total}`}
                  </div>
                </div>
              </div>
              <Progress
                value={((importing ? importProgress : kiroProgress).total > 0) ? ((importing ? importProgress : kiroProgress).current /
                  (importing ? importProgress : kiroProgress).total * 100) : 0}
                className="h-3 rounded-xl"
              />
            </div>
          </div>
        ) : (
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <TabsList className="px-6 pt-3 pb-2 border-b-0 bg-transparent h-auto">
              <div className="grid grid-cols-4 gap-1 p-1 rounded-xl border border-border bg-muted/40 w-full">
                {[
                  { key: 'json', icon: FileJson, label: t('import.jsonTab') },
                  { key: 'kiro', icon: Upload, label: t('import.kiroTab') },
                  { key: 'kiro-cli', icon: Database, label: t('import.kiroCliTab') },
                  { key: 'online', icon: LogIn, label: t('nav.desktopOAuth') },
                ].map(({ key, icon: Icon, label }) => (
                  <button
                    key={key}
                    type="button"
                    onClick={() => setActiveTab(key)}
                    className={`appearance-none outline-none border-0 py-2 px-3 text-sm rounded-lg transition-all duration-200 font-medium cursor-pointer flex items-center justify-center gap-2 ${
                      activeTab === key
                        ? 'bg-gradient-to-r from-blue-500 to-purple-600 text-white shadow-md shadow-blue-500/30'
                        : 'bg-transparent text-muted-foreground hover:text-foreground hover:bg-muted/60'
                    }`}
                  >
                    <Icon size={16} />
                    <span className="truncate">{label}</span>
                  </button>
                ))}
              </div>
            </TabsList>
            <TabsContent value="json" className="px-6 pb-4 pt-4 outline-none">
              <Stack gap="lg">
                <Group>
                  <FileButton onChange={handleFileSelect} accept=".json">
                    {(props: any) => <LegacyButton {...props} leftSection={<FileJson size={16} />}>{t('import.selectFile')}</LegacyButton>}
                  </FileButton>
                  <LegacyButton
                    color="blue"
                    size="sm"
                    onClick={() => {
                      try {
                        const parsed = JSON.parse(jsonText)
                        const formatted = JSON.stringify(parsed, null, 2)
                        setJsonText(formatted)
                        parseJson(formatted)
                      } catch {
                        // 如果解析失败，尝试格式化为数组
                        try {
                          const text = jsonText.trim()
                          if (text && !text.startsWith('[')) {
                            const formatted = JSON.stringify([JSON.parse(text)], null, 2)
                            setJsonText(formatted)
                            parseJson(formatted)
                          }
                        } catch {}
                      }
                    }}
                  >
                    格式化
                  </LegacyButton>
                  <LegacyButton color="blue" size="sm" onClick={() => { const text = JSON.stringify([{ refreshToken: "", provider: "Google" }], null, 2); setJsonText(text); parseJson(text) }}>
                    Social 模板
                  </LegacyButton>
                  <LegacyButton color="violet" size="sm" onClick={() => { const text = JSON.stringify([{ refreshToken: "", clientId: "", clientSecret: "", provider: "BuilderId" }], null, 2); setJsonText(text); parseJson(text) }}>
                    BuilderId 模板
                  </LegacyButton>
                  <LegacyButton color="grape" size="sm" onClick={() => { const text = JSON.stringify([{ refreshToken: "", clientId: "", clientSecret: "", provider: "Enterprise" }], null, 2); setJsonText(text); parseJson(text) }}>
                    Enterprise 模板
                  </LegacyButton>
                </Group>

                <div className="rounded-xl border border-input bg-background overflow-hidden focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/20 transition-all">
                  <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border/50 bg-muted/30">
                    <FileJson size={13} className="text-muted-foreground" />
                    <span className="text-[11px] font-medium text-muted-foreground">JSON 凭证</span>
                  </div>
                  <textarea
                    value={jsonText}
                    onChange={(e) => { setJsonText(e.target.value); parseJson(e.target.value) }}
                    rows={10}
                    placeholder={`[{"refreshToken": "aor...", "provider": "Google"}]`}
                    className="w-full p-4 bg-transparent text-foreground font-mono text-sm outline-none resize-none"
                  />
                </div>

                {parseResult && (
                  <Stack gap="xs">
                    {parseResult.valid.length > 0 && (
                      <Alert variant="success">
                        <CheckCircle size={16} />
                        {t('import.parseSuccess')}: {parseResult.valid.length} {t('import.validRecords')}
                      </Alert>
                    )}
                    {parseResult.errors.length > 0 && (
                      <Alert variant="destructive">
                        <AlertCircle size={16} />
                        <div className={`text-sm font-medium text-foreground`}>{t('import.validationError')}</div>
                        <Stack gap={2} mt="xs">
                          {parseResult.errors.slice(0, 5).map((err: string, i: number) => (
                            <div key={i} className={`text-xs text-foreground`}>{err}</div>
                          ))}
                          {parseResult.errors.length > 5 && (
                            <div className={`text-xs text-foreground`}>{t('import.moreErrors', { count: parseResult.errors.length - 5 })}</div>
                          )}
                        </Stack>
                      </Alert>
                    )}
                  </Stack>
                )}
              </Stack>
            </TabsContent>

            <TabsContent value="kiro" className="px-6 pb-4 pt-4 outline-none">
              <Stack gap="lg">
                <Alert variant="info">
                  <div className={`text-sm font-medium text-foreground`}>从 Kiro IDE 导入账号</div>
                  <div className={`text-xs mt-1 text-muted-foreground`}>
                    自动读取 Kiro IDE 缓存的账号信息（~/.aws/sso/cache/kiro-auth-token.json）
                  </div>
                </Alert>

                {kiroLoading ? (
                  <div className={`p-5 rounded-xl bg-muted/30 border border-border`}>
                    <div className="flex items-center gap-3">
                      <Loader2 size={20} className={`animate-spin ${accent.text}`} />
                      <div className={`text-sm text-foreground`}>正在检测 Kiro IDE 账号...</div>
                    </div>
                  </div>
                ) : kiroError ? (
                  <Alert variant="destructive">
                    <AlertCircle size={16} />
                    <div className={`text-sm font-medium text-foreground`}>检测失败</div>
                    <div className={`text-xs mt-1 text-muted-foreground`}>{kiroError}</div>
                    <LegacyButton
                      color="red"
                      size="xs"
                      className="mt-3"
                      leftSection={<RefreshCw size={14} />}
                      onClick={detectKiroAccounts}
                    >
                      重新检测
                    </LegacyButton>
                  </Alert>
                ) : kiroAccounts.length > 0 ? (
                  <>
                    <Alert variant="success">
                      <CheckCircle size={16} />
                      <div className={`text-sm font-medium text-foreground`}>检测到 {kiroAccounts.length} 个账号</div>
                    </Alert>

                    <div className={`p-4 rounded-xl bg-muted/30 border border-border max-h-[240px] overflow-y-auto`}>
                      <Stack gap="sm">
                        {kiroAccounts.map((account, index) => (
                          <div key={index} className={`p-3 rounded-lg glass-card border border-border`}>
                            <div className="flex items-center justify-between">
                              <div>
                                <div className={`text-sm font-medium text-foreground`}>
                                  {getProviderDisplayName(account.provider)} ({account.authMethod})
                                </div>
                                <div className={`text-xs text-muted-foreground`}>
                                  {getAccountDisplayName(account)}
                                </div>
                              </div>
                              <div className={`px-2 py-1 rounded text-xs info-badge`}>
                                {account.authMethod === 'IdC' ? 'IdC' : 'Social'}
                              </div>
                            </div>
                          </div>
                        ))}
                      </Stack>
                    </div>
                  </>
                ) : (
                  <Alert variant="default">
                    <AlertCircle size={16} />
                    <div className={`text-sm text-foreground`}>未检测到 Kiro IDE 账号</div>
                    <div className={`text-xs mt-1 text-muted-foreground`}>
                      请先在 Kiro IDE 中登录账号
                    </div>
                  </Alert>
                )}
              </Stack>
            </TabsContent>

            <TabsContent value="kiro-cli" className="px-6 pb-4 pt-4 outline-none">
              <Stack gap="lg">
                <Alert variant="info">
                  <AlertCircle size={16} />
                  <div className={`text-sm font-medium text-foreground`}>{t('import.kiroCliTitle')}</div>
                  <div className={`text-xs mt-1 text-muted-foreground whitespace-pre-line`}>
                    {t('import.kiroCliHint')}
                  </div>
                </Alert>

                {kiroCliDetecting ? (
                  <div className={`p-5 rounded-xl bg-muted/30 border border-border`}>
                    <div className="flex items-center gap-3">
                      <Loader2 size={20} className={`animate-spin ${accent.text}`} />
                      <div className={`text-sm text-foreground`}>{t('import.kiroCliDetecting')}</div>
                    </div>
                  </div>
                ) : kiroCliDetected ? (
                  <Alert variant="success">
                    <CheckCircle size={16} />
                    <div className={`text-sm font-medium text-foreground`}>{t('import.kiroCliDetected')}</div>
                    <div className={`text-xs mt-1 text-muted-foreground`}>{kiroCliDbPath}</div>
                  </Alert>
                ) : (
                  <Alert variant="default">
                    <AlertCircle size={16} />
                    <div className={`text-sm text-foreground`}>{t('import.kiroCliNotDetected')}</div>
                    <div className={`text-xs mt-1 text-muted-foreground`}>
                      {t('import.kiroCliPathHintManual')}
                    </div>
                  </Alert>
                )}

                <div className={`p-4 rounded-xl bg-muted/30 border border-border`}>
                  <Stack gap="md">
                    <div>
                      <label className={`text-sm font-medium text-foreground block mb-2`}>
                        {t('import.kiroCliPathLabel')}
                      </label>
                      <div className="flex gap-2">
                        <input
                          type="text"
                          value={kiroCliDbPath}
                          onChange={(e) => {
                            setKiroCliDbPath(e.target.value)
                            setKiroCliDetected(false)
                          }}
                          placeholder={t('import.kiroCliPathPlaceholder')}
                          className={`flex-1 px-4 py-3 border rounded-xl text-foreground bg-background border-input ${colors.inputFocus} focus:ring-2 transition-all outline-none`}
                        />
                        <FileButton
                          onChange={(file: any) => {
                            if (file) {
                              setKiroCliDbPath(file.path)
                              setKiroCliDetected(false)
                            }
                          }}
                          accept=".sqlite3,.db"
                        >
                          {(props: any) => (
                            <LegacyButton
                              {...props}
                              className="px-4"
                            >
                              浏览
                            </LegacyButton>
                          )}
                        </FileButton>
                      </div>
                    </div>
                  </Stack>
                </div>
              </Stack>
            </TabsContent>

            <TabsContent value="online" className="outline-none">
              <div className="h-[460px] overflow-auto">
                <Login onLogin={onClose} />
              </div>
            </TabsContent>
          </Tabs>
        )}
      </DialogBody>

      <DialogFooter>
        <Button variant="secondary" onClick={onClose} disabled={importing || kiroImporting || kiroCliImporting}>
          {importResult || kiroResult || kiroCliResult ? t('common.close') : t('common.cancel')}
        </Button>
        {!(importResult || kiroResult || kiroCliResult) && (
          <Button
            onClick={activeTab === 'json' ? handleJsonImport : activeTab === 'kiro' ? handleKiroImport : handleKiroCliImport}
            disabled={importing || kiroImporting || kiroCliImporting || (activeTab === 'json' && !parseResult?.valid.length) || (activeTab === 'kiro' && kiroAccounts.length === 0) || (activeTab === 'kiro-cli' && !kiroCliDbPath)}
            loading={importing || kiroImporting || kiroCliImporting}
          >
            {t('common.import')}
          </Button>
        )}
      </DialogFooter>
    </DialogContent>
  </DialogRoot>
)
}

export default ImportAccountModal

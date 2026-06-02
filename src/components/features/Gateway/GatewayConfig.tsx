import type React from 'react'
import { useState } from 'react'
import { RotateCw, TrendingUp, Shuffle, Zap, Network, KeyRound, Filter, ShieldCheck } from 'lucide-react'
import { Button } from '@/components/ui/actions/button'
import { Input } from '@/components/ui/forms/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/forms/select'
import { Switch } from '@/components/ui/forms/switch'
import { Textarea } from '@/components/ui/forms/textarea'
import { GatewaySurfaceCard } from './GatewayShared'
import ModelMappingDialog from './ModelMappingDialog'
import ApiKeysDialog from './ApiKeysDialog'
import PromptFilterRulesDialog from './PromptFilterRulesDialog'

interface GatewayConfigProps {
  config: any;
  fieldErrors: Record<string, string>;
  setField: (key: string, value: any) => void;
  accountOptions: any[];
  groupOptions: any[];
  setConfig: React.Dispatch<React.SetStateAction<any>>;
  applyGatewayLocalOnlyChange: (config: any, checked: boolean, generator: () => string) => any;
  createGeneratedApiKey: () => string;
  handleSaveConfig: () => Promise<void>;
  handleAutoStartToggle: (checked: boolean) => Promise<void>;
  onShowClientConfig?: () => void;
  hasConfiguredClients?: boolean;
}

const ACCENT = {
  blue: { bar: 'from-blue-500 to-blue-400', chip: 'bg-blue-500/15 text-blue-500' },
  violet: { bar: 'from-violet-500 to-violet-400', chip: 'bg-violet-500/15 text-violet-500' },
  amber: { bar: 'from-amber-500 to-amber-400', chip: 'bg-amber-500/15 text-amber-500' },
  emerald: { bar: 'from-emerald-500 to-emerald-400', chip: 'bg-emerald-500/15 text-emerald-500' },
} as const
type AccentKey = keyof typeof ACCENT

/** 区块：左侧渐变竖条 + 图标徽章 + 标题，下方内容区 */
function Section({ icon, title, accent, children }: { icon: React.ReactNode; title: string; accent: AccentKey; children: React.ReactNode }) {
  const a = ACCENT[accent]
  return (
    <section className="overflow-hidden rounded-xl border border-border/60 bg-card/30">
      <header className="flex items-center gap-2.5 border-b border-border/50 bg-gradient-to-r from-muted/40 to-transparent px-3.5 py-2.5">
        <span className={`h-4 w-1 rounded-full bg-gradient-to-b ${a.bar}`} />
        <span className={`flex h-6 w-6 items-center justify-center rounded-lg ${a.chip}`}>{icon}</span>
        <span className="text-sm font-semibold text-foreground">{title}</span>
      </header>
      <div className="p-3">{children}</div>
    </section>
  )
}

/** 字段瓦片：大写小标签 + 控件 + 可选错误 */
function Field({ label, error, className = '', children }: { label: string; error?: string; className?: string; children: React.ReactNode }) {
  return (
    <div className={`flex flex-col gap-1.5 rounded-lg border border-border/60 bg-muted/20 p-2.5 transition-colors focus-within:border-primary/50 ${className}`}>
      <label className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{label}</label>
      {children}
      {error && <div className="text-[11px] text-red-500">{error}</div>}
    </div>
  )
}

/** 开关瓦片：标题(+描述) + Switch，开启时主题色高亮 */
function Toggle({ label, desc, checked, onChange }: { label: string; desc?: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className={`flex items-center justify-between gap-2 rounded-lg border p-2.5 transition-colors ${checked ? 'border-primary/50 bg-primary/5' : 'border-border/60 bg-muted/20'}`}>
      <div className="min-w-0">
        <div className="text-sm font-medium text-foreground">{label}</div>
        {desc && <div className="truncate text-[11px] text-muted-foreground">{desc}</div>}
      </div>
      <Switch checked={checked} onCheckedChange={onChange} />
    </div>
  )
}

/** 操作行：状态文案 + 按钮 */
function ActionRow({ text, children }: { text: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-2 rounded-lg border border-border/60 bg-muted/20 px-3 py-2.5">
      <div className="truncate text-sm text-muted-foreground">{text}</div>
      {children}
    </div>
  )
}

const REGIONS = [
  'us-east-1', 'us-east-2', 'us-west-1', 'us-west-2',
  'eu-central-1', 'eu-central-2', 'eu-west-1', 'eu-west-2', 'eu-west-3', 'eu-north-1', 'eu-south-1', 'eu-south-2',
  'ap-northeast-1', 'ap-northeast-2', 'ap-northeast-3', 'ap-southeast-1', 'ap-southeast-2', 'ap-southeast-3', 'ap-southeast-4', 'ap-southeast-5', 'ap-southeast-7', 'ap-south-1', 'ap-south-2', 'ap-east-1',
  'af-south-1', 'ca-central-1', 'ca-west-1', 'sa-east-1', 'me-south-1', 'me-central-1', 'il-central-1', 'mx-central-1',
  'us-gov-west-1', 'us-gov-east-1', 'cn-north-1', 'cn-northwest-1',
]

function GatewayConfig({
  config,
  fieldErrors,
  setField,
  accountOptions,
  setConfig,
  applyGatewayLocalOnlyChange,
  createGeneratedApiKey,
  handleSaveConfig,
  handleAutoStartToggle,
  onShowClientConfig,
  hasConfiguredClients = false,
}: GatewayConfigProps) {
  const [showModelMappingDialog, setShowModelMappingDialog] = useState(false)
  const [showApiKeysDialog, setShowApiKeysDialog] = useState(false)
  const [showPromptFilterRulesDialog, setShowPromptFilterRulesDialog] = useState(false)

  const poolMode = config.accountMode === 'pool' || config.accountMode === 'group'
  const rawKeys = (config.clientApiKeysText || '').split(/[\n,]+/).map((k: string) => k.trim()).filter(Boolean)
  const enabledKeys = rawKeys.filter((k: string) => !k.startsWith('#disabled#')).length

  return (
    <div className="grid grid-cols-1 gap-3">
      <GatewaySurfaceCard className="rounded-xl">
        <div className="flex flex-col gap-3">
          {/* 1. 网络与路由 */}
          <Section icon={<Network size={13} />} title="网络与路由" accent="blue">
            <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
              {/* 监听端点：地址 / 端口 / Region */}
              <div className="rounded-xl border border-border/60 bg-gradient-to-br from-muted/40 to-transparent p-3">
                <div className="flex flex-wrap items-start gap-2.5">
                  <div className="flex min-w-[150px] flex-1 flex-col gap-1">
                    <label className="text-[11px] text-muted-foreground">地址</label>
                    <Input
                      value={config.host}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) => setField('host', e.target.value || '127.0.0.1')}
                      className={fieldErrors.host ? 'border-red-500' : ''}
                    />
                    {fieldErrors.host && <div className="text-[11px] text-red-500">{fieldErrors.host}</div>}
                  </div>
                  <div className="flex w-[100px] flex-col gap-1">
                    <label className="text-[11px] text-muted-foreground">端口</label>
                    <Input
                      type="number"
                      value={config.port}
                      min={1}
                      max={65535}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) => setField('port', Number(e.target.value) || 8765)}
                      className={fieldErrors.port ? 'border-red-500' : ''}
                    />
                    {fieldErrors.port && <div className="text-[11px] text-red-500">{fieldErrors.port}</div>}
                  </div>
                  <div className="flex w-[170px] flex-col gap-1">
                    <label className="text-[11px] text-muted-foreground">Region</label>
                    <Select value={config.region} onValueChange={(v: string) => setField('region', v || 'us-east-1')}>
                      <SelectTrigger className={`w-full ${fieldErrors.region ? 'border-red-500' : ''}`}><SelectValue /></SelectTrigger>
                      <SelectContent>
                        {REGIONS.map((r) => <SelectItem key={r} value={r}>{r}</SelectItem>)}
                      </SelectContent>
                    </Select>
                    {fieldErrors.region && <div className="text-[11px] text-red-500">{fieldErrors.region}</div>}
                  </div>
                </div>
              </div>

              {/* 账号路由：分段切换 + 条件控件 */}
              <div className="space-y-2.5 rounded-xl border border-border/60 bg-muted/20 p-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-foreground">账号路由</div>
                  </div>
                  <div className="inline-flex shrink-0 rounded-lg border border-border/60 bg-background p-0.5 text-xs font-medium">
                    <button type="button" onClick={() => setField('accountMode', 'single')}
                      className={`rounded-md px-2.5 py-1 transition-colors ${!poolMode ? 'bg-primary text-primary-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}`}>单账号</button>
                    <button type="button" onClick={() => setField('accountMode', 'pool')}
                      className={`rounded-md px-2.5 py-1 transition-colors ${poolMode ? 'bg-primary text-primary-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}`}>多账号轮询</button>
                  </div>
                </div>
                {poolMode ? (
                  <div className="flex flex-col gap-1">
                    <Select value={config.strategy} onValueChange={(v: string) => setField('strategy', v || 'round_robin')}>
                      <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="round_robin"><div className="flex items-center gap-2"><RotateCw size={14} /><span>轮询</span></div></SelectItem>
                        <SelectItem value="most_quota"><div className="flex items-center gap-2"><TrendingUp size={14} /><span>优先剩余额度</span></div></SelectItem>
                        <SelectItem value="random"><div className="flex items-center gap-2"><Shuffle size={14} /><span>随机</span></div></SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                ) : (
                  <div className="flex flex-col gap-1">
                    <Select value={config.accountId} onValueChange={(v: string) => setField('accountId', v)}>
                      <SelectTrigger className={`w-full ${fieldErrors.accountId ? 'border-red-500' : ''}`}>
                        <SelectValue placeholder="选择一个账号" />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        {accountOptions.map((opt: any) => <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>)}
                      </SelectContent>
                    </Select>
                    {fieldErrors.accountId && <div className="text-[11px] text-red-500">{fieldErrors.accountId}</div>}
                  </div>
                )}
              </div>
            </div>
          </Section>

          {/* 2. 客户端认证与模型 */}
          <Section icon={<KeyRound size={13} />} title="客户端认证与模型" accent="violet">
            <div className="grid grid-cols-1 gap-2.5 md:grid-cols-3">
              <ActionRow text={rawKeys.length > 0 ? `${rawKeys.length} 个 Key，${enabledKeys} 个启用` : '暂无 API Key'}>
                <Button size="sm" variant="outline" className="h-7 text-sm" onClick={() => setShowApiKeysDialog(true)}>管理 Keys</Button>
              </ActionRow>
              <ActionRow text={config.modelMappings?.length > 0 ? `${config.modelMappings.length} 条映射，${config.modelMappings.filter((r: any) => r.enabled).length} 条启用` : '暂无映射规则'}>
                <Button size="sm" variant="outline" className="h-7 text-sm" onClick={() => setShowModelMappingDialog(true)}>
                  <Shuffle size={12} className="mr-1" />映射规则
                </Button>
              </ActionRow>
              {onShowClientConfig && (
                <ActionRow text={hasConfiguredClients ? '✓ 已配置客户端' : '写入客户端配置'}>
                  <Button size="sm" variant={hasConfiguredClients ? 'default' : 'outline'} className="h-7 text-sm" onClick={onShowClientConfig}>
                    <Zap size={12} className="mr-1" />{hasConfiguredClients ? '重新配置' : '配置客户端'}
                  </Button>
                </ActionRow>
              )}
            </div>
            {fieldErrors.clientApiKeysText && <div className="mt-2 text-xs text-red-500">{fieldErrors.clientApiKeysText}</div>}
          </Section>

          {/* 3. 提示词过滤 */}
          <Section icon={<Filter size={13} />} title="提示词过滤" accent="amber">
            <div className="space-y-2.5">
              <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
                <Toggle label="精简CC提示" checked={!!config.filterClaudeCode} onChange={(v) => setField('filterClaudeCode', v)} />
                <Toggle label="去边界标记" checked={!!config.filterStripBoundaries} onChange={(v) => setField('filterStripBoundaries', v)} />
                <Toggle label="去环境噪音" checked={!!config.filterEnvNoise} onChange={(v) => setField('filterEnvNoise', v)} />
              </div>
              <ActionRow text={config.promptFilterRules?.length > 0 ? `${config.promptFilterRules.length} 条自定义规则，${config.promptFilterRules.filter((r: any) => r.enabled).length} 条启用` : '暂无自定义规则'}>
                <Button size="sm" variant="outline" className="h-7 text-sm" onClick={() => setShowPromptFilterRulesDialog(true)}>管理规则</Button>
              </ActionRow>
            </div>
          </Section>

          {/* 4. 安全与高级 */}
          <Section icon={<ShieldCheck size={13} />} title="安全与高级" accent="emerald">
            <div className="space-y-2.5">
              <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-3 lg:grid-cols-5">
                <Toggle
                  label="仅本机"
                  checked={!!config.localOnly}
                  onChange={(checked) => setConfig((prev: any) => applyGatewayLocalOnlyChange(prev, checked, createGeneratedApiKey))}
                />
                <Toggle label="自动启动" checked={!!config.enabled} onChange={handleAutoStartToggle} />
                <Toggle label="响应缓存" checked={!!config.responseCacheEnabled} onChange={(v) => setField('responseCacheEnabled', v)} />
                <Field label="缓存TTL(秒)">
                  <Input
                    type="number"
                    value={config.responseCacheTtl}
                    min={30}
                    max={3600}
                    className="h-7 text-sm"
                    onChange={(e: React.ChangeEvent<HTMLInputElement>) => setField('responseCacheTtl', Number(e.target.value) || 180)}
                    disabled={!config.responseCacheEnabled}
                  />
                </Field>
                <Field label="阈值%">
                  <Input
                    type="number"
                    value={config.threshold}
                    min={1}
                    max={100}
                    className="h-7 text-sm"
                    onChange={(e: React.ChangeEvent<HTMLInputElement>) => setField('threshold', Number(e.target.value) || 90)}
                  />
                </Field>
              </div>
              {!config.localOnly && (
                <Field label="IP 白名单" error={fieldErrors.allowedIpsText}>
                  <Textarea
                    placeholder={'192.168.1.10\n10.0.0.0/24'}
                    rows={2}
                    value={config.allowedIpsText}
                    onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => setField('allowedIpsText', e.target.value)}
                    className={fieldErrors.allowedIpsText ? 'border-red-500' : ''}
                  />
                </Field>
              )}
            </div>
          </Section>
        </div>
      </GatewaySurfaceCard>

      <ModelMappingDialog
        open={showModelMappingDialog}
        onOpenChange={setShowModelMappingDialog}
        modelMappings={config.modelMappings}
        setField={setField}
        onSave={handleSaveConfig}
      />
      <ApiKeysDialog
        open={showApiKeysDialog}
        onOpenChange={setShowApiKeysDialog}
        clientApiKeysText={config.clientApiKeysText}
        setConfig={setConfig}
        onSave={handleSaveConfig}
      />
      <PromptFilterRulesDialog
        open={showPromptFilterRulesDialog}
        onOpenChange={setShowPromptFilterRulesDialog}
        promptFilterRules={config.promptFilterRules}
        setField={setField}
        onSave={handleSaveConfig}
      />
    </div>
  )
}

export default GatewayConfig

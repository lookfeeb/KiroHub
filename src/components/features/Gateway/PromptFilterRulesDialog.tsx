import { Plus, Trash2, Filter } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'

// 预置过滤规则
const PRESET_RULES = [
  {
    name: '过滤 Git 状态信息',
    ruleType: 'lines-containing',
    matchPattern: 'git status',
    replace: ''
  },
  {
    name: '过滤最近提交信息',
    ruleType: 'lines-containing',
    matchPattern: 'Recent commits:',
    replace: ''
  },
  {
    name: '过滤助手知识截止日期',
    ruleType: 'lines-containing',
    matchPattern: 'Assistant knowledge cutoff',
    replace: ''
  },
  {
    name: '过滤计费头信息',
    ruleType: 'lines-containing',
    matchPattern: 'x-anthropic-billing-header:',
    replace: ''
  },
  {
    name: '过滤快速模式标签',
    ruleType: 'regex',
    matchPattern: '<fast_mode_info>.*?</fast_mode_info>',
    replace: ''
  },
  {
    name: '过滤项目路径信息',
    ruleType: 'lines-containing',
    matchPattern: '.claude/projects/',
    replace: ''
  }
]

interface PromptFilterRulesDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  promptFilterRules: any[]
  setField: (key: string, value: any) => void
  onSave?: () => void
}

function PromptFilterRulesDialog({ open, onOpenChange, promptFilterRules, setField, onSave }: PromptFilterRulesDialogProps) {
  const rules = promptFilterRules || []

  const handleToggle = (idx: number, checked: boolean) => {
    const updated = [...rules]
    updated[idx] = { ...updated[idx], enabled: checked }
    setField('promptFilterRules', updated)
  }

  const handleDelete = (idx: number) => {
    setField('promptFilterRules', rules.filter((_: any, i: number) => i !== idx))
  }

  const handleAdd = () => {
    const nameEl = document.getElementById('dialog-filter-name') as HTMLInputElement
    const typeEl = document.getElementById('dialog-filter-type') as HTMLInputElement
    const patternEl = document.getElementById('dialog-filter-pattern') as HTMLTextAreaElement
    const replaceEl = document.getElementById('dialog-filter-replace') as HTMLTextAreaElement

    if (!nameEl?.value?.trim() || !patternEl?.value?.trim()) return

    const newRule = {
      id: crypto.randomUUID(),
      name: nameEl.value.trim(),
      enabled: true,
      ruleType: typeEl?.value || 'lines-containing',
      matchPattern: patternEl.value.trim(),
      replace: replaceEl?.value || ''
    }
    setField('promptFilterRules', [...rules, newRule])
    nameEl.value = ''
    patternEl.value = ''
    replaceEl.value = ''
  }

  const handlePreset = () => {
    const existingPatterns = new Set(rules.map((r: any) => r.matchPattern))
    const newRules = PRESET_RULES
      .filter(p => !existingPatterns.has(p.matchPattern))
      .map(p => ({
        id: crypto.randomUUID(),
        name: p.name,
        enabled: true,
        ruleType: p.ruleType,
        matchPattern: p.matchPattern,
        replace: p.replace
      }))
    if (newRules.length > 0) {
      setField('promptFilterRules', [...rules, ...newRules])
    }
  }

  const handleSave = async () => {
    if (onSave) {
      await onSave()
    }
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>提示词过滤规则</DialogTitle>
          <DialogDescription>
            自定义正则表达式或关键字过滤规则，用于清理系统提示中的噪音内容
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {/* 现有规则列表 */}
          {rules.length > 0 && (
            <div className="space-y-2">
              <Label className="text-sm font-medium">已配置规则 ({rules.length})</Label>
              <div className="space-y-2 max-h-64 overflow-y-auto border rounded-lg p-3 bg-muted/20">
                {rules.map((rule: any, idx: number) => (
                  <div key={rule.id || idx} className="flex items-start gap-3 p-3 rounded-lg border bg-background">
                    <Switch
                      checked={rule.enabled}
                      onCheckedChange={(checked: boolean) => handleToggle(idx, checked)}
                      className="mt-1"
                    />
                    <div className="flex-1 min-w-0 space-y-1">
                      <div className="flex items-center gap-2">
                        <span className="font-medium text-sm">{rule.name}</span>
                        <Badge variant="outline" className="text-xs">
                          {rule.ruleType === 'regex' ? '正则' : '包含关键字'}
                        </Badge>
                      </div>
                      <div className="text-xs text-muted-foreground font-mono break-all">
                        匹配: {rule.matchPattern}
                      </div>
                      {rule.ruleType === 'regex' && rule.replace && (
                        <div className="text-xs text-muted-foreground font-mono break-all">
                          替换: {rule.replace}
                        </div>
                      )}
                    </div>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-8 w-8 p-0 text-destructive hover:text-destructive"
                      onClick={() => handleDelete(idx)}
                    >
                      <Trash2 size={14} />
                    </Button>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* 添加新规则 */}
          <div className="space-y-3 border rounded-lg p-4 bg-muted/10">
            <Label className="text-sm font-medium">添加新规则</Label>
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <Label className="text-xs text-muted-foreground">规则名称</Label>
                <Input id="dialog-filter-name" placeholder="例如：过滤 Git 状态" />
              </div>
              <div className="space-y-1.5">
                <Label className="text-xs text-muted-foreground">规则类型</Label>
                <Select defaultValue="lines-containing">
                  <SelectTrigger id="dialog-filter-type" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="lines-containing">包含关键字（删除匹配行）</SelectItem>
                    <SelectItem value="regex">正则表达式（替换匹配内容）</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
            <div className="space-y-1.5">
              <Label className="text-xs text-muted-foreground">匹配模式</Label>
              <Textarea
                id="dialog-filter-pattern"
                placeholder="关键字模式：git status&#10;正则模式：&lt;fast_mode_info&gt;.*?&lt;/fast_mode_info&gt;"
                rows={2}
                className="font-mono text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <Label className="text-xs text-muted-foreground">替换内容（仅正则类型，留空表示删除）</Label>
              <Input
                id="dialog-filter-replace"
                placeholder="留空表示删除匹配内容"
                className="font-mono text-xs"
              />
            </div>
            <div className="flex gap-2">
              <Button size="sm" onClick={handleAdd} className="flex-1">
                <Plus size={14} className="mr-1" />
                添加规则
              </Button>
              <Button size="sm" variant="outline" onClick={handlePreset}>
                <Filter size={14} className="mr-1" />
                添加预置规则
              </Button>
            </div>
          </div>

          {/* 底部操作 */}
          <div className="flex justify-end gap-2 pt-2 border-t">
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              取消
            </Button>
            <Button onClick={handleSave}>
              保存配置
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

export default PromptFilterRulesDialog

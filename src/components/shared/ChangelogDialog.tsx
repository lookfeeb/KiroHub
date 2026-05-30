import type { ReactNode } from 'react'
import { FileText } from 'lucide-react'
import {
  DialogRoot,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogBody,
  DialogFooter,
} from './dialog'
import { Button } from './button'

// 行内：**加粗** 与 [文本](链接)
const INLINE = /(\*\*([^*]+)\*\*|\[([^\]]+)\]\(([^)]+)\))/g
function renderInline(text: string, key: string) {
  const out: ReactNode[] = []
  let last = 0
  let m: RegExpExecArray | null
  let i = 0
  INLINE.lastIndex = 0
  while ((m = INLINE.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index))
    if (m[2] !== undefined) {
      out.push(<strong key={`${key}-b${i}`} className="font-semibold text-foreground">{m[2]}</strong>)
    } else {
      out.push(
        <a key={`${key}-a${i}`} href={m[4]} target="_blank" rel="noopener noreferrer"
          className="text-primary underline underline-offset-2 hover:opacity-80 break-all">{m[3]}</a>
      )
    }
    last = m.index + m[0].length
    i++
  }
  if (last < text.length) out.push(text.slice(last))
  return out
}

// 轻量 markdown：标题 / 列表 / 分割线 / 段落（覆盖 GitHub release 常见格式）
function renderMarkdown(md: string) {
  const lines = md.replace(/\r/g, '').split('\n')
  const blocks: ReactNode[] = []
  let list: string[] | null = null
  const flush = () => {
    if (list && list.length) {
      blocks.push(
        <ul key={`ul${blocks.length}`} className="list-disc pl-5 space-y-1 marker:text-muted-foreground">
          {list.map((li, j) => <li key={j}>{renderInline(li, `li${blocks.length}-${j}`)}</li>)}
        </ul>
      )
    }
    list = null
  }
  lines.forEach((raw, idx) => {
    const line = raw.replace(/\s+$/, '')
    if (/^#{1,6}\s/.test(line)) {
      flush()
      const level = (line.match(/^#+/) as RegExpMatchArray)[0].length
      const text = line.replace(/^#+\s/, '')
      blocks.push(
        <p key={idx} className={level <= 2 ? 'text-base font-semibold text-foreground' : 'text-sm font-semibold text-foreground'}>
          {renderInline(text, `h${idx}`)}
        </p>
      )
    } else if (/^\s*[-*]\s+/.test(line)) {
      if (!list) list = []
      list.push(line.replace(/^\s*[-*]\s+/, ''))
    } else if (/^\s+\S/.test(raw) && list) {
      // 列表项的换行续行
      list[list.length - 1] += ' ' + line.trim()
    } else if (/^---+$/.test(line.trim())) {
      flush()
      blocks.push(<hr key={idx} className="border-border/60" />)
    } else if (line.trim() === '') {
      flush()
    } else {
      flush()
      blocks.push(<p key={idx} className="text-foreground">{renderInline(line, `p${idx}`)}</p>)
    }
  })
  flush()
  return blocks
}

function ChangelogDialog({ version, body, onClose }: { version: string; body: string; onClose: () => void }) {
  return (
    <DialogRoot open={true} onOpenChange={(open) => !open && onClose()}>
      <DialogContent maxWidth="640px">
        <DialogHeader icon={FileText} iconColor="text-blue-500" iconBg="bg-gradient-to-br from-blue-500/20 to-indigo-500/10">
          <DialogTitle>v{version} 更新内容</DialogTitle>
        </DialogHeader>

        <DialogBody>
          <div className="rounded-xl border border-border bg-background/40 px-4 py-3.5 text-sm leading-relaxed space-y-2.5">
            {renderMarkdown(body)}
          </div>
        </DialogBody>

        <DialogFooter>
          <Button variant="primary" size="lg" onClick={onClose}>确定</Button>
        </DialogFooter>
      </DialogContent>
    </DialogRoot>
  )
}

export default ChangelogDialog

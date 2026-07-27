import { useState, type ReactNode } from 'react'
import { Check, Copy } from 'lucide-react'

// 行内：**加粗** / `代码` / [文本](链接)
const INLINE = /(\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\(([^)]+)\))/g
function inline(text: string, k: string): ReactNode[] {
  const out: ReactNode[] = []
  let last = 0
  let m: RegExpExecArray | null
  let i = 0
  INLINE.lastIndex = 0
  while ((m = INLINE.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index))
    if (m[2] !== undefined) out.push(<strong key={`${k}b${i}`} className="font-semibold text-foreground">{m[2]}</strong>)
    else if (m[3] !== undefined) out.push(<code key={`${k}c${i}`} className="rounded-md border border-border/60 bg-muted/70 px-1.5 py-0.5 font-mono text-[0.85em] text-foreground">{m[3]}</code>)
    else out.push(<a key={`${k}a${i}`} href={m[5]} target="_blank" rel="noopener noreferrer" className="text-primary underline underline-offset-2 hover:opacity-80 break-all">{m[4]}</a>)
    last = m.index + m[0].length
    i++
  }
  if (last < text.length) out.push(text.slice(last))
  return out
}

const parseRow = (l: string) => {
  const cells = l.split('|').map(s => s.trim())
  if (cells[0] === '') cells.shift()
  if (cells.length && cells[cells.length - 1] === '') cells.pop()
  return cells
}

function CodeBlock({ lang, code }: { lang: string; code: string }) {
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1400)
    } catch {
      setCopied(false)
    }
  }

  return (
    <div className="my-3 overflow-hidden rounded-xl border border-border/80 bg-slate-950 text-slate-100 shadow-sm dark:bg-slate-950/90">
      <div className="flex h-8 items-center border-b border-white/10 bg-white/[0.045] px-3">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-400">{lang || 'text'}</span>
        <button
          type="button"
          onClick={handleCopy}
          className="ml-auto inline-flex items-center gap-1 rounded-md px-1.5 py-1 text-[10px] text-slate-400 transition-colors hover:bg-white/10 hover:text-slate-100"
          aria-label="复制代码"
        >
          {copied ? <Check className="size-3 text-emerald-400" /> : <Copy className="size-3" />}
          {copied ? '已复制' : '复制'}
        </button>
      </div>
      <pre className="overflow-x-auto p-3.5 font-mono text-xs leading-6"><code>{code}</code></pre>
    </div>
  )
}

/** 轻量 Markdown：代码块 / 表格 / 标题 / 列表 / 分割线 / 行内格式 */
function Markdown({ text }: { text: string }) {
  const lines = (text || '').replace(/\r/g, '').split('\n')
  const blocks: ReactNode[] = []
  let i = 0
  let key = 0
  while (i < lines.length) {
    const line = lines[i]
    // 代码块
    if (/^```/.test(line.trim())) {
      const lang = line.trim().slice(3).trim()
      const code: string[] = []
      i++
      while (i < lines.length && !/^```/.test(lines[i].trim())) { code.push(lines[i]); i++ }
      i++
      blocks.push(<CodeBlock key={key++} lang={lang} code={code.join('\n')} />)
      continue
    }
    // 表格
    if (line.includes('|') && i + 1 < lines.length && /-/.test(lines[i + 1]) && /^\s*\|?[\s:|-]+\|?\s*$/.test(lines[i + 1])) {
      const head = parseRow(line)
      i += 2
      const rows: string[][] = []
      while (i < lines.length && lines[i].includes('|')) { rows.push(parseRow(lines[i])); i++ }
      blocks.push(
        <div key={key++} className="my-3 overflow-x-auto rounded-xl border border-border/80">
          <table className="w-full border-collapse text-xs">
            <thead><tr>{head.map((h, hi) => <th key={hi} className="border-b border-r border-border/80 bg-muted/60 px-3 py-2 text-left font-semibold text-foreground last:border-r-0">{inline(h, `th${key}-${hi}`)}</th>)}</tr></thead>
            <tbody>{rows.map((r, ri) => <tr key={ri} className="even:bg-muted/20">{r.map((c, ci) => <td key={ci} className="border-b border-r border-border/60 px-3 py-2 text-foreground last:border-r-0">{inline(c, `td${key}-${ri}-${ci}`)}</td>)}</tr>)}</tbody>
          </table>
        </div>
      )
      continue
    }
    // 标题
    if (/^#{1,6}\s/.test(line)) {
      const lvl = (line.match(/^#+/) as RegExpMatchArray)[0].length
      blocks.push(<p key={key++} className={lvl <= 2 ? 'mb-1.5 mt-4 text-[15px] font-bold text-foreground' : 'mb-1 mt-3 text-[13px] font-semibold text-foreground'}>{inline(line.replace(/^#+\s/, ''), `h${key}`)}</p>)
      i++
      continue
    }
    // 分割线
    if (/^(-{3,}|\*{3,})$/.test(line.trim())) { blocks.push(<hr key={key++} className="my-4 border-border/60" />); i++; continue }
    // 引用
    if (/^\s*>\s?/.test(line)) {
      const quoted: string[] = []
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) { quoted.push(lines[i].replace(/^\s*>\s?/, '')); i++ }
      blocks.push(<blockquote key={key++} className="my-3 rounded-r-lg border-l-2 border-primary/40 bg-primary/[0.035] px-3 py-2 text-muted-foreground">{quoted.map((item, itemIndex) => <p key={itemIndex} className="leading-6">{inline(item, `quote${key}-${itemIndex}`)}</p>)}</blockquote>)
      continue
    }
    // 列表
    if (/^\s*[-*]\s+/.test(line)) {
      const items: string[] = []
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i])) { items.push(lines[i].replace(/^\s*[-*]\s+/, '')); i++ }
      blocks.push(<ul key={key++} className="my-2 list-disc space-y-1.5 pl-5 marker:text-muted-foreground">{items.map((it, ii) => <li key={ii} className="pl-0.5 leading-6 text-foreground">{inline(it, `li${key}-${ii}`)}</li>)}</ul>)
      continue
    }
    // 有序列表
    if (/^\s*\d+\.\s+/.test(line)) {
      const items: string[] = []
      while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) { items.push(lines[i].replace(/^\s*\d+\.\s+/, '')); i++ }
      blocks.push(<ol key={key++} className="my-2 list-decimal space-y-1.5 pl-5 marker:text-muted-foreground">{items.map((it, ii) => <li key={ii} className="pl-0.5 leading-6 text-foreground">{inline(it, `oli${key}-${ii}`)}</li>)}</ol>)
      continue
    }
    // 空行
    if (line.trim() === '') { i++; continue }
    // 段落
    blocks.push(<p key={key++} className="my-1.5 break-words leading-6 text-foreground">{inline(line, `p${key}`)}</p>)
    i++
  }
  return <div className="text-[13px]">{blocks}</div>
}

export default Markdown

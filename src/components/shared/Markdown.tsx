import type { ReactNode } from 'react'

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
    else if (m[3] !== undefined) out.push(<code key={`${k}c${i}`} className="px-1 py-0.5 rounded bg-muted text-[0.85em] font-mono text-foreground">{m[3]}</code>)
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
      blocks.push(
        <div key={key++} className="my-2 overflow-hidden rounded-lg border border-border bg-muted/40">
          {lang && <div className="px-3 py-1 text-[10px] uppercase tracking-wide text-muted-foreground border-b border-border/60 bg-muted/40">{lang}</div>}
          <pre className="overflow-x-auto p-3 text-xs font-mono leading-relaxed text-foreground"><code>{code.join('\n')}</code></pre>
        </div>
      )
      continue
    }
    // 表格
    if (line.includes('|') && i + 1 < lines.length && /-/.test(lines[i + 1]) && /^\s*\|?[\s:|-]+\|?\s*$/.test(lines[i + 1])) {
      const head = parseRow(line)
      i += 2
      const rows: string[][] = []
      while (i < lines.length && lines[i].includes('|')) { rows.push(parseRow(lines[i])); i++ }
      blocks.push(
        <div key={key++} className="my-2 overflow-x-auto">
          <table className="w-full text-xs border-collapse">
            <thead><tr>{head.map((h, hi) => <th key={hi} className="border border-border bg-muted/50 px-2 py-1 text-left font-semibold text-foreground">{inline(h, `th${key}-${hi}`)}</th>)}</tr></thead>
            <tbody>{rows.map((r, ri) => <tr key={ri}>{r.map((c, ci) => <td key={ci} className="border border-border px-2 py-1 text-foreground">{inline(c, `td${key}-${ri}-${ci}`)}</td>)}</tr>)}</tbody>
          </table>
        </div>
      )
      continue
    }
    // 标题
    if (/^#{1,6}\s/.test(line)) {
      const lvl = (line.match(/^#+/) as RegExpMatchArray)[0].length
      blocks.push(<p key={key++} className={lvl <= 2 ? 'mt-3 mb-1 text-sm font-bold text-foreground' : 'mt-2 mb-1 text-[13px] font-semibold text-foreground'}>{inline(line.replace(/^#+\s/, ''), `h${key}`)}</p>)
      i++
      continue
    }
    // 分割线
    if (/^(-{3,}|\*{3,})$/.test(line.trim())) { blocks.push(<hr key={key++} className="my-2 border-border/60" />); i++; continue }
    // 列表
    if (/^\s*[-*]\s+/.test(line)) {
      const items: string[] = []
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i])) { items.push(lines[i].replace(/^\s*[-*]\s+/, '')); i++ }
      blocks.push(<ul key={key++} className="my-1.5 list-disc pl-5 space-y-1 marker:text-muted-foreground">{items.map((it, ii) => <li key={ii} className="text-foreground">{inline(it, `li${key}-${ii}`)}</li>)}</ul>)
      continue
    }
    // 空行
    if (line.trim() === '') { i++; continue }
    // 段落
    blocks.push(<p key={key++} className="my-1 leading-relaxed text-foreground break-words">{inline(line, `p${key}`)}</p>)
    i++
  }
  return <div className="text-sm">{blocks}</div>
}

export default Markdown

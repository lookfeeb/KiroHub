import type { ReactNode } from 'react'
import { messages } from './locales/zh-CN'

export type TFunc = (key: string, vars?: Record<string, string | number>) => string

const NESTED = /\$t\(([^)]+)\)/g
const INTERP = /\{\{\s*(\w+)\s*\}\}/g

// 本地极简翻译：查表 → 解析 $t() 嵌套 → 填充 {{var}}
export const t: TFunc = (key, vars) => {
  let s = messages[key]
  if (s == null) return key
  s = s.replace(NESTED, (_m, k: string) => messages[k.trim()] ?? k.trim())
  if (vars) s = s.replace(INTERP, (_m, name: string) => (vars[name] != null ? String(vars[name]) : `{{${name}}}`))
  return s
}

// 兼容旧的 react-i18next 调用方式
export function useTranslation() {
  return { t, i18n: { language: 'zh-CN', changeLanguage: async () => {} } }
}

// 透传 Provider，保持 main.tsx 不变
export function I18nProvider({ children }: { children: ReactNode }) {
  return <>{children}</>
}

export default { t }

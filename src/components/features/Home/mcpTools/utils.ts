import { AlertTriangle, KeyRound, ShieldCheck, XCircle } from 'lucide-react'
import type { AuthMeta, McpClient, McpOAuthStatus } from './types'

export const CLIENTS = [
  { key: 'codex', label: 'Codex' },
  { key: 'kiro', label: 'Kiro' },
  { key: 'claude-cli', label: 'Claude CLI' },
] as const

export function isRemoteType(type: string) {
  return ['url', 'http', 'sse'].includes(type)
}

export function authKey(client: McpClient, name: string) {
  return `${client}:${name}`
}

export function refreshKey(client: McpClient, name: string) {
  return `${authKey(client, name)}:refresh`
}

export function copyKey(client: McpClient, name: string, toClient: McpClient) {
  return `${authKey(client, name)}:copy:${toClient}`
}

export function compactClientLabel(label: string) {
  return label.replace('Claude CLI', 'Claude')
}

export function authMeta(status?: McpOAuthStatus): AuthMeta {
  if (!status?.authorized) {
    return { label: '未授权', cls: 'bg-muted text-muted-foreground', icon: KeyRound }
  }
  if (status.needsReauth || status.expired) {
    return { label: '需重新授权', cls: 'bg-red-500/12 text-red-600', icon: XCircle }
  }
  if (status.refreshFailed) {
    return { label: '刷新失败', cls: 'bg-orange-500/12 text-orange-600', icon: AlertTriangle }
  }
  if (status.expiringSoon) {
    return { label: '即将过期', cls: 'bg-amber-500/12 text-amber-600', icon: AlertTriangle }
  }
  return { label: '已授权', cls: 'bg-green-500/12 text-green-600', icon: ShieldCheck }
}

import type { LucideIcon } from 'lucide-react'

export type McpClient = 'kiro' | 'codex' | 'claude-cli'
export type McpServerType = 'command' | 'url' | 'http' | 'sse'

export interface McpServerItem {
  name: string;
  client: McpClient;
  type: McpServerType;
  detail: string;
  disabled: boolean;
}

export interface McpOAuthStatus {
  authorized: boolean;
  expiresAt: number;
  expiringSoon: boolean;
  expired: boolean;
  refreshFailed: boolean;
  needsReauth: boolean;
  credentialKey?: string | null;
  message?: string | null;
}

export interface McpStats {
  totalServers: number;
  enabledServers: number;
  estimatedTools: number;
}

export interface McpClientOption {
  key: McpClient;
  label: string;
}

export interface AuthMeta {
  label: string;
  cls: string;
  icon: LucideIcon;
}

export const AI_MODELS = [
  { value: 'auto', label: 'Auto (智能选择) - 1.0x', recommended: true },
  { value: 'claude-opus-4.7', label: 'Claude Opus 4.7 (1M) - 2.2x', recommended: false },
  { value: 'claude-opus-4.6', label: 'Claude Opus 4.6 (1M) - 2.2x', recommended: false },
  { value: 'claude-opus-4.5', label: 'Claude Opus 4.5 (200K) - 2.2x', recommended: false },
  { value: 'claude-sonnet-4.6', label: 'Claude Sonnet 4.6 (1M) - 1.3x', recommended: false },
  { value: 'claude-sonnet-4.5', label: 'Claude Sonnet 4.5 (200K) - 1.3x', recommended: false },
  { value: 'claude-sonnet-4', label: 'Claude Sonnet 4.0 (200K) - 1.3x', recommended: false },
  { value: 'claude-haiku-4.5', label: 'Claude Haiku 4.5 (200K) - 0.4x', recommended: false },
  { value: 'deepseek-3.2', label: 'DeepSeek 3.2 (128K) - 0.25x', recommended: false },
  { value: 'minimax-m2.5', label: 'MiniMax M2.5 (200K) - 0.25x', recommended: false },
  { value: 'glm-5', label: 'GLM-5 (200K) - 0.5x', recommended: false },
  { value: 'minimax-m2.1', label: 'MiniMax M2.1 (200K) - 0.15x', recommended: false },
  { value: 'qwen3-coder-next', label: 'Qwen3 Coder Next (256K) - 0.05x', recommended: false },
]

export const NOTIFICATION_SETTINGS_FIELD_MAP = {
  'kiroAgent.notifications.agent.actionRequired': 'notifyActionRequired',
  'kiroAgent.notifications.agent.failure': 'notifyFailure',
  'kiroAgent.notifications.agent.success': 'notifySuccess',
  'kiroAgent.notifications.billing': 'notifyBilling'
}


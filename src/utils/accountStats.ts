// 账号统计计算工具函数
import { isActiveStatus, isBannedStatus, isCappedStatus, isUnavailableStatus } from './accountStatus'
import { getSafeAccountDisplayName } from '../components/features/AccountManager/utils/accountRuntime'

// 获取账号显示名称（email 或 userId）
export const getAccountDisplayName = (account) => {
  return getSafeAccountDisplayName(account)
}

// 智能格式化使用量：整数显示整数，小数保留2位（去掉末尾0）
const formatUsage = (value) => {
  if (value == null) return '0'
  if (Number.isInteger(value)) return value.toString()
  return parseFloat(value.toFixed(2)).toString()
}

// 从 account 获取 breakdown（API 返回 camelCase）
const getBreakdown = (a) => {
  return a.usageData?.usageBreakdownList?.[0] || null
}

// 获取总配额（主配额 + 未过期的试用 + 未过期的奖励）
const getQuota = (a) => {
  if (isUnavailableStatus(a) && !isCappedStatus(a)) return 0

  const breakdown = getBreakdown(a)
  if (!breakdown) return a.quota ?? 0
  
  const now = Date.now()
  const main = breakdown.usageLimit ?? 0
  
  // 检查试用是否激活（只看状态，不看日期）
  const trialInfo = breakdown.freeTrialInfo
  const trialActive = trialInfo?.freeTrialStatus === 'ACTIVE'
  const freeTrial = trialActive ? (trialInfo?.usageLimit ?? 0) : 0
  
  // 检查每个奖励配额（只计入未过期且状态为 ACTIVE 的奖励）
  const bonuses = Array.isArray(breakdown.bonuses) ? breakdown.bonuses : []
  let bonus = 0
  bonuses.forEach(b => {
    const expiry = b.expiresAt ? b.expiresAt * 1000 : Infinity
    if (expiry > now && b.status === 'ACTIVE') {
      bonus += b.usageLimit ?? 0
    }
  })
  
  return main + freeTrial + bonus
}

// 获取已使用量（主配额 + 未过期的试用 + 未过期的奖励）
const getUsed = (a) => {
  if (isUnavailableStatus(a) && !isCappedStatus(a)) return 0

  const breakdown = getBreakdown(a)
  if (!breakdown) return a.used ?? 0
  
  const now = Date.now()
  const main = breakdown.currentUsage ?? 0
  
  // 检查试用是否激活（只看状态，不看日期）
  const trialInfo = breakdown.freeTrialInfo
  const trialActive = trialInfo?.freeTrialStatus === 'ACTIVE'
  const freeTrial = trialActive ? (trialInfo?.currentUsage ?? 0) : 0
  
  // 检查每个奖励配额（只计入未过期且状态为 ACTIVE 的奖励）
  const bonuses = Array.isArray(breakdown.bonuses) ? breakdown.bonuses : []
  let bonus = 0
  bonuses.forEach(b => {
    const expiry = b.expiresAt ? b.expiresAt * 1000 : Infinity
    if (expiry > now && b.status === 'ACTIVE') {
      bonus += b.currentUsage ?? 0
    }
  })
  
  return main + freeTrial + bonus
}
const getSubType = (a) => a.usageData?.subscriptionInfo?.type ?? a.subscriptionType ?? ''
const getSubPlan = (a) => a.usageData?.subscriptionInfo?.subscriptionTitle ?? a.subscriptionPlan ?? ''

// 账号是否开启了超额（透支）
const isOverageEnabled = (a) => {
  const u = a.usageData
  return u?.subscriptionInfo?.overageCapability === 'OVERAGE_CAPABLE'
    && u?.overageConfiguration?.overageStatus === 'ENABLED'
}

// 合并配额信息：开启超额(透支)时把超额上限并入总配额，使用量并入超额用量
const getMergedQuota = (a) => {
  const breakdown = getBreakdown(a)
  const overageEnabled = isOverageEnabled(a)
  const baseQuota = getQuota(a)
  const baseUsed = getUsed(a)
  const overageCap = breakdown?.overageCapWithPrecision ?? breakdown?.overageCap ?? 0
  const overageUsed = breakdown?.currentOveragesWithPrecision ?? breakdown?.currentOverages ?? 0
  const quota = overageEnabled ? baseQuota + overageCap : baseQuota
  const used = overageEnabled ? baseUsed + overageUsed : baseUsed
  const remaining = Math.max(0, quota - used)
  const percent = quota > 0 ? Math.round((used / quota) * 100) : 0
  return { quota, used, remaining, percent, overageEnabled, baseQuota, baseUsed, overageCap, overageUsed }
}

export function calcAccountStats(accounts) {
  const total = accounts.length
  const active = accounts.filter(a => isActiveStatus(a)).length
  const banned = accounts.filter(a => isBannedStatus(a)).length
  const unavailable = accounts.filter(a => isUnavailableStatus(a)).length
  // 保留精确值，不再取整
  const totalQuota = accounts.reduce((sum, a) => sum + getQuota(a), 0)
  const totalUsed = accounts.reduce((sum, a) => sum + getUsed(a), 0)
  const proPlus = accounts.filter(a => getSubType(a).includes('PRO+') || getSubPlan(a).includes('PRO+')).length
  const pro = accounts.filter(a => 
    (getSubType(a).includes('PRO') || getSubPlan(a).includes('PRO')) && 
    !(getSubType(a).includes('PRO+') || getSubPlan(a).includes('PRO+'))
  ).length
  const usagePercent = totalQuota > 0 ? Number((totalUsed / totalQuota * 100).toFixed(1)) : 0

  return { 
    total, active, banned, unavailable, proPlus, pro, usagePercent,
    totalQuota, totalUsed, remaining: totalQuota - totalUsed,
    // 格式化后的显示值
    totalQuotaStr: formatUsage(totalQuota),
    totalUsedStr: formatUsage(totalUsed),
    remainingStr: formatUsage(totalQuota - totalUsed)
  }
}

export function getUsagePercent(used, quota) {
  return quota === 0 ? 0 : Math.min(100, (used / quota) * 100)
}

export { getQuota, getUsed, getSubType, getSubPlan, formatUsage, getMergedQuota, isOverageEnabled }

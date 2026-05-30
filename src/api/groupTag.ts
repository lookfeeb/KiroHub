// 分组与标签 API 调用
import { invoke } from '@tauri-apps/api/core'
import type { TagDefinition, GroupDefinition } from '../types/account'

// ============================================================
// 分组操作
// ============================================================

export async function getGroups() {
  return await invoke<GroupDefinition[]>('get_groups')
}

export async function addGroup(name, color = null) {
  return await invoke('add_group', { name, color })
}

export async function updateGroup(id, name = null, color = null) {
  return await invoke('update_group', { id, name, color })
}

export async function deleteGroup(id) {
  return await invoke('delete_group', { id })
}

export async function reorderGroups(ids) {
  return await invoke('reorder_groups', { ids })
}

// ============================================================
// 标签操作
// ============================================================

export async function getTags() {
  return await invoke<TagDefinition[]>('get_tags')
}

export async function addTag(name, color) {
  return await invoke('add_tag', { name, color })
}

export async function updateTag(id, name = null, color = null) {
  return await invoke('update_tag', { id, name, color })
}

export async function deleteTag(id) {
  return await invoke('delete_tag', { id })
}

// ============================================================
// 账号分组/标签关联
// ============================================================

export async function setAccountGroup(accountId, groupId = null) {
  return await invoke('set_account_group', { accountId, groupId })
}

export async function addTagToAccount(accountId, tagId) {
  return await invoke('add_tag_to_account', { accountId, tagId })
}

export async function removeTagFromAccount(accountId, tagId) {
  return await invoke('remove_tag_from_account', { accountId, tagId })
}

export async function setAccountTags(accountId, tagIds) {
  return await invoke('set_account_tags', { accountId, tagIds })
}

// 批量移除账号的指定标签
export async function removeAccountTags(accountId, tagIds) {
  return await invoke('remove_account_tags', { accountId, tagIds })
}

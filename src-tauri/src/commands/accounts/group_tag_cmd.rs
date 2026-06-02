// 分组与标签管理命令

#![allow(clippy::needless_pass_by_value)] // Tauri 命令的 String 参数需要按值传递（框架序列化要求）

use crate::commands::common::{lock_store, save_store};
use crate::core::account::{Account, AccountGroup, AccountTag, AccountTagLink};
use crate::state::AppState;
use std::collections::HashMap;
use tauri::State;

fn clear_group_references(accounts: &mut [Account], id: &str) {
    for account in accounts {
        if account.group_id.as_deref() == Some(id) {
            account.group_id = None;
        }
    }
}

fn remove_tag_references(accounts: &mut [Account], id: &str) {
    for account in accounts {
        remove_tag_links(account, &[id.to_string()]);
    }
}

fn add_tag_link_if_missing(account: &mut Account, tag_id: &str, tag_name: Option<String>) -> bool {
    if account.tag_links.iter().any(|link| link.tag_id == tag_id) {
        return false;
    }

    account
        .tag_links
        .push(AccountTagLink::new(tag_id.to_string(), tag_name));
    true
}

fn remove_tag_links(account: &mut Account, tag_ids: &[String]) {
    account
        .tag_links
        .retain(|link| !tag_ids.contains(&link.tag_id));
}

fn replace_account_tag_links(
    account: &mut Account,
    tag_ids: &[String],
    tag_names: &HashMap<String, String>,
) {
    let existing_ids: Vec<String> = account
        .tag_links
        .iter()
        .map(|link| link.tag_id.clone())
        .collect();

    account
        .tag_links
        .retain(|link| tag_ids.contains(&link.tag_id));

    for tag_id in tag_ids {
        if !existing_ids.contains(tag_id) {
            let tag_name = tag_names.get(tag_id).cloned();
            let _ = add_tag_link_if_missing(account, tag_id, tag_name);
        }
    }
}

// ============================================================
// 分组命令
// ============================================================

#[tauri::command]
pub fn get_groups(state: State<AppState>) -> Vec<AccountGroup> {
    match lock_store(&state.group_tag_store, "group_tag_store") {
        Ok(store) => store.get_groups(),
        Err(err) => {
            eprintln!("[group_tag_cmd] {err}");
            Vec::new()
        }
    }
}

#[tauri::command]
pub fn add_group(
    state: State<AppState>,
    name: String,
    color: Option<String>,
) -> Result<AccountGroup, String> {
    lock_store(&state.group_tag_store, "group_tag_store")?.add_group(name, color)
}

#[tauri::command]
pub fn update_group(
    state: State<AppState>,
    id: &str,
    name: Option<String>,
    color: Option<String>,
) -> Result<AccountGroup, String> {
    lock_store(&state.group_tag_store, "group_tag_store")?.update_group(id, name, color)
}

#[tauri::command]
pub fn delete_group(state: State<AppState>, id: &str) -> bool {
    // 删除分组时，清除所有账号的 group_id
    let mut store = match lock_store(&state.store, "store") {
        Ok(store) => store,
        Err(err) => {
            eprintln!("[group_tag_cmd] {err}");
            return false;
        }
    };
    clear_group_references(&mut store.accounts, id);
    if let Err(err) = save_store(&store) {
        eprintln!("[group_tag_cmd] {err}");
        return false;
    }
    drop(store);
    match lock_store(&state.group_tag_store, "group_tag_store") {
        Ok(mut store) => match store.delete_group(id) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("[group_tag_cmd] {err}");
                false
            }
        },
        Err(err) => {
            eprintln!("[group_tag_cmd] {err}");
            false
        }
    }
}

#[tauri::command]
pub fn reorder_groups(state: State<AppState>, ids: Vec<String>) -> bool {
    match lock_store(&state.group_tag_store, "group_tag_store") {
        Ok(mut store) => match store.reorder_groups(&ids) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("[group_tag_cmd] {err}");
                false
            }
        },
        Err(err) => {
            eprintln!("[group_tag_cmd] {err}");
            false
        }
    }
}

// ============================================================
// 标签命令
// ============================================================

#[tauri::command]
pub fn get_tags(state: State<AppState>) -> Vec<AccountTag> {
    match lock_store(&state.group_tag_store, "group_tag_store") {
        Ok(store) => store.get_tags(),
        Err(err) => {
            eprintln!("[group_tag_cmd] {err}");
            Vec::new()
        }
    }
}

#[tauri::command]
pub fn add_tag(state: State<AppState>, name: String, color: String) -> Result<AccountTag, String> {
    lock_store(&state.group_tag_store, "group_tag_store")?.add_tag(name, color)
}

#[tauri::command]
pub fn update_tag(
    state: State<AppState>,
    id: &str,
    name: Option<String>,
    color: Option<String>,
) -> Result<AccountTag, String> {
    lock_store(&state.group_tag_store, "group_tag_store")?.update_tag(id, name, color)
}

#[tauri::command]
pub fn delete_tag(state: State<AppState>, id: &str) -> bool {
    // 删除标签时，从所有账号中移除该标签
    let mut store = match lock_store(&state.store, "store") {
        Ok(store) => store,
        Err(err) => {
            eprintln!("[group_tag_cmd] {err}");
            return false;
        }
    };
    remove_tag_references(&mut store.accounts, id);
    if let Err(err) = save_store(&store) {
        eprintln!("[group_tag_cmd] {err}");
        return false;
    }
    drop(store);
    match lock_store(&state.group_tag_store, "group_tag_store") {
        Ok(mut store) => match store.delete_tag(id) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("[group_tag_cmd] {err}");
                false
            }
        },
        Err(err) => {
            eprintln!("[group_tag_cmd] {err}");
            false
        }
    }
}

// ============================================================
// 账号分组/标签关联命令
// ============================================================

#[tauri::command]
pub fn set_account_group(
    state: State<AppState>,
    account_id: &str,
    group_id: Option<String>,
) -> Result<(), String> {
    let mut store = lock_store(&state.store, "store")?;
    if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
        account.group_id = group_id;
        save_store(&store)?;
        Ok(())
    } else {
        Err("账号不存在".to_string())
    }
}

#[tauri::command]
pub fn add_tag_to_account(
    state: State<AppState>,
    account_id: &str,
    tag_id: &str,
) -> Result<(), String> {
    // 先获取标签名
    let tag_name = {
        let gt_store = lock_store(&state.group_tag_store, "group_tag_store")?;
        gt_store
            .get_tags()
            .iter()
            .find(|t| t.id == tag_id)
            .map(|t| t.name.clone())
    };

    let mut store = lock_store(&state.store, "store")?;
    if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
        if add_tag_link_if_missing(account, tag_id, tag_name) {
            save_store(&store)?;
        }
        Ok(())
    } else {
        Err("账号不存在".to_string())
    }
}

#[tauri::command]
pub fn remove_tag_from_account(
    state: State<AppState>,
    account_id: &str,
    tag_id: &str,
) -> Result<(), String> {
    let mut store = lock_store(&state.store, "store")?;
    if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
        remove_tag_links(account, &[tag_id.to_string()]);
        save_store(&store)?;
        Ok(())
    } else {
        Err("账号不存在".to_string())
    }
}

#[tauri::command]
pub fn set_account_tags(
    state: State<AppState>,
    account_id: &str,
    tag_ids: Vec<String>,
) -> Result<(), String> {
    // 先获取所有标签名
    let tag_names: HashMap<String, String> = {
        let gt_store = lock_store(&state.group_tag_store, "group_tag_store")?;
        gt_store
            .get_tags()
            .iter()
            .map(|t| (t.id.clone(), t.name.clone()))
            .collect()
    };

    let mut store = lock_store(&state.store, "store")?;
    if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
        replace_account_tag_links(account, &tag_ids, &tag_names);
        save_store(&store)?;
        Ok(())
    } else {
        Err("账号不存在".to_string())
    }
}

#[tauri::command]
pub fn remove_account_tags(
    state: State<AppState>,
    account_id: &str,
    tag_ids: Vec<String>,
) -> Result<(), String> {
    let mut store = lock_store(&state.store, "store")?;
    if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
        remove_tag_links(account, &tag_ids);
        save_store(&store)?;
        Ok(())
    } else {
        Err("账号不存在".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        add_tag_link_if_missing, clear_group_references, remove_tag_links, remove_tag_references,
        replace_account_tag_links,
    };
    use crate::core::account::{Account, AccountTagLink};
    use std::collections::HashMap;

    #[test]
    fn clear_group_references_only_resets_matching_group_ids() {
        let mut matching = Account::new("match@example.com".to_string(), "match".to_string());
        matching.group_id = Some("group-a".to_string());

        let mut different = Account::new("other@example.com".to_string(), "other".to_string());
        different.group_id = Some("group-b".to_string());

        let mut accounts = vec![matching, different];

        clear_group_references(&mut accounts, "group-a");

        assert_eq!(accounts[0].group_id, None);
        assert_eq!(accounts[1].group_id.as_deref(), Some("group-b"));
    }

    #[test]
    fn remove_tag_references_only_removes_matching_links() {
        let mut account = Account::new("tag@example.com".to_string(), "tag".to_string());
        account.tag_links = vec![
            AccountTagLink::new("tag-a".to_string(), Some("A".to_string())),
            AccountTagLink::new("tag-b".to_string(), Some("B".to_string())),
        ];

        let mut accounts = vec![account];
        remove_tag_references(&mut accounts, "tag-a");

        assert_eq!(accounts[0].tag_links.len(), 1);
        assert_eq!(accounts[0].tag_links[0].tag_id, "tag-b");
    }

    #[test]
    fn replace_account_tag_links_preserves_existing_links_and_adds_new_ones() {
        let mut account = Account::new("set@example.com".to_string(), "set".to_string());
        let existing = AccountTagLink {
            tag_id: "tag-a".to_string(),
            tag_name: Some("A".to_string()),
            linked_at: "2026-01-01 10:00".to_string(),
        };
        account.tag_links = vec![
            existing.clone(),
            AccountTagLink {
                tag_id: "tag-b".to_string(),
                tag_name: Some("B".to_string()),
                linked_at: "2026-01-02 10:00".to_string(),
            },
        ];

        let tag_names = HashMap::from([
            ("tag-a".to_string(), "A".to_string()),
            ("tag-c".to_string(), "C".to_string()),
        ]);

        replace_account_tag_links(
            &mut account,
            &["tag-a".to_string(), "tag-c".to_string()],
            &tag_names,
        );

        assert_eq!(account.tag_links.len(), 2);
        assert_eq!(account.tag_links[0].tag_id, "tag-a");
        assert_eq!(account.tag_links[0].linked_at, existing.linked_at);
        assert_eq!(account.tag_links[1].tag_id, "tag-c");
        assert_eq!(account.tag_links[1].tag_name.as_deref(), Some("C"));
    }

    #[test]
    fn add_tag_link_if_missing_only_appends_new_tag_ids() {
        let mut account = Account::new("append@example.com".to_string(), "append".to_string());
        account.tag_links = vec![AccountTagLink {
            tag_id: "tag-a".to_string(),
            tag_name: Some("A".to_string()),
            linked_at: "2026-01-01 10:00".to_string(),
        }];

        assert!(!add_tag_link_if_missing(
            &mut account,
            "tag-a",
            Some("A".to_string())
        ));
        assert!(add_tag_link_if_missing(
            &mut account,
            "tag-b",
            Some("B".to_string())
        ));
        assert_eq!(account.tag_links.len(), 2);
        assert_eq!(account.tag_links[1].tag_id, "tag-b");
    }

    #[test]
    fn remove_tag_links_only_drops_requested_ids() {
        let mut account = Account::new("remove@example.com".to_string(), "remove".to_string());
        account.tag_links = vec![
            AccountTagLink::new("tag-a".to_string(), Some("A".to_string())),
            AccountTagLink::new("tag-b".to_string(), Some("B".to_string())),
            AccountTagLink::new("tag-c".to_string(), Some("C".to_string())),
        ];

        remove_tag_links(&mut account, &["tag-a".to_string(), "tag-c".to_string()]);

        assert_eq!(account.tag_links.len(), 1);
        assert_eq!(account.tag_links[0].tag_id, "tag-b");
    }
}

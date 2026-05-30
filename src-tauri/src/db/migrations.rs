//! Schema 迁移：以 `PRAGMA user_version` 作为 schema 版本号（与“数据迁移完成标记”
//! `migrated_from_json` 区分，后者存于 `app_meta` KV 表，由阶段5 迁移器写入）。
//!
//! 迁移以「版本号 -> SQL」有序列表表达，启动时按当前 user_version 顺序补齐。
//! 仅建表，不写入业务数据，零行为变更。

use rusqlite::Connection;

/// 迁移步骤数量（= 最新 schema 版本），供初始化校验使用。
#[allow(dead_code)]
pub const MIGRATIONS_LEN: usize = MIGRATIONS.len();

/// 迁移步骤列表：索引即目标版本（从 1 开始）。新增 schema 变更时在末尾追加。
const MIGRATIONS: &[&str] = &[
    // v1：初始 schema
    r#"
    -- 账号：核心列用于查询/排序，整行 JSON 存 data 列以免丢字段（阶段2 再决定拆列）
    CREATE TABLE IF NOT EXISTS accounts (
        id            TEXT PRIMARY KEY,
        email         TEXT,
        user_id       TEXT,
        label         TEXT NOT NULL DEFAULT '',
        status        TEXT NOT NULL DEFAULT 'active',
        group_id      TEXT,
        enabled       INTEGER NOT NULL DEFAULT 1,
        added_at      TEXT,
        position      INTEGER NOT NULL DEFAULT 0,
        data          TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_accounts_group  ON accounts(group_id);
    CREATE INDEX IF NOT EXISTS idx_accounts_status ON accounts(status);

    CREATE TABLE IF NOT EXISTS account_groups (
        id            TEXT PRIMARY KEY,
        name          TEXT NOT NULL,
        color         TEXT,
        "order"       INTEGER NOT NULL DEFAULT 0,
        created_at    TEXT
    );

    CREATE TABLE IF NOT EXISTS account_tags (
        id            TEXT PRIMARY KEY,
        name          TEXT NOT NULL,
        color         TEXT NOT NULL DEFAULT '',
        created_at    TEXT
    );

    -- 账号↔标签关联（拆关联表 + 级联删除）
    CREATE TABLE IF NOT EXISTS account_tag_links (
        account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        tag_id        TEXT NOT NULL REFERENCES account_tags(id) ON DELETE CASCADE,
        tag_name      TEXT,
        linked_at     TEXT,
        PRIMARY KEY (account_id, tag_id)
    );
    CREATE INDEX IF NOT EXISTS idx_tag_links_tag ON account_tag_links(tag_id);

    -- 通用 KV（app_settings / gateway_config / mcp_oauth / 数据迁移标记 app_meta）
    CREATE TABLE IF NOT EXISTS app_settings   (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS gateway_config (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS mcp_oauth      (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS app_meta       (key TEXT PRIMARY KEY, value TEXT NOT NULL);

    -- 使用历史（时序）
    CREATE TABLE IF NOT EXISTS usage_history (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        account_id    TEXT,
        recorded_at   TEXT NOT NULL,
        data          TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_usage_account ON usage_history(account_id);
    CREATE INDEX IF NOT EXISTS idx_usage_time    ON usage_history(recorded_at);

    -- 网关请求日志（高频写，阶段4 走异步批量写）
    CREATE TABLE IF NOT EXISTS gateway_request_log (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        ts            TEXT NOT NULL,
        account_id    TEXT,
        data          TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_gwlog_ts ON gateway_request_log(ts);

    -- 响应缓存：大 BLOB 留磁盘，仅元数据入库（§9）
    CREATE TABLE IF NOT EXISTS response_cache_meta (
        cache_key     TEXT PRIMARY KEY,
        session_id    TEXT,
        created_at    INTEGER NOT NULL,
        expires_at    INTEGER NOT NULL,
        input_tokens  INTEGER,
        output_tokens INTEGER,
        message_count INTEGER,
        total_chars   INTEGER
    );
    CREATE INDEX IF NOT EXISTS idx_cache_expires ON response_cache_meta(expires_at);
    "#,
];

/// 按 `user_version` 顺序补齐迁移。幂等：已应用的版本不再执行。
pub fn run(conn: &Connection) -> rusqlite::Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let target = MIGRATIONS.len() as i64;
    for v in current..target {
        conn.execute_batch(MIGRATIONS[v as usize])?;
    }
    if target > current {
        // user_version 不支持参数绑定，需内联（target 为内部常量，无注入风险）
        conn.execute_batch(&format!("PRAGMA user_version = {target};"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_and_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
        // 再跑一次不应报错、版本不变
        run(&conn).unwrap();
        let v2: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, v2);
        // 关键表存在
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='accounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn integration_cascade_kv_and_usage_history() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        run(&conn).unwrap();

        // account_tag_links 随账号删除级联清空
        conn.execute_batch(
            "INSERT INTO accounts(id,label,data) VALUES('a','l','{}');
             INSERT INTO account_tags(id,name) VALUES('t','tag');
             INSERT INTO account_tag_links(account_id,tag_id) VALUES('a','t');",
        )
        .unwrap();
        let links: i64 = conn
            .query_row("SELECT count(*) FROM account_tag_links", [], |r| r.get(0))
            .unwrap();
        assert_eq!(links, 1);
        conn.execute("DELETE FROM accounts WHERE id='a'", []).unwrap();
        let links_after: i64 = conn
            .query_row("SELECT count(*) FROM account_tag_links", [], |r| r.get(0))
            .unwrap();
        assert_eq!(links_after, 0, "删除账号应级联清空标签关联");

        // KV upsert（与 kv_set 同义）
        for v in ["v1", "v2"] {
            conn.execute(
                "INSERT INTO app_settings(key,value) VALUES('k',?1) \
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [v],
            )
            .unwrap();
        }
        let v: String = conn
            .query_row("SELECT value FROM app_settings WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, "v2");

        // usage_history 按 recorded_at 排序
        conn.execute_batch(
            "INSERT INTO usage_history(recorded_at,data) VALUES('2026-05-02','b');
             INSERT INTO usage_history(recorded_at,data) VALUES('2026-05-01','a');",
        )
        .unwrap();
        let first: String = conn
            .query_row(
                "SELECT data FROM usage_history ORDER BY recorded_at LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(first, "a");
    }

    #[test]
    fn vacuum_into_produces_consistent_single_file_snapshot() {
        let dir = std::env::temp_dir().join(format!("kirohub_bak_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        conn.execute("INSERT INTO app_settings(key,value) VALUES('k','v')", [])
            .unwrap();

        let dest = dir.join("snap.db");
        let dest_lit = dest.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!("VACUUM INTO '{dest_lit}'")).unwrap();
        assert!(dest.exists(), "VACUUM INTO 应生成单文件快照");

        let snap = Connection::open(&dest).unwrap();
        let v: String = snap
            .query_row("SELECT value FROM app_settings WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, "v");
        drop(snap);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

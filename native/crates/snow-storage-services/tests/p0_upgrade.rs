mod p0_support;

// 临时库隔离 SQLite；测试进程的 snow-support panic 日志须由公共测试环境单独隔离。

use std::path::Path;

use rusqlite::{params, types::FromSql, Connection};
use serde_json::{json, Value};
use snow_storage_services::{
    database::{ensure_database, open_connection},
    services::{
        custom_header_schemes, mcp_server_configs, project_mcp_server_configs, project_memories,
        sensitive_command_feedback as feedback,
    },
};

const V69: &str = include_str!("fixtures/p0-upgrade/v69.sql");
const V70_FEEDBACK: &str = include_str!("fixtures/p0-upgrade/v70-feedback.sql");
const PROJECT: &str = "p0-project";
const CHAT: &str = "p0-chat";

fn scalar<T: FromSql>(conn: &Connection, sql: &str) -> rusqlite::Result<T> {
    conn.query_row(sql, [], |row| row.get(0))
}

// 只用独立 SQLite 连接造旧库；返回时已关闭，首次生产启动一定读到磁盘上的旧结构。
fn seed_legacy(path: &Path, version: u32) -> rusqlite::Result<()> {
    assert!(!path.exists());
    let conn = Connection::open(path)?;
    conn.execute_batch(V69)?;
    match version {
        69 => (),
        70 => conn.execute_batch(V70_FEEDBACK)?,
        _ => panic!("没有 v{version} 历史样本"),
    }

    // 按历史键格式写入，不走新版序列化器，否则会把要验证缺省行为的 oauth 字段补上。
    let key = format!(
        "project_mcp_server_configs_{}",
        blake3::hash(PROJECT.as_bytes()).to_hex()
    );
    let old_project_json = json!({
        "projectId": PROJECT,
        "servers": [{
            "serverId": "old-project-http", "name": "P0 project", "transportType": "http",
            "url": "https://p0.invalid/project", "command": "", "argsJson": "[]", "envJson": "{}",
            "headersJson": r#"{"X-P0-Project":"fixture-project"}"#, "enabled": true,
            "timeoutMs": 7500, "sortOrder": 0, "updatedAt": "2024-01-02 03:04:05"
        }]
    });
    conn.execute(
        "INSERT INTO system_settings (id, setting_name, setting_code, setting_value)
         VALUES ('project-mcp-row', 'Project MCP server configs', ?1, ?2)",
        params![key, old_project_json.to_string()],
    )?;

    assert_eq!(scalar::<u32>(&conn, "PRAGMA user_version")?, version);
    assert!(!scalar::<bool>(
        &conn,
        "SELECT EXISTS (SELECT 1 FROM pragma_table_info('mcp_server_configs') WHERE name = 'oauth')",
    )?);
    assert_eq!(
        scalar::<bool>(
            &conn,
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema
                            WHERE type = 'table' AND name = 'sensitive_command_feedback')",
        )?,
        version == 70
    );
    Ok(())
}

// 由生产入口建库/升级，再用生产连接检查连接级 pragma；不在被测连接上手动启用外键。
fn boot_v71(path: &Path) -> rusqlite::Result<()> {
    ensure_database(path).expect("生产入口应能初始化或升级合成库");
    let mut conn = open_connection(path)?;
    assert_eq!(scalar::<i64>(&conn, "PRAGMA user_version")?, 71);
    assert_eq!(scalar::<i64>(&conn, "PRAGMA foreign_keys")?, 1);
    assert!(scalar::<String>(&conn, "PRAGMA journal_mode")?.eq_ignore_ascii_case("wal"));
    assert_eq!(scalar::<String>(&conn, "PRAGMA quick_check")?, "ok");
    assert_eq!(
        scalar::<i64>(&conn, "SELECT count(*) FROM pragma_foreign_key_check")?,
        0
    );
    assert!(scalar::<bool>(
        &conn,
        "SELECT EXISTS (SELECT 1 FROM pragma_index_list('sensitive_command_feedback')
                        WHERE name = 'idx_sensitive_command_feedback_created')",
    )?);

    // 验证实际缺省写入（旧行回填为 0 不代表新行的 DEFAULT 也正确）。
    let tx = conn.transaction()?;
    let oauth = scalar::<i64>(
        &tx,
        "INSERT INTO mcp_server_configs (id, server_id) VALUES ('default-probe', 'probe')
         RETURNING oauth",
    )?;
    assert_eq!(oauth, 0);
    tx.rollback()?;

    let err = conn
        .execute(
            "INSERT INTO chat_messages (id, message_id, conversation_id, role, content)
             VALUES ('orphan-probe', 'orphan-message', 'missing-parent', 'user', 'fixture')",
            [],
        )
        .expect_err("没有父会话的消息必须被外键拒绝");
    assert!(
        matches!(err, rusqlite::Error::SqliteFailure(ref e, _)
            if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY),
        "预期外键约束错误，实际为 {err}"
    );
    Ok(())
}

fn restart_v71(path: &Path) -> rusqlite::Result<()> {
    // 此前串行使用一条连接。暂时借住它，令生产入口用另一条连接重开同一文件，
    // 而不清空整个进程的连接池或改变其他测试的状态。
    let _previous_connection = open_connection(path)?;
    boot_v71(path)
}

fn assert_legacy_configs(path: &Path, expected_global_oauth: bool) {
    let global = mcp_server_configs::list_mcp_server_configs(path).expect("读取全局 MCP");
    assert_eq!(global.len(), 1);
    let server = &global[0];
    assert_eq!(server.id, "old-mcp-row");
    assert_eq!(server.server_id, "old-global-http");
    assert_eq!(server.transport_type, "http");
    assert_eq!(server.url, "https://p0.invalid/global");
    assert_eq!(server.timeout_ms, Some(12500));
    assert_eq!(server.updated_at, "2024-01-02 03:04:05");
    assert_eq!(server.oauth, expected_global_oauth);
    assert!(!server.enabled, "升级不能重新启用旧服务器");
    assert_eq!(
        serde_json::from_str::<Value>(&server.headers_json).unwrap(),
        json!({"X-P0-MCP": "fixture-global"})
    );

    let schemes = custom_header_schemes::list_custom_header_schemes(path).expect("读取请求头方案");
    assert_eq!(schemes.len(), 1);
    assert_eq!(schemes[0].scheme_id, "old-header-scheme");
    assert!(schemes[0].is_active);
    assert_eq!(
        serde_json::from_str::<Value>(&schemes[0].headers_json).unwrap(),
        json!({"X-P0-Scheme": "fixture-scheme"})
    );

    let project = project_mcp_server_configs::list_project_mcp_server_configs(path, PROJECT)
        .expect("用生产接口反序列化缺少 oauth 的旧项目 JSON");
    assert_eq!(project.len(), 1);
    let server = &project[0];
    assert_eq!(server.server_id, "old-project-http");
    assert_eq!(server.url, "https://p0.invalid/project");
    assert!(server.enabled);
    assert!(!server.oauth, "启用 HTTP 服务器不等于启用 OAuth");
    assert_eq!(
        serde_json::from_str::<Value>(&server.headers_json).unwrap(),
        json!({"X-P0-Project": "fixture-project"})
    );
}

fn assert_memory_anchors(path: &Path) -> rusqlite::Result<()> {
    let notes = project_memories::list_memories_by_conversation(path, CHAT, 10)
        .expect("读取该会话贡献的记忆");
    assert_eq!(notes.len(), 2);
    let conn = open_connection(path)?;
    for (id, dir, scope, content, response, message) in [
        (
            "project-memory",
            PROJECT,
            "project",
            "project note",
            "response-a",
            "message-a",
        ),
        (
            "global-memory",
            "",
            "global",
            "global note",
            "response-b",
            "message-b",
        ),
    ] {
        let note = notes
            .iter()
            .find(|m| m.memory_id == id)
            .expect("历史记忆丢失");
        assert_eq!(note.directory_id, dir);
        assert_eq!(note.scope, scope);
        assert_eq!(note.content, content);
        assert_eq!(note.conversation_id, CHAT);
        assert_eq!(note.response_id, response);
        let linked_message: String = conn.query_row(
            "SELECT m.message_id FROM chat_messages m JOIN chat_conversations c USING (conversation_id)
             WHERE c.directory_id = ?1 AND c.conversation_id = ?2 AND m.response_id = ?3",
            params![PROJECT, note.conversation_id, note.response_id],
            |row| row.get(0),
        )?;
        assert_eq!(linked_message, message);
    }
    Ok(())
}

// 不只比较条数：原始决定、理由、项目、规则必须匹配，并返回主键/时间检查是否被重建。
fn feedback_stamp(path: &Path) -> rusqlite::Result<(String, String)> {
    assert_eq!(feedback::count_feedback(path).expect("读取反馈条数"), 1);
    open_connection(path)?.query_row(
        "SELECT id, created_at FROM sensitive_command_feedback
         WHERE project_id = ?1 AND command = 'Get-Location' AND rule_ids_json = '[\"p0-rule\"]'
           AND model_verdict = 'allow' AND model_reason = 'fixture model reason'
           AND user_decision = 'rejected' AND user_note = 'fixture user note'",
        [PROJECT],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
}

// 这是存储契约，不会执行任何工具。实际调用候选键另由 Node 测试覆盖。
#[test]
fn v69_project_permissions_and_agent_tools_do_not_expand_on_upgrade() -> rusqlite::Result<()> {
    use snow_storage_core::system_settings::list_tool_approval_project_approved_tools;
    use snow_storage_services::services::sub_agent_configs::get_sub_agent_config;

    let db = p0_support::TestDb::new();
    let path = db.path();
    seed_legacy(path, 69)?;
    {
        let conn = Connection::open(path)?;
        conn.execute_batch(include_str!("fixtures/p0-upgrade/v69-agent-tools.sql"))?;
        let key = format!(
            "project_tool_approval_scope_{}",
            blake3::hash(PROJECT.as_bytes())
        );
        let old =
            json!({"projectId": PROJECT, "approvedToolNames": ["terminal-send", "config-set"]});
        conn.execute(
            "INSERT INTO system_settings (id, setting_name, setting_code, setting_value)
             VALUES ('old-permissions-row', 'Project tool approval', ?1, ?2)",
            params![key, old.to_string()],
        )?;
    }
    for _ in 0..2 {
        ensure_database(path).expect("生产迁移必须保留已有名单");
        let mut names = list_tool_approval_project_approved_tools(path, PROJECT).unwrap();
        names.sort();
        assert_eq!(names, ["config-set", "terminal-send"]);
        let agent = get_sub_agent_config(path, "p0-inspector", Some(PROJECT))
            .unwrap()
            .expect("升级后找不到旧子代理");
        assert_eq!(
            serde_json::from_str::<Value>(&agent.tools_json).unwrap(),
            json!(["filesystem-read", "config-get", "terminal-list"])
        );
        assert!(list_tool_approval_project_approved_tools(path, "p0-other")
            .unwrap()
            .is_empty());
        assert!(get_sub_agent_config(path, "p0-inspector", Some("p0-other"))
            .unwrap()
            .is_none());
        // 确保下一次走新连接，同时不接触真实应用进程中的池。
        snow_storage_services::database::close_pooled_connections();
    }
    Ok(())
}

#[test]
fn empty_database_bootstraps_v71_with_safe_defaults() -> rusqlite::Result<()> {
    let db = p0_support::TestDb::new();
    assert!(!db.path().exists());
    boot_v71(db.path())?;
    assert_eq!(
        feedback::count_feedback(db.path()).expect("读取空反馈表"),
        0
    );
    Ok(())
}

#[test]
fn v69_upgrade_creates_feedback_and_preserves_anchors_on_restart() -> rusqlite::Result<()> {
    let db = p0_support::TestDb::new();
    seed_legacy(db.path(), 69)?;
    boot_v71(db.path())?;
    assert_legacy_configs(db.path(), false);
    assert_memory_anchors(db.path())?;
    assert_eq!(
        feedback::count_feedback(db.path()).expect("v69 升级创建空反馈表"),
        0
    );

    // 这条路径只写 SQLite，不执行命令，也不会补建反馈表。
    feedback::insert_feedback(
        db.path(),
        &feedback::FeedbackInput {
            project_id: PROJECT.into(),
            command: "Get-Location".into(),
            rule_ids: vec!["p0-rule".into()],
            model_verdict: Some("allow".into()),
            model_reason: Some("fixture model reason".into()),
            user_decision: "rejected".into(),
            user_note: Some("fixture user note".into()),
        },
    )
    .expect("升级后生产服务可写反馈");
    let stamp = feedback_stamp(db.path())?;

    restart_v71(db.path())?;
    assert_legacy_configs(db.path(), false);
    assert_memory_anchors(db.path())?;
    assert_eq!(feedback_stamp(db.path())?, stamp);
    Ok(())
}

#[test]
fn v70_upgrade_keeps_existing_feedback_and_explicit_oauth_on_restart() -> rusqlite::Result<()> {
    let db = p0_support::TestDb::new();
    seed_legacy(db.path(), 70)?;
    boot_v71(db.path())?;
    assert_legacy_configs(db.path(), false);
    assert_memory_anchors(db.path())?;
    let original = (
        "feedback-before-upgrade".to_owned(),
        "2024-01-02 03:04:05".to_owned(),
    );
    assert_eq!(feedback_stamp(db.path())?, original);

    // 升级后显式改选；若每次启动都执行无条件回填，这一断言就会失败。
    assert_eq!(
        open_connection(db.path())?.execute(
            "UPDATE mcp_server_configs SET oauth = 1 WHERE server_id = 'old-global-http'",
            [],
        )?,
        1
    );
    restart_v71(db.path())?;
    assert_legacy_configs(db.path(), true);
    assert_memory_anchors(db.path())?;
    assert_eq!(feedback_stamp(db.path())?, original);
    Ok(())
}

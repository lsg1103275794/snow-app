//! 项目记忆的 SQLite 特征测试。只用显式传入库路径的服务，不初始化应用配置。

mod p0_support;

use std::path::Path;

use rusqlite::params;
use snow_storage_services::database;
use snow_storage_services::services::{chat_conversations as chat, project_memories as mem};
use snow_storage_services::MemoryRecord;

const PROJECT: &str = "p0-project-a";
const OTHER: &str = "p0-project-b";
const GLOBAL: &str = "";

fn fresh_db() -> p0_support::TestDb {
    let db = p0_support::TestDb::new();
    assert!(!db.path().try_exists().unwrap(), "只允许初始化全新的测试库");
    database::ensure_database(db.path()).expect("创建并迁移合成库");
    db
}

fn memory<'a>(
    directory_id: &'a str,
    title: &'a str,
    content: &'a str,
) -> mem::MemoryUpsertInput<'a> {
    mem::MemoryUpsertInput {
        directory_id,
        title,
        content,
        kind: "fact",
        source: "agent",
        status: "active",
        importance: 3,
        tags: vec![],
        conversation_id: "",
        response_id: "",
    }
}

fn save(path: &Path, input: mem::MemoryUpsertInput<'_>) -> (MemoryRecord, bool) {
    mem::upsert_memory(path, &input).expect("保存合成记忆")
}

fn visible(path: &Path, project: &str) -> Vec<MemoryRecord> {
    mem::list_memories(path, project, None, None, 20, 0)
        .unwrap()
        .items
}

fn only(mut rows: Vec<MemoryRecord>) -> MemoryRecord {
    assert_eq!(rows.len(), 1, "应恰好读到一条记忆");
    rows.pop().unwrap()
}

fn assert_members(rows: &[MemoryRecord], expected: &[&str]) {
    let mut actual: Vec<_> = rows.iter().map(|row| row.memory_id.as_str()).collect();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn same_title_merges_in_one_project_but_not_across_projects() {
    let db = fresh_db();
    let path = db.path();
    let title = "合成标尺";
    let (first, created) = save(
        path,
        mem::MemoryUpsertInput {
            importance: 4,
            tags: vec!["seed".into()],
            ..memory(PROJECT, title, "甲初版")
        },
    );
    assert!(created);
    let (other, created) = save(path, memory(OTHER, title, "乙原文"));
    assert!(created);
    assert_ne!(first.memory_id, other.memory_id);

    let (merged, created) = save(
        path,
        mem::MemoryUpsertInput {
            tags: vec!["revision".into()],
            ..memory(PROJECT, title, "甲修订")
        },
    );
    assert!(!created);
    assert_eq!(merged.memory_id, first.memory_id);
    let stored = only(visible(path, PROJECT));
    assert_eq!(stored.memory_id, first.memory_id);
    assert_eq!(stored.content, "甲修订");
    assert_eq!(stored.importance, 4);
    let mut tags = stored.tags;
    tags.sort_unstable();
    assert_eq!(tags, ["revision", "seed"]);
    assert_eq!(only(visible(path, OTHER)).content, "乙原文");
}

#[test]
fn keyword_search_matches_title_content_or_tag_in_project_and_global_only() {
    let db = fresh_db();
    let path = db.path();
    let (local, _) = save(path, memory(PROJECT, "quartz 标尺", "甲原文"));
    let (global, _) = save(path, memory(GLOBAL, "通用标尺", "正文含 quartz"));
    let (tagged, _) = save(
        path,
        mem::MemoryUpsertInput {
            tags: vec!["quartz".into()],
            ..memory(PROJECT, "颗粒编号", "只在标签命中")
        },
    );
    let (other, _) = save(path, memory(OTHER, "quartz 外部标尺", "quartz"));
    save(
        path,
        mem::MemoryUpsertInput {
            importance: 5,
            ..memory(PROJECT, "枝条", "无关内容")
        },
    );

    let search =
        |project, query| mem::search_memories(path, project, query, None, None, 20).unwrap();
    let hits = search(PROJECT, "quartz");
    assert_members(
        &hits,
        &[&local.memory_id, &global.memory_id, &tagged.memory_id],
    );
    // 三项重要性相同；整词标题命中强于仅正文或仅标签命中，不依赖同分顺序。
    assert_eq!(hits[0].memory_id, local.memory_id);
    assert_members(
        &search(OTHER, "quartz"),
        &[&other.memory_id, &global.memory_id],
    );
    assert!(search(PROJECT, "zircon").is_empty());
}

#[test]
fn moving_project_to_global_and_back_keeps_the_memory_id() {
    let db = fresh_db();
    let path = db.path();
    let (original, _) = save(path, memory(PROJECT, "迁域标尺", "固定原文"));
    let move_to = |directory: &str| {
        mem::update_memory(
            path,
            &original.memory_id,
            &mem::MemoryUpdatePatch {
                directory_id: Some(directory.to_owned()),
                kind: None,
                title: None,
                content: None,
                importance: None,
                status: None,
                tags: None,
            },
        )
        .unwrap()
    };
    assert!(visible(path, OTHER).is_empty());

    let global = move_to(GLOBAL);
    assert_eq!(global.scope, "global");
    assert_eq!(global.directory_id, GLOBAL);
    assert_eq!(global.memory_id, original.memory_id);
    assert_members(&visible(path, PROJECT), &[&original.memory_id]);
    assert_members(&visible(path, OTHER), &[&original.memory_id]);

    let local = move_to(OTHER);
    assert_eq!(local.scope, "project");
    assert_eq!(local.directory_id, OTHER);
    assert_eq!(local.memory_id, original.memory_id);
    assert!(visible(path, PROJECT).is_empty());
    assert_members(&visible(path, OTHER), &[&original.memory_id]);
}

fn conversation(path: &Path, title: &str) -> String {
    chat::create_chat_conversation(path, PROJECT, title, None).expect("创建合成会话")
}

#[test]
fn prompt_snapshot_first_write_wins_per_conversation() {
    let db = fresh_db();
    let path = db.path();
    // 用真实注入候选的正文作为合成快照文本；这里不调用 MCP 章节渲染或分发。
    let candidate_text =
        || only(mem::top_memories_for_injection(path, PROJECT, 3, 30).unwrap()).content;
    let old = conversation(path, "旧会话");
    save(path, memory(PROJECT, "冻结标尺", "第一版"));
    mem::write_prompt_snapshot(path, &old, PROJECT, &candidate_text()).unwrap();

    save(path, memory(PROJECT, "冻结标尺", "第二版"));
    let new = conversation(path, "新会话");
    assert_eq!(mem::read_prompt_snapshot(path, &new).unwrap(), None);
    let current = candidate_text();
    assert_eq!(current, "第二版");
    mem::write_prompt_snapshot(path, &old, PROJECT, &current).unwrap();
    mem::write_prompt_snapshot(path, &new, PROJECT, &current).unwrap();
    assert_eq!(
        mem::read_prompt_snapshot(path, &old).unwrap().as_deref(),
        Some("第一版")
    );
    assert_eq!(
        mem::read_prompt_snapshot(path, &new).unwrap().as_deref(),
        Some("第二版")
    );
}

fn sourced(path: &Path, conversation_id: &str, response_id: &str, title: &str) -> String {
    save(
        path,
        mem::MemoryUpsertInput {
            conversation_id,
            response_id,
            ..memory(PROJECT, title, "合成内容")
        },
    )
    .0
    .memory_id
}

// 只用 SQL 准备有序的消息行；回复边界、候选记忆和删除范围均由生产接口决定。
fn exchange(
    path: &Path,
    conversation_id: &str,
    request_row: &str,
    reply_row: &str,
    response_id: &str,
) {
    database::open_connection(path).unwrap().execute(
        "INSERT INTO chat_messages (id, message_id, conversation_id, role, content, response_id, status)
         VALUES (?1, 'msg-' || ?1, ?3, 'user', '合成请求', '', 'sent'),
                (?2, 'msg-' || ?2, ?3, 'assistant', '合成回答', ?4, 'completed')",
        params![request_row, reply_row, conversation_id, response_id],
    ).unwrap();
}

#[test]
fn rolling_back_the_last_response_removes_only_its_selected_memory() {
    let db = fresh_db();
    let path = db.path();
    let main = conversation(path, "目标会话");
    let other = conversation(path, "对照会话");
    exchange(path, &main, "010", "011", "r1");
    exchange(path, &main, "020", "021", "r2");
    exchange(path, &other, "030", "031", "r2"); // 不同会话故意复用 response_id。
    let earlier = sourced(path, &main, "r1", "前一轮");
    let target = sourced(path, &main, "r2", "目标末轮");
    let unanchored = sourced(path, &main, "", "无回复锚点");
    let foreign = sourced(path, &other, "r2", "另一会话的末轮");

    let preview = mem::list_memories_for_rollback(path, &main, None, Some("r2"), &[]).unwrap();
    assert_members(&preview, &[&target]);
    let selected: Vec<_> = preview.into_iter().map(|row| row.memory_id).collect();
    chat::truncate_conversation_from_response(path, &main, "r2").unwrap();
    // 截断与按预选 ID 清理是两个服务调用；没有硬编码待删 ID，也不模拟 UI 确认。
    assert_eq!(
        mem::delete_memories_by_memory_ids(path, &selected).unwrap(),
        1
    );
    assert_members(&visible(path, PROJECT), &[&earlier, &unanchored, &foreign]);
    let remaining_rows = |conversation: &str| {
        chat::list_chat_messages(path, conversation)
            .unwrap()
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(remaining_rows(&main), ["010", "011"]);
    assert_eq!(remaining_rows(&other), ["030", "031"]);
}

fn parent_and_node(path: &Path) -> (String, &'static str) {
    let parent = conversation(path, "父会话");
    let node = "p0-workflow-node";
    // 删除入口提交后会清理上传和 checkpoint；本夹具没有上传消息，flow checkpoint 留空。
    chat::create_workflow_node_session(
        path,
        node,
        &parent,
        "p0-flow",
        "",
        "node",
        "子节点",
        PROJECT,
        "",
        "",
    )
    .unwrap();
    (parent, node)
}

#[test]
fn deleting_a_parent_with_cleanup_off_keeps_parent_and_child_memories() {
    let db = fresh_db();
    let path = db.path();
    let (parent, node) = parent_and_node(path);
    let parent_memory = sourced(path, &parent, "", "父条目");
    let node_memory = sourced(path, node, "", "子条目");
    mem::write_prompt_snapshot(path, node, PROJECT, "子节点快照").unwrap();

    // N-API 将省略选项映射为 false；此处直接验证服务收到 false 时的行为。
    chat::delete_conversation(path, &parent, false).unwrap();
    assert!(!chat::conversation_exists(path, &parent).unwrap());
    assert!(!chat::conversation_exists(path, node).unwrap());
    assert_members(&visible(path, PROJECT), &[&parent_memory, &node_memory]);
    assert_eq!(mem::read_prompt_snapshot(path, node).unwrap(), None);
}

#[test]
fn opting_in_to_deletion_cleans_only_the_parent_and_child_memories() {
    let db = fresh_db();
    let path = db.path();
    let (parent, node) = parent_and_node(path);
    let other = conversation(path, "独立会话");
    let parent_memory = sourced(path, &parent, "", "父条目");
    let node_memory = sourced(path, node, "", "子条目");
    let kept = sourced(path, &other, "", "对照条目");
    assert_members(
        &visible(path, PROJECT),
        &[&parent_memory, &node_memory, &kept],
    );

    chat::delete_conversation(path, &parent, true).unwrap();
    assert!(!chat::conversation_exists(path, &parent).unwrap());
    assert!(!chat::conversation_exists(path, node).unwrap());
    assert!(chat::conversation_exists(path, &other).unwrap());
    assert_members(&visible(path, PROJECT), &[&kept]);
}

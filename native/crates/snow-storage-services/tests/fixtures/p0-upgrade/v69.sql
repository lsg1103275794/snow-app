-- v69 关键表切片（14b7ce6c^）；这里的六张表在 v70 中结构相同。
-- 所有行均为合成数据。没有 OAuth 列、没有反馈表，也不从当前建库函数导出。
PRAGMA foreign_keys = ON;

CREATE TABLE mcp_server_configs (
    id TEXT PRIMARY KEY NOT NULL,
    server_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL DEFAULT '',
    transport_type TEXT NOT NULL DEFAULT 'stdio',
    url TEXT NOT NULL DEFAULT '',
    command TEXT NOT NULL DEFAULT '',
    args_json TEXT NOT NULL DEFAULT '[]',
    env_json TEXT NOT NULL DEFAULT '{}',
    headers_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    timeout_ms INTEGER,
    sort_order INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX idx_mcp_server_configs_enabled ON mcp_server_configs(enabled);
CREATE INDEX idx_mcp_server_configs_source ON mcp_server_configs(source);

CREATE TABLE custom_header_schemes (
    id TEXT PRIMARY KEY NOT NULL,
    scheme_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL DEFAULT '',
    headers_json TEXT NOT NULL DEFAULT '{}',
    is_active INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX idx_custom_header_schemes_active ON custom_header_schemes(is_active);

CREATE TABLE system_settings (
    id TEXT PRIMARY KEY NOT NULL,
    setting_name TEXT NOT NULL,
    setting_code TEXT NOT NULL UNIQUE,
    setting_value TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE TABLE chat_conversations (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL DEFAULT '',
    summary TEXT NOT NULL DEFAULT '',
    last_message_preview TEXT NOT NULL DEFAULT '',
    message_count INTEGER NOT NULL DEFAULT 0,
    model TEXT NOT NULL DEFAULT '',
    api_profile_name TEXT NOT NULL DEFAULT '',
    thinking_strength TEXT,
    responses_fast_mode INTEGER,
    last_response_id TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'active',
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_input_tokens INTEGER NOT NULL DEFAULT 0,
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    run_input_tokens INTEGER NOT NULL DEFAULT 0,
    run_output_tokens INTEGER NOT NULL DEFAULT 0,
    run_cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0,
    run_cache_read_input_tokens INTEGER NOT NULL DEFAULT 0,
    last_run_duration_ms INTEGER NOT NULL DEFAULT 0,
    run_ttft_sum_ms INTEGER NOT NULL DEFAULT 0,
    run_request_count INTEGER NOT NULL DEFAULT 0,
    directory_id TEXT NOT NULL DEFAULT '',
    forked_from_conversation_id TEXT NOT NULL DEFAULT '',
    fork_message_count INTEGER NOT NULL DEFAULT 0,
    goal_mode INTEGER,
    worktree_mode INTEGER,
    workflow_mode INTEGER,
    goal_mode_token_budget INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX idx_chat_conversations_updated_at ON chat_conversations(updated_at DESC, id DESC);
CREATE INDEX idx_chat_conversations_status ON chat_conversations(status);

CREATE TABLE chat_messages (
    id TEXT PRIMARY KEY NOT NULL,
    message_id TEXT NOT NULL UNIQUE,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    origin TEXT NOT NULL DEFAULT 'unknown',
    content TEXT NOT NULL,
    model TEXT NOT NULL DEFAULT '',
    response_id TEXT NOT NULL DEFAULT '',
    checkpoint_id TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'sent',
    interruption_reason TEXT,
    recovery_outcome TEXT,
    raw_json TEXT NOT NULL DEFAULT '{}',
    thinking TEXT NOT NULL DEFAULT '',
    thinking_duration_ms INTEGER NOT NULL DEFAULT 0,
    thinking_token_count INTEGER NOT NULL DEFAULT 0,
    thinking_blocks_json TEXT NOT NULL DEFAULT '[]',
    tool_calls_json TEXT NOT NULL DEFAULT '[]',
    tool_corrections_json TEXT NOT NULL DEFAULT '{}',
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_input_tokens INTEGER NOT NULL DEFAULT 0,
    retry_attempts INTEGER NOT NULL DEFAULT 0,
    retry_error TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    FOREIGN KEY(conversation_id) REFERENCES chat_conversations(conversation_id) ON DELETE CASCADE
);
CREATE INDEX idx_chat_messages_conversation_id ON chat_messages(conversation_id, id ASC);
CREATE INDEX idx_chat_messages_response_id ON chat_messages(response_id);

CREATE TABLE project_memories (
    id TEXT PRIMARY KEY NOT NULL,
    memory_id TEXT NOT NULL UNIQUE,
    directory_id TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT 'fact',
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'agent',
    status TEXT NOT NULL DEFAULT 'active',
    importance INTEGER NOT NULL DEFAULT 2,
    conversation_id TEXT NOT NULL DEFAULT '',
    response_id TEXT NOT NULL DEFAULT '',
    tags_json TEXT NOT NULL DEFAULT '[]',
    last_recalled_at TEXT,
    recall_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX idx_project_memories_directory
    ON project_memories(directory_id, status, importance DESC, updated_at DESC, id DESC);

INSERT INTO mcp_server_configs
    (id, server_id, name, transport_type, url, headers_json, enabled, timeout_ms, updated_at)
VALUES ('old-mcp-row', 'old-global-http', 'P0 global', 'http', 'https://p0.invalid/global',
        '{"X-P0-MCP":"fixture-global"}', 0, 12500, '2024-01-02 03:04:05');
INSERT INTO custom_header_schemes (id, scheme_id, name, headers_json, is_active)
VALUES ('old-header-row', 'old-header-scheme', 'P0 scheme', '{"X-P0-Scheme":"fixture-scheme"}', 1);

INSERT INTO chat_conversations (id, conversation_id, directory_id, message_count, last_response_id)
VALUES ('chat-row', 'p0-chat', 'p0-project', 2, 'response-b');
INSERT INTO chat_messages (id, message_id, conversation_id, role, content, response_id)
VALUES ('msg-row-a', 'message-a', 'p0-chat', 'assistant', 'Fixture turn A', 'response-a'),
       ('msg-row-b', 'message-b', 'p0-chat', 'assistant', 'Fixture turn B', 'response-b');
-- 同名但作用域不同；全局记忆也保留写入它的会话与回合。
INSERT INTO project_memories (id, memory_id, directory_id, title, content, conversation_id, response_id)
VALUES ('mem-row-a', 'project-memory', 'p0-project', 'P0 anchor', 'project note', 'p0-chat', 'response-a'),
       ('mem-row-b', 'global-memory', '', 'P0 anchor', 'global note', 'p0-chat', 'response-b');

PRAGMA user_version = 69;

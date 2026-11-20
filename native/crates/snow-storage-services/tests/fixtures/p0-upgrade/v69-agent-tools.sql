-- v0.5.15 (69149068) 中 sub_agent_configs 的历史结构；不经新版本的保存/校验器造数据。
CREATE TABLE sub_agent_configs (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    system_prompt TEXT NOT NULL DEFAULT '',
    tools_json TEXT NOT NULL DEFAULT '[]',
    config_profile TEXT NOT NULL DEFAULT '',
    model TEXT NOT NULL DEFAULT '',
    builtin INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'manual',
    project_id TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(agent_id, project_id)
);
CREATE INDEX idx_sub_agent_configs_builtin ON sub_agent_configs(builtin);
CREATE INDEX idx_sub_agent_configs_source ON sub_agent_configs(source);

INSERT INTO sub_agent_configs (id, agent_id, name, tools_json, project_id)
VALUES ('old-agent-row', 'p0-inspector', 'P0 synthetic inspector',
        '["filesystem-read","config-get","terminal-list"]', 'p0-project');

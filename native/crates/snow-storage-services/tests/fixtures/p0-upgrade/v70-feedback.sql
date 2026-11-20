-- v70 相对 v69 的增量（14b7ce6c）。模拟已经记录过一次用户决定的旧库。
CREATE TABLE sensitive_command_feedback (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL DEFAULT '',
    command TEXT NOT NULL,
    rule_ids_json TEXT NOT NULL DEFAULT '[]',
    model_verdict TEXT,
    model_reason TEXT,
    user_decision TEXT NOT NULL,
    user_note TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX idx_sensitive_command_feedback_created
    ON sensitive_command_feedback(created_at DESC, id DESC);

INSERT INTO sensitive_command_feedback
    (id, project_id, command, rule_ids_json, model_verdict, model_reason, user_decision, user_note, created_at)
VALUES ('feedback-before-upgrade', 'p0-project', 'Get-Location', '["p0-rule"]', 'allow',
        'fixture model reason', 'rejected', 'fixture user note', '2024-01-02 03:04:05');

PRAGMA user_version = 70;

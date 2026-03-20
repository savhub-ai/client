-- Track AI API token usage and cost for auditing and budgeting.

CREATE TABLE ai_usage_logs (
    id              UUID PRIMARY KEY,
    task_type       TEXT NOT NULL,      -- 'flock_metadata', 'security_scan'
    provider        TEXT NOT NULL,      -- 'zhipu', 'doubao'
    model           TEXT NOT NULL,      -- 'glm-4-flash', 'glm-5', etc.
    prompt_tokens   INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens    INTEGER NOT NULL DEFAULT 0,
    target_type     TEXT,               -- 'flock', 'skill'
    target_id       UUID,
    created_at      TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_ai_usage_logs_task_type ON ai_usage_logs (task_type);
CREATE INDEX idx_ai_usage_logs_created_at ON ai_usage_logs (created_at);

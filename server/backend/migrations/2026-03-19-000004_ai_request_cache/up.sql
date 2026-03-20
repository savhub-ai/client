-- Track AI request results per commit so re-indexing the same commit
-- skips succeeded requests and retries failed ones.

CREATE TABLE ai_request_cache (
    id          UUID PRIMARY KEY,
    task_type   TEXT NOT NULL,       -- 'flock_metadata', 'repo_metadata', 'security_scan'
    target_type TEXT NOT NULL,       -- 'flock', 'repo', 'skill'
    target_id   UUID NOT NULL,
    commit_sha  TEXT NOT NULL,
    success     BOOLEAN NOT NULL DEFAULT FALSE,
    error_message TEXT,
    created_at  TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX idx_ai_request_cache_lookup
    ON ai_request_cache (task_type, target_id, commit_sha);

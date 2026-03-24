CREATE TABLE pending_index_repos (
    id UUID PRIMARY KEY,
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    expected_start_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (repo_id)
);

CREATE INDEX idx_pending_index_repos_start ON pending_index_repos (expected_start_at ASC);

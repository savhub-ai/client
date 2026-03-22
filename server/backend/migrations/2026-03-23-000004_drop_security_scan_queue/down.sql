CREATE TABLE security_scan_queue (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'pending',
    repo_id UUID NOT NULL REFERENCES repos(id),
    repo_url TEXT NOT NULL,
    path TEXT NOT NULL,
    flock_id UUID NOT NULL REFERENCES flocks(id),
    commit_hash TEXT NOT NULL,
    scan_files JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_security_scan_queue_repo_path_commit
    ON security_scan_queue (repo_url, path, commit_hash);

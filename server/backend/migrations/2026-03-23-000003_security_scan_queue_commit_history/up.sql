ALTER TABLE security_scan_queue
    DROP CONSTRAINT IF EXISTS security_scan_queue_repo_url_path_key;

ALTER TABLE security_scan_queue
    ADD CONSTRAINT security_scan_queue_repo_url_path_commit_hash_key
    UNIQUE (repo_url, path, commit_hash);

DROP INDEX IF EXISTS idx_security_scan_queue_pending;
CREATE INDEX idx_security_scan_queue_pending
    ON security_scan_queue (created_at ASC)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_security_scan_queue_flock_commit_updated
    ON security_scan_queue (flock_id, commit_hash, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_skills_ai_scan_claim
    ON skills (security_status, updated_at ASC)
    WHERE soft_deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_security_scans_target_module_created
    ON security_scans (target_id, scan_module, created_at DESC);

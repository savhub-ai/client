DROP INDEX IF EXISTS idx_security_scans_target_module_created;
DROP INDEX IF EXISTS idx_skills_ai_scan_claim;
DROP INDEX IF EXISTS idx_security_scan_queue_flock_commit_updated;

DROP INDEX IF EXISTS idx_security_scan_queue_pending;
CREATE INDEX idx_security_scan_queue_pending
    ON security_scan_queue (created_at ASC)
    WHERE status = 'pending';

ALTER TABLE security_scan_queue
    DROP CONSTRAINT IF EXISTS security_scan_queue_repo_url_path_commit_hash_key;

WITH ranked AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY repo_url, path
            ORDER BY updated_at DESC, created_at DESC, id DESC
        ) AS rn
    FROM security_scan_queue
)
DELETE FROM security_scan_queue
WHERE id IN (
    SELECT id
    FROM ranked
    WHERE rn > 1
);

ALTER TABLE security_scan_queue
    ADD CONSTRAINT security_scan_queue_repo_url_path_key
    UNIQUE (repo_url, path);

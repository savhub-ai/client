ALTER TABLE flocks
    DROP COLUMN IF EXISTS last_synced_at,
    DROP COLUMN IF EXISTS sync_status,
    DROP COLUMN IF EXISTS sync_error,
    DROP COLUMN IF EXISTS git_commit_sha;

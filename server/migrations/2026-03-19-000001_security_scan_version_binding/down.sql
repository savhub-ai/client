ALTER TABLE security_scans
    DROP COLUMN IF EXISTS version_id,
    DROP COLUMN IF EXISTS commit_sha;

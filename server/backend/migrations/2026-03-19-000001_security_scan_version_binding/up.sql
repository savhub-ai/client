-- Bind security scans to specific skill versions and git commits so the client
-- can verify it is downloading the exact code that was scanned.

ALTER TABLE security_scans
    ADD COLUMN version_id UUID REFERENCES skill_versions(id) ON DELETE SET NULL,
    ADD COLUMN commit_sha TEXT;

-- When a new version is imported, the flock / skill security_status should
-- reset to 'scanning' until scans complete. No data migration needed here
-- because the application code will handle this going forward.

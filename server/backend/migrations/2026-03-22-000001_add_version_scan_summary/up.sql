-- Add consolidated security scan summary to skill_versions.
-- Stores the per-version scan results from multiple engines (VirusTotal, LLM, static)
-- as a single JSONB blob so the detail API can serve it without extra joins.
ALTER TABLE skill_versions ADD COLUMN scan_summary JSONB;

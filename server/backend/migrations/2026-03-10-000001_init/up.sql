CREATE TABLE users (
    id UUID PRIMARY KEY,
    handle TEXT NOT NULL UNIQUE,
    display_name TEXT NULL,
    bio TEXT NULL,
    avatar_url TEXT NULL,
    github_user_id TEXT NULL,
    github_login TEXT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE user_tokens (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE repos (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    git_url TEXT NOT NULL UNIQUE,
    git_ref TEXT,
    git_sha TEXT NOT NULL,
    license TEXT,
    visibility TEXT NOT NULL DEFAULT 'public',
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    keywords TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    last_indexed_at TIMESTAMPTZ
);

CREATE TABLE flocks (
    id UUID PRIMARY KEY,
    slug TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT,
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    keywords TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    description TEXT NOT NULL,
    version TEXT,
    status TEXT NOT NULL,
    visibility TEXT NULL,
    license TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    source JSONB NOT NULL,
    imported_by_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    stats_comments BIGINT NOT NULL DEFAULT 0,
    stats_ratings BIGINT NOT NULL DEFAULT 0,
    stats_avg_rating DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    security_status TEXT NOT NULL DEFAULT 'unverified',
    stats_stars BIGINT NOT NULL DEFAULT 0,
    stats_max_installs BIGINT NOT NULL DEFAULT 0,
    stats_max_unique_users BIGINT NOT NULL DEFAULT 0,
    UNIQUE (repo_id, slug)
);

CREATE TABLE skills (
    id UUID PRIMARY KEY,
    slug TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    keywords TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    description TEXT NULL,
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    flock_id UUID NOT NULL REFERENCES flocks(id) ON DELETE CASCADE,
    version TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    license TEXT,
    source JSONB NOT NULL DEFAULT '{}'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    entry_data JSONB NULL,
    runtime_data JSONB NULL,
    scan_commit_hash TEXT NOT NULL,
    security_status TEXT NOT NULL DEFAULT 'unverified',
    latest_version_id UUID NULL,
    tags JSONB NOT NULL DEFAULT '{}'::jsonb,
    moderation_status TEXT NOT NULL DEFAULT 'active',
    highlighted BOOLEAN NOT NULL DEFAULT FALSE,
    official BOOLEAN NOT NULL DEFAULT FALSE,
    deprecated BOOLEAN NOT NULL DEFAULT FALSE,
    suspicious BOOLEAN NOT NULL DEFAULT FALSE,
    stats_downloads BIGINT NOT NULL DEFAULT 0,
    stats_stars BIGINT NOT NULL DEFAULT 0,
    stats_versions BIGINT NOT NULL DEFAULT 0,
    stats_comments BIGINT NOT NULL DEFAULT 0,
    stats_installs BIGINT NOT NULL DEFAULT 0,
    stats_unique_users BIGINT NOT NULL DEFAULT 0,
    soft_deleted_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE skill_versions (
    id UUID PRIMARY KEY,
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    flock_id UUID REFERENCES flocks(id) ON DELETE CASCADE,
    skill_id UUID REFERENCES skills(id) ON DELETE CASCADE,
    git_ref TEXT NOT NULL,
    git_sha TEXT NOT NULL,
    version TEXT,
    changelog TEXT NOT NULL,
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    files JSONB NOT NULL,
    parsed_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    search_document TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    scan_commit_hash TEXT NOT NULL,
    scan_summary JSONB,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL,
    soft_deleted_at TIMESTAMPTZ NULL,
    UNIQUE (skill_id, version)
);

CREATE TABLE skill_comments (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    flock_id UUID NOT NULL REFERENCES flocks(id) ON DELETE CASCADE,
    skill_id UUID REFERENCES skills(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    soft_deleted_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE skill_stars (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    flock_id UUID NOT NULL REFERENCES flocks(id) ON DELETE CASCADE,
    skill_id UUID REFERENCES skills(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (skill_id, user_id)
);

CREATE TABLE skill_blocks (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    flock_id UUID NOT NULL REFERENCES flocks(id) ON DELETE CASCADE,
    skill_id UUID REFERENCES skills(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, flock_id)
);

CREATE TABLE skill_ratings (
    id UUID PRIMARY KEY,
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    flock_id UUID NOT NULL REFERENCES flocks(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id),
    score SMALLINT NOT NULL CHECK (score >= 1 AND score <= 10),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(flock_id, user_id)
);

CREATE TABLE skill_installs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    skill_id UUID NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    flock_id UUID NOT NULL REFERENCES flocks(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    client_type TEXT NOT NULL DEFAULT 'unknown',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE audit_logs (
    id UUID PRIMARY KEY,
    actor_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id UUID NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE reports (
    id UUID PRIMARY KEY,
    reporter_user_id UUID NOT NULL REFERENCES users(id),
    target_type TEXT NOT NULL,
    target_id UUID NOT NULL,
    reason TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    reviewed_by_user_id UUID REFERENCES users(id),
    reviewed_at TIMESTAMPTZ,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE index_jobs (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'pending',
    job_type TEXT NOT NULL,
    git_url TEXT NOT NULL,
    git_ref TEXT NOT NULL DEFAULT 'main',
    git_sha TEXT NOT NULL,
    git_subdir TEXT NOT NULL DEFAULT '.',
    url_hash TEXT,
    repo_slug TEXT,
    requested_by_user_id UUID NOT NULL REFERENCES users(id),
    result_data JSONB NOT NULL DEFAULT '{}',
    error_message TEXT,
    progress_pct INT NOT NULL DEFAULT 0,
    progress_message TEXT NOT NULL DEFAULT '',
    force_index BOOLEAN NOT NULL DEFAULT FALSE,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE index_rules (
    id UUID PRIMARY KEY,
    repo_url TEXT NOT NULL UNIQUE,
    path_regex TEXT NOT NULL,
    strategy TEXT NOT NULL DEFAULT 'smart',
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE security_scans (
    id UUID PRIMARY KEY,
    target_type TEXT NOT NULL,
    target_id UUID NOT NULL,
    commit_hash TEXT NOT NULL,
    scan_module TEXT NOT NULL,
    result TEXT NOT NULL,
    severity TEXT,
    details JSONB NOT NULL DEFAULT '{}',
    scanned_by_user_id UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL,
    version_id UUID REFERENCES skill_versions(id) ON DELETE SET NULL
);

CREATE TABLE site_admins (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    granted_by_user_id UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (user_id)
);

CREATE TABLE browse_histories (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL,
    resource_id UUID NOT NULL,
    viewed_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE ai_usage_logs (
    id UUID PRIMARY KEY,
    task_type TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    target_type TEXT,
    target_id UUID,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE ai_request_cache (
    id UUID PRIMARY KEY,
    task_type TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id UUID NOT NULL,
    commit_hash TEXT NOT NULL,
    success BOOLEAN NOT NULL DEFAULT FALSE,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE security_scan_queue (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'pending',
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    repo_url TEXT NOT NULL,
    path TEXT NOT NULL,
    flock_id UUID NOT NULL REFERENCES flocks(id) ON DELETE CASCADE,
    commit_hash TEXT NOT NULL,
    scan_files JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (repo_url, path)
);

CREATE INDEX idx_security_scan_queue_pending ON security_scan_queue (created_at ASC) WHERE status = 'pending';

-- Indexes
CREATE INDEX idx_users_role ON users(role);
CREATE UNIQUE INDEX idx_users_github_user_id ON users(github_user_id) WHERE github_user_id IS NOT NULL;
CREATE UNIQUE INDEX idx_users_github_login ON users(github_login) WHERE github_login IS NOT NULL;

CREATE INDEX idx_repos_updated ON repos(updated_at DESC);

CREATE INDEX idx_flocks_repo ON flocks(repo_id, updated_at DESC);
CREATE INDEX idx_flocks_imported_by ON flocks(imported_by_user_id);

CREATE UNIQUE INDEX idx_skills_unique_flock_slug ON skills(flock_id, slug);
CREATE INDEX idx_skills_repo ON skills(repo_id);
CREATE INDEX idx_skills_flock ON skills(flock_id);
CREATE INDEX idx_skills_updated ON skills(updated_at DESC);
CREATE INDEX idx_skills_status ON skills(moderation_status, soft_deleted_at);

CREATE INDEX idx_skill_versions_skill ON skill_versions(skill_id, created_at DESC);
CREATE INDEX idx_skill_versions_fingerprint ON skill_versions(fingerprint);
CREATE INDEX idx_skill_comments_flock ON skill_comments(flock_id, created_at DESC);
CREATE INDEX idx_skill_ratings_flock ON skill_ratings(flock_id);
CREATE INDEX idx_skill_blocks_user ON skill_blocks(user_id);
CREATE INDEX idx_skill_blocks_flock ON skill_blocks(flock_id);
CREATE INDEX idx_skill_installs_skill_id ON skill_installs(skill_id);
CREATE INDEX idx_skill_installs_flock_id ON skill_installs(flock_id);
CREATE INDEX idx_skill_installs_user_id ON skill_installs(user_id);

CREATE INDEX idx_audit_logs_created ON audit_logs(created_at DESC);

CREATE INDEX idx_reports_target ON reports(target_type, target_id);
CREATE INDEX idx_reports_reporter ON reports(reporter_user_id);
CREATE INDEX idx_reports_status ON reports(status);

CREATE INDEX idx_index_jobs_dedup ON index_jobs (git_url, git_sha) WHERE status = 'completed' AND git_sha IS NOT NULL;
CREATE INDEX idx_index_jobs_url_hash_active ON index_jobs (url_hash) WHERE status IN ('pending', 'running');

CREATE INDEX idx_browse_histories_user ON browse_histories (user_id, viewed_at DESC);
CREATE INDEX idx_browse_histories_cleanup ON browse_histories (viewed_at);

CREATE INDEX idx_ai_usage_logs_task_type ON ai_usage_logs (task_type);
CREATE INDEX idx_ai_usage_logs_created_at ON ai_usage_logs (created_at);

CREATE UNIQUE INDEX idx_ai_request_cache_lookup ON ai_request_cache (task_type, target_id, commit_hash);

-- Seed index rules
INSERT INTO index_rules (id, repo_url, path_regex, strategy, description, created_at, updated_at) VALUES
    ('01968c00-0000-7000-8000-000000000001', 'https://github.com/anthropics/skills.git', '^skills$', 'each_dir_as_flock', 'Anthropic official skills', NOW(), NOW()),
    ('01968c00-0000-7000-8000-000000000002', 'https://github.com/sickn33/antigravity-awesome-skills.git', '^skills$', 'each_dir_as_flock', 'Antigravity awesome skills', NOW(), NOW()),
    ('01968c00-0000-7000-8000-000000000003', 'https://github.com/mofa-org/mofa-skills.git', '.*', 'each_dir_as_flock', 'MoFA skills', NOW(), NOW()),
    ('01968c00-0000-7000-8000-000000000004', 'https://github.com/K-Dense-AI/claude-scientific-skills.git', '^scientific-skills$', 'each_dir_as_flock', 'K-Dense scientific skills', NOW(), NOW()),
    ('01968c00-0000-7000-8000-000000000005', 'https://github.com/openclaw/skills.git', '.*', 'each_dir_as_flock', 'OpenClaw skills', NOW(), NOW());

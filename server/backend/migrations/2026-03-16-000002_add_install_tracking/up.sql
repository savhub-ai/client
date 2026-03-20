-- Track individual skill installs for "Most Installed" and "Most People Used"
CREATE TABLE skill_installs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    skill_id UUID NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    flock_id UUID NOT NULL REFERENCES flocks(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    client_type TEXT NOT NULL DEFAULT 'unknown',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_skill_installs_skill_id ON skill_installs(skill_id);
CREATE INDEX idx_skill_installs_flock_id ON skill_installs(flock_id);
CREATE INDEX idx_skill_installs_user_id ON skill_installs(user_id);

-- Cached stats on skills
ALTER TABLE skills ADD COLUMN stats_installs BIGINT NOT NULL DEFAULT 0;
ALTER TABLE skills ADD COLUMN stats_unique_users BIGINT NOT NULL DEFAULT 0;

-- Cached max stats on flocks (max of contained skills)
ALTER TABLE flocks ADD COLUMN stats_max_installs BIGINT NOT NULL DEFAULT 0;
ALTER TABLE flocks ADD COLUMN stats_max_unique_users BIGINT NOT NULL DEFAULT 0;

-- Revert to the original FK constraints without ON DELETE CASCADE.

ALTER TABLE skill_versions
    DROP CONSTRAINT IF EXISTS skill_versions_repo_id_fkey,
    ADD CONSTRAINT skill_versions_repo_id_fkey
        FOREIGN KEY (repo_id) REFERENCES repos(id);

ALTER TABLE skill_versions
    DROP CONSTRAINT IF EXISTS skill_versions_flock_id_fkey,
    ADD CONSTRAINT skill_versions_flock_id_fkey
        FOREIGN KEY (flock_id) REFERENCES flocks(id);

ALTER TABLE skill_comments
    DROP CONSTRAINT IF EXISTS skill_comments_repo_id_fkey,
    ADD CONSTRAINT skill_comments_repo_id_fkey
        FOREIGN KEY (repo_id) REFERENCES repos(id);

ALTER TABLE skill_comments
    DROP CONSTRAINT IF EXISTS skill_comments_flock_id_fkey,
    ADD CONSTRAINT skill_comments_flock_id_fkey
        FOREIGN KEY (flock_id) REFERENCES flocks(id);

ALTER TABLE skill_stars
    DROP CONSTRAINT IF EXISTS skill_stars_repo_id_fkey,
    ADD CONSTRAINT skill_stars_repo_id_fkey
        FOREIGN KEY (repo_id) REFERENCES repos(id);

ALTER TABLE skill_stars
    DROP CONSTRAINT IF EXISTS skill_stars_flock_id_fkey,
    ADD CONSTRAINT skill_stars_flock_id_fkey
        FOREIGN KEY (flock_id) REFERENCES flocks(id);

ALTER TABLE skill_blocks
    DROP CONSTRAINT IF EXISTS skill_blocks_repo_id_fkey,
    ADD CONSTRAINT skill_blocks_repo_id_fkey
        FOREIGN KEY (repo_id) REFERENCES repos(id);

ALTER TABLE skill_blocks
    DROP CONSTRAINT IF EXISTS skill_blocks_flock_id_fkey,
    ADD CONSTRAINT skill_blocks_flock_id_fkey
        FOREIGN KEY (flock_id) REFERENCES flocks(id);

ALTER TABLE skill_ratings
    DROP CONSTRAINT IF EXISTS skill_ratings_repo_id_fkey,
    ADD CONSTRAINT skill_ratings_repo_id_fkey
        FOREIGN KEY (repo_id) REFERENCES repos(id);

ALTER TABLE skill_ratings
    DROP CONSTRAINT IF EXISTS skill_ratings_flock_id_fkey,
    ADD CONSTRAINT skill_ratings_flock_id_fkey
        FOREIGN KEY (flock_id) REFERENCES flocks(id);

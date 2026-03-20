ALTER TABLE flocks DROP COLUMN stats_max_unique_users;
ALTER TABLE flocks DROP COLUMN stats_max_installs;
ALTER TABLE skills DROP COLUMN stats_unique_users;
ALTER TABLE skills DROP COLUMN stats_installs;
DROP TABLE IF EXISTS skill_installs;

-- Rename path_pattern to path_regex and convert existing values to regex syntax
ALTER TABLE index_rules RENAME COLUMN path_pattern TO path_regex;

-- Convert existing glob-style values to regex:
--   '*' -> '.*'  (match everything)
--   'skills' -> '^skills$'  (exact match)
UPDATE index_rules SET path_regex = '.*' WHERE path_regex = '*';
UPDATE index_rules SET path_regex = '^' || path_regex || '$'
    WHERE path_regex NOT LIKE '.*' AND path_regex NOT LIKE '^%';

-- Migrate strategy name: subdirs_as_flocks -> each_dir_as_flock
UPDATE index_rules SET strategy = 'each_dir_as_flock' WHERE strategy = 'subdirs_as_flocks';

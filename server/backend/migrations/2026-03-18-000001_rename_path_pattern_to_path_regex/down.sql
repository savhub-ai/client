-- Revert strategy name
UPDATE index_rules SET strategy = 'subdirs_as_flocks' WHERE strategy = 'each_dir_as_flock';

-- Revert regex values back to glob-style:
--   '.*' -> '*'
--   '^skills$' -> 'skills'
UPDATE index_rules SET path_regex = '*' WHERE path_regex = '.*';
UPDATE index_rules SET path_regex = regexp_replace(regexp_replace(path_regex, '^\^', ''), '\$$', '')
    WHERE path_regex LIKE '^%' AND path_regex LIKE '%$';

ALTER TABLE index_rules RENAME COLUMN path_regex TO path_pattern;

DROP INDEX IF EXISTS user_tokens_token_hash_key;

ALTER TABLE user_tokens
    DROP COLUMN IF EXISTS token_prefix,
    DROP COLUMN IF EXISTS token_hash;

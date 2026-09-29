-- The intake reading the conversation's final question was asked on (ADR-066).
ALTER TABLE conversations ADD COLUMN final_check_reading jsonb;

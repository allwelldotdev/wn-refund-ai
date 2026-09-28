-- Admins message the customer on an escalated request. A message carries its
-- author, and each side's read marker drives the unread counts.
ALTER TABLE messages ADD COLUMN author_admin_id uuid REFERENCES admins (id);
ALTER TABLE messages ADD CONSTRAINT messages_author_role_check
  CHECK (author_admin_id IS NULL OR role = 'admin');

ALTER TABLE conversations ADD COLUMN customer_read_seq int NOT NULL DEFAULT 0;
ALTER TABLE conversations ADD COLUMN admin_read_seq int NOT NULL DEFAULT 0;
-- Conversations that already exist start fully read.
UPDATE conversations SET customer_read_seq = last_seq, admin_read_seq = last_seq;

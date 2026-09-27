-- Admin settings that sit outside the versioned policy: one row.
CREATE TABLE app_settings (
  id                  boolean PRIMARY KEY DEFAULT true CHECK (id),
  allow_disputes      boolean NOT NULL DEFAULT true,
  updated_by_admin_id uuid REFERENCES admins (id),
  updated_at          timestamptz
);
INSERT INTO app_settings DEFAULT VALUES;

-- Set once, when the customer disputes an automatic denial; the request is
-- then escalated and its original decision audit stays as it was.
ALTER TABLE refund_requests ADD COLUMN disputed_at timestamptz;

-- Messages from an admin and system notes (e.g. "You disputed this decision").
ALTER TABLE messages DROP CONSTRAINT messages_role_check;
ALTER TABLE messages ADD CONSTRAINT messages_role_check
  CHECK (role IN ('customer', 'assistant', 'admin', 'system'));
ALTER TABLE messages DROP CONSTRAINT messages_check;
ALTER TABLE messages ADD CONSTRAINT messages_assistant_kind_role_check
  CHECK ((role = 'assistant') = (assistant_kind IS NOT NULL));

ALTER TABLE request_events DROP CONSTRAINT request_events_kind_check;
ALTER TABLE request_events ADD CONSTRAINT request_events_kind_check
  CHECK (kind IN ('decided', 'review_drafted', 'review_failed', 'resolved', 'disputed'));
ALTER TABLE request_events DROP CONSTRAINT request_events_actor_kind_check;
ALTER TABLE request_events ADD CONSTRAINT request_events_actor_kind_check
  CHECK (actor_kind IN ('system', 'admin', 'customer'));

-- Replies that file no request: an item that already has one, a customer who
-- needs nothing else, and an off-topic message steered back to refunds.
ALTER TABLE messages DROP CONSTRAINT messages_assistant_kind_check;
ALTER TABLE messages ADD CONSTRAINT messages_assistant_kind_check
  CHECK (assistant_kind IN ('clarify', 'verdict', 'holding', 'existing_request', 'closing', 'redirect'));

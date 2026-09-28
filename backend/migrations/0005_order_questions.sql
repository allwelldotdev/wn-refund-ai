-- Replies that file no request: a greeting, what is on record for an order
-- the customer asked about, and the order list to pick from.
ALTER TABLE messages DROP CONSTRAINT messages_assistant_kind_check;
ALTER TABLE messages ADD CONSTRAINT messages_assistant_kind_check
  CHECK (assistant_kind IN ('clarify', 'verdict', 'holding', 'existing_request', 'closing', 'redirect', 'greeting', 'order_status', 'order_list'));

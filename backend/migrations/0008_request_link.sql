-- The reply that points a customer asking again about a request to its thread.
ALTER TABLE messages DROP CONSTRAINT messages_assistant_kind_check;
ALTER TABLE messages ADD CONSTRAINT messages_assistant_kind_check
  CHECK (assistant_kind IN ('clarify', 'verdict', 'holding', 'existing_request', 'closing', 'redirect', 'greeting', 'order_status', 'order_list', 'final_check', 'request_link'));

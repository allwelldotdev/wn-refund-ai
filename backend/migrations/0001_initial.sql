-- Initial schema. Money is integer cents; timestamps are timestamptz (UTC).
-- Enumerations are CHECK constraints rather than PG enum types.

CREATE TABLE customers (
  id            uuid PRIMARY KEY,
  name          text NOT NULL,
  email         text NOT NULL UNIQUE,
  password_hash text NOT NULL,
  scenario      text NOT NULL,
  created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE admins (
  id            uuid PRIMARY KEY,
  name          text NOT NULL,
  email         text NOT NULL UNIQUE,
  password_hash text NOT NULL,
  created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
  id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  principal_kind text NOT NULL CHECK (principal_kind IN ('customer', 'admin')),
  customer_id    uuid REFERENCES customers (id) ON DELETE CASCADE,
  admin_id       uuid REFERENCES admins (id) ON DELETE CASCADE,
  created_at     timestamptz NOT NULL DEFAULT now(),
  expires_at     timestamptz NOT NULL,
  CHECK ((principal_kind = 'customer' AND customer_id IS NOT NULL AND admin_id IS NULL)
      OR (principal_kind = 'admin' AND admin_id IS NOT NULL AND customer_id IS NULL))
);

CREATE TABLE orders (
  id           uuid PRIMARY KEY,
  ref          text NOT NULL UNIQUE,
  customer_id  uuid NOT NULL REFERENCES customers (id),
  placed_at    timestamptz NOT NULL,
  delivered_at timestamptz,
  status       text NOT NULL CHECK (status IN ('processing', 'shipped', 'delivered')),
  total_cents  bigint NOT NULL CHECK (total_cents >= 0),
  created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX orders_customer_idx ON orders (customer_id, placed_at DESC);

CREATE TABLE order_items (
  id           uuid PRIMARY KEY,
  order_id     uuid NOT NULL REFERENCES orders (id) ON DELETE CASCADE,
  name         text NOT NULL,
  category     text NOT NULL,
  quantity     int NOT NULL DEFAULT 1 CHECK (quantity > 0),
  amount_cents bigint NOT NULL CHECK (amount_cents >= 0),
  final_sale   boolean NOT NULL DEFAULT false
);
CREATE INDEX order_items_order_idx ON order_items (order_id);

-- Append-only (ADR-008/016): rows are never updated or deleted; a revert inserts
-- a copy. content_hash is deliberately not unique, so a revert can repeat a hash.
CREATE TABLE policy_versions (
  id                       uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  version                  int NOT NULL UNIQUE,
  rules                    jsonb NOT NULL,
  content_hash             text NOT NULL,
  author_kind              text NOT NULL CHECK (author_kind IN ('system', 'admin')),
  author_admin_id          uuid REFERENCES admins (id),
  change_note              text,
  reverted_from_version_id uuid REFERENCES policy_versions (id),
  created_at               timestamptz NOT NULL DEFAULT now(),
  CHECK ((author_kind = 'admin') = (author_admin_id IS NOT NULL))
);

CREATE TABLE conversations (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  customer_id uuid NOT NULL REFERENCES customers (id),
  last_seq    int NOT NULL DEFAULT 0,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX conversations_customer_idx ON conversations (customer_id, created_at DESC);

-- seq is allocated by bumping conversations.last_seq in the same transaction.
-- Bodies are stored and rendered as plain text only (ADR-023).
CREATE TABLE messages (
  id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  conversation_id uuid NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
  seq             int NOT NULL,
  role            text NOT NULL CHECK (role IN ('customer', 'assistant')),
  assistant_kind  text CHECK (assistant_kind IN ('clarify', 'verdict', 'holding')),
  body            text NOT NULL CHECK (char_length(body) BETWEEN 1 AND 4000),
  client_msg_id   uuid UNIQUE,
  order_id        uuid REFERENCES orders (id),
  created_at      timestamptz NOT NULL DEFAULT now(),
  UNIQUE (conversation_id, seq),
  CHECK ((role = 'customer') = (assistant_kind IS NULL)),
  CHECK ((role = 'customer') = (client_msg_id IS NOT NULL))
);

-- Pre-scan matches as character offsets into messages.body (ADR-023).
CREATE TABLE message_signals (
  id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  message_id uuid NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
  scope      text NOT NULL CHECK (scope IN ('message', 'window')),
  detector   text NOT NULL,
  start_char int NOT NULL CHECK (start_char >= 0),
  end_char   int NOT NULL CHECK (end_char > start_char),
  score      real NOT NULL CHECK (score >= 0 AND score <= 1),
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (message_id, scope, detector, start_char, end_char)
);

CREATE SEQUENCE refund_ref_seq START 1001;

-- One refund request per conversation: the verdict freezes (ADR-021).
CREATE TABLE refund_requests (
  id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  ref             text NOT NULL UNIQUE DEFAULT ('RR-' || nextval('refund_ref_seq')),
  conversation_id uuid NOT NULL UNIQUE REFERENCES conversations (id),
  customer_id     uuid NOT NULL REFERENCES customers (id),
  order_id        uuid REFERENCES orders (id),
  order_item_id   uuid REFERENCES order_items (id),
  amount_cents    bigint CHECK (amount_cents >= 0),
  reason_category text CHECK (reason_category IN
                    ('damaged', 'wrong_item', 'not_received', 'changed_mind', 'not_as_described', 'other')),
  state           text NOT NULL CHECK (state IN
                    ('approved', 'denied', 'escalated', 'resolved_approved', 'resolved_denied')),
  created_at      timestamptz NOT NULL DEFAULT now(),
  resolved_at     timestamptz,
  CHECK ((order_id IS NULL) = (order_item_id IS NULL))
);
-- At most one refund that pays out per order item (ADR-013).
CREATE UNIQUE INDEX refund_requests_one_active_per_item
  ON refund_requests (order_item_id) WHERE state IN ('approved', 'resolved_approved');
CREATE INDEX refund_requests_created_idx ON refund_requests (created_at DESC);
CREATE INDEX refund_requests_state_created_idx ON refund_requests (state, created_at DESC);
CREATE INDEX refund_requests_customer_created_idx ON refund_requests (customer_id, created_at DESC);

-- Drives the admin status timeline.
CREATE TABLE request_events (
  id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  refund_request_id uuid NOT NULL REFERENCES refund_requests (id) ON DELETE CASCADE,
  kind              text NOT NULL CHECK (kind IN ('decided', 'review_drafted', 'review_failed', 'resolved')),
  actor_kind        text NOT NULL CHECK (actor_kind IN ('system', 'admin')),
  actor_admin_id    uuid REFERENCES admins (id),
  payload           jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at        timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX request_events_request_idx ON request_events (refund_request_id, created_at);

-- Everything needed to explain a verdict. Messages with seq <= evaluated_through_seq
-- are "Used in decision"; later ones are "After decision" (ADR-022).
CREATE TABLE decision_audit (
  id                    uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  refund_request_id     uuid NOT NULL UNIQUE REFERENCES refund_requests (id) ON DELETE CASCADE,
  policy_version_id     uuid NOT NULL REFERENCES policy_versions (id),
  content_hash          text NOT NULL,
  evaluated_through_seq int NOT NULL,
  verdict               text NOT NULL CHECK (verdict IN ('approved', 'denied', 'escalated')),
  prescan_signals       jsonb NOT NULL,
  extracted             jsonb,
  facts                 jsonb NOT NULL,
  rule_trace            jsonb NOT NULL,
  flags                 jsonb NOT NULL,
  stages                jsonb NOT NULL,
  created_at            timestamptz NOT NULL DEFAULT now()
);

-- Review-model draft for escalations plus the admin's final call.
CREATE TABLE escalation_reviews (
  id                   uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  refund_request_id    uuid NOT NULL UNIQUE REFERENCES refund_requests (id) ON DELETE CASCADE,
  status               text NOT NULL CHECK (status IN ('pending', 'drafted', 'failed')),
  model                text,
  effort               text,
  draft                jsonb,
  error                text,
  latency_ms           int,
  prompt_tokens        int,
  completion_tokens    int,
  drafted_at           timestamptz,
  resolution           text CHECK (resolution IN ('approved', 'denied')),
  resolution_note      text,
  resolved_by_admin_id uuid REFERENCES admins (id),
  resolved_at          timestamptz,
  created_at           timestamptz NOT NULL DEFAULT now()
);

-- How far along an order is, and simulated orders customers add from My
-- orders to try the chat (demo only). Seeded orders get `fulfilment` from the
-- seed; added ones always set it.
ALTER TABLE orders
  ADD COLUMN fulfilment text CHECK (fulfilment IN ('delivered', 'used', 'confirmed', 'active')),
  ADD COLUMN starts_at  timestamptz,
  ADD COLUMN ends_at    timestamptz,
  ADD COLUMN is_test    boolean NOT NULL DEFAULT false;

-- The seed supplies stable ids; added orders take generated ones.
ALTER TABLE orders ALTER COLUMN id SET DEFAULT gen_random_uuid();
ALTER TABLE order_items ALTER COLUMN id SET DEFAULT gen_random_uuid();

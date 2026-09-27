//! Customer orders: seeded, plus test orders a customer adds (demo only).
//! `active_refund` marks items that already have a refund that pays out,
//! which the engine refuses to repeat (ADR-013, ADR-030).

use chrono::{DateTime, Utc};
use domain::types::{Fulfilment, OrderStatus};
use serde::Serialize;
use uuid::Uuid;

use crate::{Db, DbError, parse_enum};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OrderItem {
    pub id: Uuid,
    pub name: String,
    pub category: String,
    pub quantity: i32,
    pub amount_cents: i64,
    pub final_sale: bool,
    pub active_refund: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Order {
    pub id: Uuid,
    #[serde(rename = "ref")]
    pub order_ref: String,
    pub placed_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub status: OrderStatus,
    pub total_cents: i64,
    pub fulfilment: Option<Fulfilment>,
    /// A confirmed booking's start, or when an active plan began.
    pub starts_at: Option<DateTime<Utc>>,
    /// When an active plan runs out, if it has an end.
    pub ends_at: Option<DateTime<Utc>>,
    pub is_test: bool,
    pub items: Vec<OrderItem>,
}

impl Order {
    pub fn item(&self, id: Uuid) -> Option<&OrderItem> {
        self.items.iter().find(|i| i.id == id)
    }
}

/// Test orders first, newest added first (so one just added is on the first
/// page), then the rest newest first; items by name.
pub async fn list_orders_for_customer(db: &Db, customer_id: Uuid) -> Result<Vec<Order>, DbError> {
    let rows = sqlx::query!(
        r#"SELECT o.id AS order_id, o.ref, o.placed_at, o.delivered_at, o.status, o.total_cents,
                  o.fulfilment, o.starts_at, o.ends_at, o.is_test,
                  i.id AS item_id, i.name, i.category, i.quantity, i.amount_cents, i.final_sale,
                  EXISTS (SELECT 1 FROM refund_requests r
                          WHERE r.order_item_id = i.id
                            AND r.state IN ('approved', 'resolved_approved')) AS "active_refund!"
           FROM orders o
           JOIN order_items i ON i.order_id = o.id
           WHERE o.customer_id = $1
           ORDER BY o.is_test DESC, CASE WHEN o.is_test THEN o.created_at END DESC,
                    o.placed_at DESC, o.id, i.name, i.id"#,
        customer_id,
    )
    .fetch_all(&db.0)
    .await?;

    let mut orders: Vec<Order> = Vec::new();
    for r in rows {
        if orders.last().is_none_or(|o| o.id != r.order_id) {
            orders.push(Order {
                id: r.order_id,
                order_ref: r.r#ref,
                placed_at: r.placed_at,
                delivered_at: r.delivered_at,
                status: parse_enum(&r.status)?,
                total_cents: r.total_cents,
                fulfilment: r.fulfilment.as_deref().map(parse_enum).transpose()?,
                starts_at: r.starts_at,
                ends_at: r.ends_at,
                is_test: r.is_test,
                items: Vec::new(),
            });
        }
        let order = orders.last_mut().expect("pushed above");
        order.items.push(OrderItem {
            id: r.item_id,
            name: r.name,
            category: r.category,
            quantity: r.quantity,
            amount_cents: r.amount_cents,
            final_sale: r.final_sale,
            active_refund: r.active_refund,
        });
    }
    Ok(orders)
}

/// The next number in the `ORD-` sequence; not reserved, so only a preview.
pub async fn next_order_ref(db: &Db) -> Result<String, DbError> {
    let n = sqlx::query_scalar!(
        r#"SELECT COALESCE(max(substring(ref FROM 5)::int), 10000) + 1 AS "n!"
           FROM orders WHERE ref ~ '^ORD-[0-9]+$'"#
    )
    .fetch_one(&db.0)
    .await?;
    Ok(format!("ORD-{n}"))
}

#[derive(Debug)]
pub struct NewOrderItem {
    pub name: String,
    pub category: String,
    pub quantity: i32,
    pub amount_cents: i64,
    pub final_sale: bool,
}

#[derive(Debug)]
pub struct NewOrder {
    pub placed_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub status: OrderStatus,
    pub fulfilment: Fulfilment,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub items: Vec<NewOrderItem>,
}

/// A test order for `customer_id` with the next `ORD-` number, which a
/// transaction-scoped advisory lock hands out one at a time. Returns its id.
pub async fn create_test_order(db: &Db, customer_id: Uuid, o: &NewOrder) -> Result<Uuid, DbError> {
    let mut tx = db.0.begin().await?;
    sqlx::query!("SELECT pg_advisory_xact_lock(hashtext('order_ref'))")
        .execute(&mut *tx)
        .await?;
    let total: i64 = o.items.iter().map(|i| i.amount_cents).sum();
    let order_id = sqlx::query_scalar!(
        r#"INSERT INTO orders (ref, customer_id, placed_at, delivered_at, status, total_cents,
                               fulfilment, starts_at, ends_at, is_test)
           SELECT 'ORD-' || (COALESCE(max(substring(ref FROM 5)::int), 10000) + 1),
                  $1, $2, $3, $4, $5, $6, $7, $8, true
           FROM orders WHERE ref ~ '^ORD-[0-9]+$'
           RETURNING id"#,
        customer_id,
        o.placed_at,
        o.delivered_at,
        o.status.as_str(),
        total,
        o.fulfilment.as_str(),
        o.starts_at,
        o.ends_at,
    )
    .fetch_one(&mut *tx)
    .await?;
    for item in &o.items {
        sqlx::query!(
            "INSERT INTO order_items (order_id, name, category, quantity, amount_cents, final_sale)
             VALUES ($1, $2, $3, $4, $5, $6)",
            order_id,
            item.name,
            item.category,
            item.quantity,
            item.amount_cents,
            item.final_sale,
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(order_id)
}

pub async fn is_order_owned(db: &Db, order_id: Uuid, customer_id: Uuid) -> Result<bool, DbError> {
    let owned = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM orders WHERE id = $1 AND customer_id = $2) AS "owned!""#,
        order_id,
        customer_id,
    )
    .fetch_one(&db.0)
    .await?;
    Ok(owned)
}

/// Owners of any of `refs` (exact match), for the cross-customer check.
pub async fn owners_by_ref(db: &Db, refs: &[String]) -> Result<Vec<(String, Uuid)>, DbError> {
    let rows = sqlx::query!(
        "SELECT ref, customer_id FROM orders WHERE ref = ANY($1)",
        refs,
    )
    .fetch_all(&db.0)
    .await?;
    Ok(rows.into_iter().map(|r| (r.r#ref, r.customer_id)).collect())
}

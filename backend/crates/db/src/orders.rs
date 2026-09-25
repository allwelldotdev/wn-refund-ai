//! Customer orders (read-only; seeded). `active_refund` marks items that
//! already have a refund that pays out, which the engine refuses to repeat
//! (ADR-013, ADR-030).

use chrono::{DateTime, Utc};
use domain::types::OrderStatus;
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
    pub items: Vec<OrderItem>,
}

impl Order {
    pub fn item(&self, id: Uuid) -> Option<&OrderItem> {
        self.items.iter().find(|i| i.id == id)
    }
}

/// Newest order first; items by name.
pub async fn list_orders_for_customer(db: &Db, customer_id: Uuid) -> Result<Vec<Order>, DbError> {
    let rows = sqlx::query!(
        r#"SELECT o.id AS order_id, o.ref, o.placed_at, o.delivered_at, o.status, o.total_cents,
                  i.id AS item_id, i.name, i.category, i.quantity, i.amount_cents, i.final_sale,
                  EXISTS (SELECT 1 FROM refund_requests r
                          WHERE r.order_item_id = i.id
                            AND r.state IN ('approved', 'resolved_approved')) AS "active_refund!"
           FROM orders o
           JOIN order_items i ON i.order_id = o.id
           WHERE o.customer_id = $1
           ORDER BY o.placed_at DESC, o.id, i.name, i.id"#,
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

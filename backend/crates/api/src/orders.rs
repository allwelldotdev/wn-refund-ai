//! Test orders a customer adds from My orders to try the chat (demo only):
//! the catalog, creating an order, and a dry run of the real policy engine on
//! it. Names, prices and categories come from the server's catalog.

use std::collections::HashMap;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use db::catalog::{CATALOG, CatalogItem, Group};
use db::orders::{NewOrder, NewOrderItem, Order};
use domain::engine::{Claims, Facts, ItemFacts, OrderFacts, decide};
use domain::policy::FieldError;
use domain::types::{Fulfilment, OrderStatus, ReasonCategory, Verdict};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::auth::CustomerSession;
use crate::error::{ApiError, ApiJson};

/// How far back a test order may be dated.
const MAX_AGE_DAYS: i64 = 60;
/// How far ahead a confirmed booking may start.
const MAX_START_DAYS: i64 = 120;
const MAX_HOURS: i32 = 12;
const MAX_UNITS: i32 = 20;
const RUN_OPTIONS: [i64; 4] = [7, 30, 90, 365];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/catalog", get(catalog))
        .route("/api/orders", post(create))
        .route("/api/orders/preview", post(preview))
}

#[derive(Serialize)]
struct CatalogGroup {
    key: Group,
    label: &'static str,
    items: Vec<&'static CatalogItem>,
}

#[derive(Serialize)]
struct CatalogView {
    /// A preview only: the number is assigned when the order is added.
    next_order_ref: String,
    groups: Vec<CatalogGroup>,
}

async fn catalog(
    State(state): State<AppState>,
    _: CustomerSession,
) -> Result<Json<CatalogView>, ApiError> {
    let groups = Group::ALL
        .iter()
        .map(|&key| CatalogGroup {
            key,
            label: key.label(),
            items: CATALOG.iter().filter(|i| i.group == key).collect(),
        })
        .collect();
    Ok(Json(CatalogView {
        next_order_ref: db::orders::next_order_ref(&state.db).await?,
        groups,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LineBody {
    item: String,
    quantity: i32,
}

/// Dates are calendar days as the customer picked them.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderBody {
    placed_on: NaiveDate,
    items: Vec<LineBody>,
    fulfilment: Option<Fulfilment>,
    delivered_on: Option<NaiveDate>,
    starts_on: Option<NaiveDate>,
    runs_for_days: Option<i64>,
}

fn noon(day: NaiveDate) -> DateTime<Utc> {
    day.and_hms_opt(12, 0, 0).expect("noon exists").and_utc()
}

fn line_name(item: &CatalogItem, quantity: i32) -> String {
    match (item.hourly, quantity) {
        (true, 1) => format!("{} (1 hr)", item.name),
        (true, q) => format!("{} ({q} hrs)", item.name),
        (false, 1) => item.name.to_owned(),
        (false, q) => format!("{} × {q}", item.name),
    }
}

/// Every problem at once, so the form can list them together. `today` is
/// the server's day; one day of slack covers customers ahead of UTC.
fn validate(b: &OrderBody, today: NaiveDate) -> Result<NewOrder, ApiError> {
    let mut errors = Vec::new();
    let mut fail = |path: &str, message: &str| {
        errors.push(FieldError {
            path: path.to_owned(),
            message: message.to_owned(),
        })
    };
    let latest = today + Duration::days(1);
    if b.placed_on > latest || b.placed_on < today - Duration::days(MAX_AGE_DAYS) {
        fail("placed_on", "must be within the last 60 days");
    }

    let mut items = Vec::new();
    if b.items.is_empty() {
        fail("items", "Add at least one item.");
    }
    for (i, line) in b.items.iter().enumerate() {
        let Some(item) = db::catalog::find(&line.item) else {
            fail(&format!("items[{i}].item"), "is not in the catalog");
            continue;
        };
        let max = if item.hourly { MAX_HOURS } else { MAX_UNITS };
        if !(1..=max).contains(&line.quantity) {
            fail(
                &format!("items[{i}].quantity"),
                &format!("must be between 1 and {max}"),
            );
            continue;
        }
        items.push(NewOrderItem {
            name: line_name(item, line.quantity),
            category: item.category.to_owned(),
            quantity: line.quantity,
            amount_cents: item.unit_cents * i64::from(line.quantity),
            final_sale: item.final_sale,
        });
    }

    let placed_at = noon(b.placed_on);
    let (mut delivered_at, mut status) = (Some(placed_at), OrderStatus::Delivered);
    let (mut starts_at, mut ends_at) = (None, None);
    match b.fulfilment {
        None => fail("fulfilment", "Choose a delivery status."),
        Some(Fulfilment::Used) => {}
        Some(Fulfilment::Delivered) => match b.delivered_on {
            None => fail("delivered_on", "Choose the delivery date."),
            Some(d) if d < b.placed_on => {
                fail("delivered_on", "Delivery can't be before the order date.")
            }
            Some(d) if d > latest => fail("delivered_on", "Delivery can't be in the future."),
            Some(d) => delivered_at = Some(noon(d)),
        },
        Some(Fulfilment::Confirmed) => match b.starts_on {
            None => fail("starts_on", "Choose the start date."),
            Some(d) if d < b.placed_on => {
                fail("starts_on", "The start can't be before the order date.")
            }
            Some(d) if d > today + Duration::days(MAX_START_DAYS) => {
                fail("starts_on", "The start must be within 120 days.")
            }
            Some(d) => {
                (delivered_at, status) = (None, OrderStatus::Processing);
                starts_at = Some(noon(d));
            }
        },
        Some(Fulfilment::Active) => match b.runs_for_days {
            Some(days) if RUN_OPTIONS.contains(&days) => {
                let end = b.placed_on + Duration::days(days);
                if end < today {
                    fail(
                        "runs_for_days",
                        "This period already ended. Pick a longer period or a later order date.",
                    );
                }
                (starts_at, ends_at) = (Some(placed_at), Some(noon(end)));
            }
            _ => fail("runs_for_days", "Choose how long it runs."),
        },
    }

    if !errors.is_empty() {
        return Err(ApiError::Unprocessable(errors));
    }
    Ok(NewOrder {
        placed_at,
        delivered_at,
        status,
        fulfilment: b.fulfilment.expect("checked above"),
        starts_at,
        ends_at,
        items,
    })
}

async fn create(
    State(state): State<AppState>,
    s: CustomerSession,
    ApiJson(body): ApiJson<OrderBody>,
) -> Result<Json<Order>, ApiError> {
    let order = validate(&body, Utc::now().date_naive())?;
    let id = db::orders::create_test_order(&state.db, s.customer_id, &order).await?;
    let created = db::orders::list_orders_for_customer(&state.db, s.customer_id)
        .await?
        .into_iter()
        .find(|o| o.id == id)
        .ok_or(ApiError::NotFound)?;
    tracing::info!(order = %created.order_ref, "test order added");
    Ok(Json(created))
}

#[derive(Serialize)]
struct PreviewItem {
    name: String,
    verdict: Verdict,
    /// The policy sentence behind the verdict, as a customer would read it.
    reason: String,
}

#[derive(Serialize)]
struct Preview {
    assumption: &'static str,
    items: Vec<PreviewItem>,
}

/// The real engine and current policy on the order as it would be stored,
/// for each item, assuming the customer asks today because it was damaged.
/// Nothing is saved.
async fn preview(
    State(state): State<AppState>,
    s: CustomerSession,
    ApiJson(body): ApiJson<OrderBody>,
) -> Result<Json<Preview>, ApiError> {
    let order = validate(&body, Utc::now().date_naive())?;
    let policy = db::policy::latest_policy(&state.db).await?;
    let prior = db::refunds::prior_claims(&state.db, s.customer_id, Uuid::nil()).await?;
    let mut seen = HashMap::new();
    let items = order
        .items
        .iter()
        .filter(|i| seen.insert(i.name.clone(), ()).is_none())
        .map(|item| {
            let facts = Facts {
                now: Utc::now(),
                order: Some(OrderFacts {
                    order_id: Uuid::nil(),
                    order_ref: "new".into(),
                    placed_at: order.placed_at,
                    delivered_at: order.delivered_at,
                    status: order.status,
                    item: ItemFacts {
                        order_item_id: Uuid::nil(),
                        name: item.name.clone(),
                        category: item.category.clone(),
                        amount_cents: item.amount_cents,
                        final_sale: item.final_sale,
                        has_active_refund: false,
                    },
                }),
                claims: Claims {
                    reason: Some(ReasonCategory::Damaged),
                    claimed_amount_cents: None,
                    contradictory_statements: false,
                },
                prior_claims: prior.clone(),
                flags: vec![],
            };
            let decision = decide(&policy.rules, &facts);
            PreviewItem {
                name: item.name.clone(),
                verdict: decision.verdict,
                reason: decision
                    .customer_reasons()
                    .into_iter()
                    .next()
                    .unwrap_or_default(),
            }
        })
        .collect();
    Ok(Json(Preview {
        assumption: "If you asked today because an item arrived damaged or wasn't as booked",
        items,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn body(fulfilment: Option<Fulfilment>) -> OrderBody {
        OrderBody {
            placed_on: day("2026-09-19"),
            items: vec![
                LineBody {
                    item: "meeting-room".into(),
                    quantity: 3,
                },
                LineBody {
                    item: "coffee-add-on".into(),
                    quantity: 1,
                },
            ],
            fulfilment,
            delivered_on: None,
            starts_on: None,
            runs_for_days: None,
        }
    }

    fn paths(e: ApiError) -> Vec<String> {
        match e {
            ApiError::Unprocessable(fields) => fields.into_iter().map(|f| f.path).collect(),
            other => panic!("expected field errors, got {other:?}"),
        }
    }

    #[test]
    fn a_used_booking_is_dated_the_order_day_and_priced_by_the_server() {
        let o = validate(&body(Some(Fulfilment::Used)), day("2026-09-24")).unwrap();
        assert_eq!(o.delivered_at, Some(noon(day("2026-09-19"))));
        assert_eq!(o.status, OrderStatus::Delivered);
        let names: Vec<&str> = o.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Meeting Room (3 hrs)", "Coffee add-on"]);
        assert_eq!(o.items[0].amount_cents, 7200);
    }

    #[test]
    fn a_confirmed_booking_has_not_been_delivered() {
        let mut b = body(Some(Fulfilment::Confirmed));
        b.starts_on = Some(day("2026-10-01"));
        let o = validate(&b, day("2026-09-24")).unwrap();
        assert_eq!((o.delivered_at, o.status), (None, OrderStatus::Processing));
        assert_eq!(o.starts_at, Some(noon(day("2026-10-01"))));
    }

    #[test]
    fn every_problem_is_reported_together() {
        let mut b = body(Some(Fulfilment::Active));
        b.placed_on = day("2026-07-01");
        b.items.clear();
        b.runs_for_days = Some(7);
        assert_eq!(
            paths(validate(&b, day("2026-09-24")).unwrap_err()),
            ["placed_on", "items", "runs_for_days"]
        );
        let mut b = body(Some(Fulfilment::Delivered));
        b.delivered_on = Some(day("2026-09-18"));
        b.items[0].quantity = 13;
        assert_eq!(
            paths(validate(&b, day("2026-09-24")).unwrap_err()),
            ["items[0].quantity", "delivered_on"]
        );
        assert_eq!(
            paths(validate(&body(None), day("2026-09-24")).unwrap_err()),
            ["fulfilment"]
        );
    }
}

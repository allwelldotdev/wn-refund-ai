//! Intake stage contract (ADR-002). The intake model reads the customer's
//! messages and returns structured claims plus manipulation signals. Its output
//! is untrusted: Rust checks every id against the customer's own orders before
//! anything reaches the engine.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{OrderStatus, ReasonCategory};

/// Below this, intake output raises `Flag::LowConfidence`.
pub const LOW_CONFIDENCE: f32 = 0.6;
/// Clarifying questions allowed per conversation before it escalates.
pub const MAX_CLARIFY_TURNS: u8 = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemSummary {
    pub id: Uuid,
    pub name: String,
    pub category: String,
    pub amount_cents: i64,
    pub final_sale: bool,
}

/// One of the customer's own orders, from the database (trusted).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderSummary {
    pub id: Uuid,
    pub order_ref: String,
    pub placed_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub status: OrderStatus,
    pub items: Vec<ItemSummary>,
}

/// A customer message (untrusted text).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomerMessage {
    pub id: Uuid,
    pub seq: i32,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntakeInput {
    pub orders: Vec<OrderSummary>,
    /// Order picked in the chat UI, if any.
    pub selected_order_id: Option<Uuid>,
    pub messages: Vec<CustomerMessage>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntakeStatus {
    Complete,
    NeedsInfo,
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MissingField {
    Order,
    Item,
    Reason,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InjectionSignal {
    pub message_id: Uuid,
    /// Short label, e.g. "instruction", "impersonation", "policy_claim", "encoded".
    pub kind: String,
    pub excerpt: String,
}

/// What the intake model must return, as a strict JSON schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntakeOutput {
    pub status: IntakeStatus,
    pub missing: Vec<MissingField>,
    pub order_id: Option<Uuid>,
    pub order_item_id: Option<Uuid>,
    /// Every order number the customer typed, including ones that are not theirs.
    pub mentioned_order_refs: Vec<String>,
    pub reason_category: Option<ReasonCategory>,
    pub claimed_amount_cents: Option<i64>,
    pub contradictory_statements: bool,
    pub injection_signals: Vec<InjectionSignal>,
    /// 0.0 to 1.0.
    pub confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_schema_is_closed_and_uses_wire_names() {
        let schema = serde_json::to_value(schemars::schema_for!(IntakeOutput)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        let text = schema.to_string();
        for name in [
            "needs_info",
            "wrong_item",
            "not_as_described",
            "mentioned_order_refs",
        ] {
            assert!(text.contains(name), "schema lacks {name}");
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let ok = serde_json::json!({
            "status": "complete", "missing": [], "order_id": null, "order_item_id": null,
            "mentioned_order_refs": [], "reason_category": "damaged", "claimed_amount_cents": null,
            "contradictory_statements": false, "injection_signals": [], "confidence": 0.9
        });
        assert!(serde_json::from_value::<IntakeOutput>(ok.clone()).is_ok());
        let mut extra = ok;
        extra["verdict"] = "approved".into();
        assert!(serde_json::from_value::<IntakeOutput>(extra).is_err());
    }
}

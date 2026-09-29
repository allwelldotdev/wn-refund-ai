//! Escalation review contract. After an escalation, a stronger model drafts a
//! case summary and a suggested resolution for the admin. The admin decides.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::engine::{FiredRule, OrderFacts};
use crate::intake::{CustomerMessage, IntakeOutput};
use crate::types::Flag;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReviewInput {
    pub request_ref: String,
    /// When the engine decided; the reference point for time-based rules.
    pub decided_at: DateTime<Utc>,
    /// Set when the customer disputed an automatic denial; their reason, if
    /// any, is among the messages after the decision.
    pub disputed_at: Option<DateTime<Utc>>,
    pub order: Option<OrderFacts>,
    pub extracted: Option<IntakeOutput>,
    pub fired: Vec<FiredRule>,
    pub flags: Vec<Flag>,
    pub prior_claim_count: usize,
    pub policy_prose: String,
    pub messages: Vec<CustomerMessage>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedResolution {
    Approve,
    Deny,
}

/// What the review model must return, as a strict JSON schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewOutput {
    pub summary: String,
    pub suggested_resolution: SuggestedResolution,
    pub rationale: String,
    pub risk_notes: Vec<String>,
    pub questions_for_customer: Vec<String>,
}

/// How the prompt tells the model to start a note about a security flag.
pub const MISUSE_PREFIX: &str = "Possible misuse:";

/// The flags that mean someone may be trying to misuse the refund flow, with
/// the note Rust adds when the model leaves them out (under 15 words each).
const MISUSE_NOTES: [(Flag, &str); 3] = [
    (
        Flag::ForeignOrderReference,
        "Possible misuse: the customer named an order that belongs to another customer.",
    ),
    (
        Flag::IntakeInjectionSignal,
        "Possible misuse: the messages try to instruct the assistant or claim authority.",
    ),
    (
        Flag::PrescanSignal,
        "Possible misuse: the messages contain text shaped like instructions to the system.",
    ),
];

/// Backstop for the review draft, which the admin reads before deciding. The
/// prompt asks for a leading "Possible misuse:" note on any security flag;
/// when the model writes none, Rust adds its own, so such a case is never
/// described only as a records mismatch. Someone else's order with none of
/// the customer's own leaves nothing of theirs to refund, so the suggestion
/// is deny. Idempotent.
pub fn tidy_review(mut out: ReviewOutput, input: &ReviewInput) -> ReviewOutput {
    let prefix = MISUSE_PREFIX.to_lowercase();
    let noted = out
        .risk_notes
        .iter()
        .any(|n| n.trim_start().to_lowercase().starts_with(&prefix));
    if !noted {
        let notes = MISUSE_NOTES
            .iter()
            .filter(|(flag, _)| input.flags.contains(flag))
            .map(|(_, note)| (*note).to_owned());
        out.risk_notes.splice(0..0, notes);
        out.risk_notes.truncate(3);
    }
    if input.order.is_none() && input.flags.contains(&Flag::ForeignOrderReference) {
        out.suggested_resolution = SuggestedResolution::Deny;
    }
    out
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn input(flags: Vec<Flag>, order: Option<OrderFacts>) -> ReviewInput {
        ReviewInput {
            request_ref: "RR-1001".into(),
            decided_at: Utc.with_ymd_and_hms(2026, 9, 25, 12, 0, 0).unwrap(),
            disputed_at: None,
            order,
            extracted: None,
            fired: vec![],
            flags,
            prior_claim_count: 0,
            policy_prose: String::new(),
            messages: vec![],
        }
    }

    fn lenient() -> ReviewOutput {
        ReviewOutput {
            summary: "The customer wants a deposit refunded.".into(),
            suggested_resolution: SuggestedResolution::Approve,
            rationale: "Ask for the correct order first.".into(),
            risk_notes: vec!["The order is not in the case records.".into()],
            questions_for_customer: vec!["Can you confirm the order number?".into()],
        }
    }

    fn order() -> OrderFacts {
        serde_json::from_value(serde_json::json!({
            "order_id": "00000000-0000-0000-0000-000000000001",
            "order_ref": "ORD-10351",
            "placed_at": "2026-09-01T00:00:00Z",
            "delivered_at": "2026-09-02T00:00:00Z",
            "status": "delivered",
            "item": {
                "order_item_id": "00000000-0000-0000-0000-000000000002",
                "name": "Dedicated Desk, monthly",
                "category": "Memberships",
                "amount_cents": 32900,
                "final_sale": false,
                "has_active_refund": false
            }
        }))
        .unwrap()
    }

    #[test]
    fn someone_elses_order_is_named_as_possible_misuse_and_denied() {
        let tidied = tidy_review(lenient(), &input(vec![Flag::ForeignOrderReference], None));
        assert_eq!(tidied.risk_notes[0], MISUSE_NOTES[0].1);
        assert_eq!(
            tidied.risk_notes[1],
            "The order is not in the case records."
        );
        assert_eq!(tidied.suggested_resolution, SuggestedResolution::Deny);
        assert_eq!(
            tidied.summary,
            lenient().summary,
            "only notes and the suggestion change"
        );
        let again = tidy_review(
            tidied.clone(),
            &input(vec![Flag::ForeignOrderReference], None),
        );
        assert_eq!(again, tidied, "idempotent");
    }

    #[test]
    fn a_models_own_misuse_note_is_kept() {
        let mut out = lenient();
        out.risk_notes = vec!["possible misuse: claims another customer's deposit.".into()];
        let tidied = tidy_review(out.clone(), &input(vec![Flag::ForeignOrderReference], None));
        assert_eq!(tidied.risk_notes, out.risk_notes);
    }

    #[test]
    fn every_security_flag_gets_a_note_within_three() {
        let mut out = lenient();
        out.risk_notes = vec!["one".into(), "two".into(), "three".into()];
        let flags = vec![Flag::PrescanSignal, Flag::IntakeInjectionSignal];
        let tidied = tidy_review(out, &input(flags, Some(order())));
        assert_eq!(
            tidied.risk_notes,
            [MISUSE_NOTES[1].1, MISUSE_NOTES[2].1, "one"]
        );
        assert_eq!(
            tidied.suggested_resolution,
            SuggestedResolution::Approve,
            "only someone else's order with none of their own forces deny"
        );
        for (_, note) in MISUSE_NOTES {
            assert!(note.split_whitespace().count() < 15, "{note}");
        }
    }

    #[test]
    fn other_flags_leave_the_draft_alone() {
        let flags = vec![Flag::LowConfidence, Flag::NoRuleFired];
        assert_eq!(tidy_review(lenient(), &input(flags, None)), lenient());
    }

    #[test]
    fn output_round_trips_and_schema_is_closed() {
        let out = ReviewOutput {
            summary: "Customer reports a damaged TV above the review threshold.".into(),
            suggested_resolution: SuggestedResolution::Approve,
            rationale: "Delivered 7 days ago; damage claim is consistent.".into(),
            risk_notes: vec![],
            questions_for_customer: vec!["Can you share a photo?".into()],
        };
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json["suggested_resolution"], "approve");
        assert_eq!(serde_json::from_value::<ReviewOutput>(json).unwrap(), out);

        let schema = serde_json::to_value(schemars::schema_for!(ReviewOutput)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
    }
}

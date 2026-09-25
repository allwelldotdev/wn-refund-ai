//! Escalation review contract. After an escalation, a stronger model drafts a
//! case summary and a suggested resolution for the admin. The admin decides.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::engine::{FiredRule, OrderFacts};
use crate::intake::{CustomerMessage, IntakeOutput};
use crate::types::Flag;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReviewInput {
    pub request_ref: String,
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

#[cfg(test)]
mod tests {
    use super::*;

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

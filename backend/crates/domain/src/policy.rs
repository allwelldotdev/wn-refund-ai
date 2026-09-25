//! Refund policy as typed, versionable rules (ADR-016).
//!
//! Admins configure the settings of each rule; engineers define which rule kinds
//! exist. A policy that fails `parse` or `validate` can never be stored.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rule {
    FinalSaleNotRefundable {
        enabled: bool,
    },
    RefundWindow {
        enabled: bool,
        days: i64,
        scope: WindowScope,
    },
    HumanReviewAbove {
        enabled: bool,
        amount_cents: i64,
    },
    DamagedOrIncorrectEligible {
        enabled: bool,
    },
    RepeatClaimLimit {
        enabled: bool,
        max_claims: i64,
        lookback_days: i64,
    },
    ConflictingClaimEscalates {
        enabled: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WindowScope {
    All,
    Category { category: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub rules: Vec<Rule>,
}

/// A validation problem located by a JSON path such as `rules[1].days`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub path: String,
    pub message: String,
}

impl FieldError {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

// Numeric settings are i64 so out-of-range values (including negatives) reach
// `validate` and get a field-level error, instead of failing inside serde's
// buffered tagged-enum content where only the rule index is known.
pub const WINDOW_DAYS_RANGE: std::ops::RangeInclusive<i64> = 1..=365;
pub const MAX_CLAIMS_RANGE: std::ops::RangeInclusive<i64> = 1..=100;
pub const LOOKBACK_DAYS_RANGE: std::ops::RangeInclusive<i64> = 1..=3650;

impl Rule {
    pub fn kind(&self) -> &'static str {
        match self {
            Rule::FinalSaleNotRefundable { .. } => "final_sale_not_refundable",
            Rule::RefundWindow { .. } => "refund_window",
            Rule::HumanReviewAbove { .. } => "human_review_above",
            Rule::DamagedOrIncorrectEligible { .. } => "damaged_or_incorrect_eligible",
            Rule::RepeatClaimLimit { .. } => "repeat_claim_limit",
            Rule::ConflictingClaimEscalates { .. } => "conflicting_claim_escalates",
        }
    }

    pub fn enabled(&self) -> bool {
        match self {
            Rule::FinalSaleNotRefundable { enabled }
            | Rule::RefundWindow { enabled, .. }
            | Rule::HumanReviewAbove { enabled, .. }
            | Rule::DamagedOrIncorrectEligible { enabled }
            | Rule::RepeatClaimLimit { enabled, .. }
            | Rule::ConflictingClaimEscalates { enabled } => *enabled,
        }
    }
}

impl Policy {
    /// Deserializes and range-checks a policy. Errors carry JSON paths so the
    /// admin form can show them next to the offending field.
    pub fn parse(json: &str) -> Result<Policy, Vec<FieldError>> {
        let de = &mut serde_json::Deserializer::from_str(json);
        let policy: Policy = serde_path_to_error::deserialize(de).map_err(|e| {
            let path = e.path().to_string();
            let path = if path == "." { String::new() } else { path };
            vec![FieldError::new(path, e.inner().to_string())]
        })?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<(), Vec<FieldError>> {
        let mut errors = Vec::new();
        if self.rules.is_empty() {
            errors.push(FieldError::new("rules", "at least one rule is required"));
        }
        for (i, rule) in self.rules.iter().enumerate() {
            let at = |field: &str| format!("rules[{i}].{field}");
            match rule {
                Rule::RefundWindow { days, scope, .. } => {
                    if !WINDOW_DAYS_RANGE.contains(days) {
                        errors.push(FieldError::new(at("days"), "must be between 1 and 365"));
                    }
                    if let WindowScope::Category { category } = scope
                        && category.trim().is_empty()
                    {
                        errors.push(FieldError::new(
                            at("scope.category"),
                            "category must not be empty",
                        ));
                    }
                }
                Rule::HumanReviewAbove { amount_cents, .. } => {
                    if *amount_cents <= 0 {
                        errors.push(FieldError::new(
                            at("amount_cents"),
                            "must be greater than 0",
                        ));
                    }
                }
                Rule::RepeatClaimLimit {
                    max_claims,
                    lookback_days,
                    ..
                } => {
                    if !MAX_CLAIMS_RANGE.contains(max_claims) {
                        errors.push(FieldError::new(
                            at("max_claims"),
                            "must be between 1 and 100",
                        ));
                    }
                    if !LOOKBACK_DAYS_RANGE.contains(lookback_days) {
                        errors.push(FieldError::new(
                            at("lookback_days"),
                            "must be between 1 and 3650",
                        ));
                    }
                }
                Rule::FinalSaleNotRefundable { .. }
                | Rule::DamagedOrIncorrectEligible { .. }
                | Rule::ConflictingClaimEscalates { .. } => {}
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Serialization of the typed value: field order is fixed by the type, so two
    /// JSON documents that differ only in key order or whitespace hash the same.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("policy serialization cannot fail")
    }

    /// Hex SHA-256 of `canonical_json`, recorded on every decision audit row.
    pub fn content_hash(&self) -> String {
        Sha256::digest(self.canonical_json().as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = include_str!("../../../../policy/default-policy.json");

    #[test]
    fn default_policy_parses_with_all_six_rules() {
        let p = Policy::parse(DEFAULT).expect("default policy must be valid");
        let kinds: Vec<_> = p.rules.iter().map(Rule::kind).collect();
        assert_eq!(
            kinds,
            [
                "final_sale_not_refundable",
                "refund_window",
                "human_review_above",
                "damaged_or_incorrect_eligible",
                "repeat_claim_limit",
                "conflicting_claim_escalates",
            ]
        );
        assert!(p.rules.iter().all(Rule::enabled));
    }

    #[test]
    fn round_trip_is_lossless() {
        let p = Policy::parse(DEFAULT).unwrap();
        let again = Policy::parse(&p.canonical_json()).unwrap();
        assert_eq!(p, again);
    }

    #[test]
    fn hash_ignores_key_order_and_whitespace() {
        let a = r#"{"rules":[{"kind":"refund_window","enabled":true,"days":30,"scope":{"kind":"all"}}]}"#;
        let b = r#"{ "rules": [ { "scope": {"kind": "all"}, "days": 30, "enabled": true, "kind": "refund_window" } ] }"#;
        let (a, b) = (Policy::parse(a).unwrap(), Policy::parse(b).unwrap());
        assert_eq!(a.content_hash(), b.content_hash());
        assert_eq!(a.content_hash().len(), 64);
    }

    #[test]
    fn hash_changes_when_a_setting_changes() {
        let a = Policy::parse(DEFAULT).unwrap();
        let mut b = a.clone();
        if let Rule::RefundWindow { days, .. } = &mut b.rules[1] {
            *days = 15;
        }
        assert_ne!(a.content_hash(), b.content_hash());
    }

    fn errors(json: &str) -> Vec<FieldError> {
        Policy::parse(json).expect_err("expected invalid policy")
    }

    #[test]
    fn out_of_range_values_report_field_paths() {
        let e = errors(
            r#"{"rules":[
                {"kind":"final_sale_not_refundable","enabled":true},
                {"kind":"refund_window","enabled":true,"days":0,"scope":{"kind":"category","category":" "}},
                {"kind":"human_review_above","enabled":true,"amount_cents":0},
                {"kind":"repeat_claim_limit","enabled":true,"max_claims":0,"lookback_days":4000}
            ]}"#,
        );
        let paths: Vec<_> = e.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "rules[1].days",
                "rules[1].scope.category",
                "rules[2].amount_cents",
                "rules[3].max_claims",
                "rules[3].lookback_days",
            ]
        );
    }

    #[test]
    fn window_bounds_are_inclusive() {
        let ok = |d: i64| {
            Policy::parse(&format!(
                r#"{{"rules":[{{"kind":"refund_window","enabled":true,"days":{d},"scope":{{"kind":"all"}}}}]}}"#
            ))
            .is_ok()
        };
        assert!(ok(1) && ok(365));
        assert!(!ok(0) && !ok(366));
    }

    #[test]
    fn unknown_kind_and_unknown_field_are_rejected_with_path() {
        let e = errors(r#"{"rules":[{"kind":"approve_everything","enabled":true}]}"#);
        assert_eq!(e[0].path, "rules[0].kind");
        let e = errors(r#"{"rules":[{"kind":"final_sale_not_refundable","enabled":true,"x":1}]}"#);
        assert_eq!(e[0].path, "rules[0]");
    }

    #[test]
    fn negative_days_get_a_field_level_error() {
        let e = errors(
            r#"{"rules":[{"kind":"refund_window","enabled":true,"days":-5,"scope":{"kind":"all"}}]}"#,
        );
        assert_eq!(e[0].path, "rules[0].days");
    }

    #[test]
    fn empty_rule_list_is_invalid() {
        assert_eq!(errors(r#"{"rules":[]}"#)[0].path, "rules");
    }
}

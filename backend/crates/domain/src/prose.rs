//! Customer-facing policy text, generated from the typed rules (ADR-018) so the
//! prose and the settings the engine enforces can never disagree. The same
//! sentences are the customer-safe reasons attached to fired rules.

use crate::money::format_cents;
use crate::policy::{Policy, Rule, WindowScope};

const TITLE: &str = "# Refund Policy";
const INTRO: &str = "Every request is checked against all of the rules below. When more than one rule applies, the most restrictive outcome wins: a denial outranks a review, and a review outranks an approval.";
const NO_ACTIVE_RULES: &str = "There are currently no active refund rules.";

/// Closing line of the policy; also the customer reason when no rule fired.
pub const NOT_COVERED_REASON: &str =
    "Any request not covered by an applicable rule is reviewed by a support agent.";
/// Customer reason for the fail-closed check. Says nothing about what was detected.
pub const CLOSER_LOOK_REASON: &str = "This request needs a closer look from our team.";
/// Customer reason when the item already has an approved refund.
pub const ALREADY_REFUNDED_REASON: &str =
    "This item already has a refund on record, so a support agent will review this request.";

/// One rule as it appears in the policy: a bold title and a one- or two-sentence body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleText {
    pub title: String,
    pub body: String,
}

pub fn rule_text(rule: &Rule) -> RuleText {
    let (title, body) = match rule {
        Rule::FinalSaleNotRefundable { .. } => (
            "Final sale items are not refundable.".to_owned(),
            "Items marked as final sale at purchase cannot be refunded.".to_owned(),
        ),
        Rule::RefundWindow { days, scope, .. } => {
            let (title, subject) = match scope {
                WindowScope::All => ("Refund window.".to_owned(), "Refunds".to_owned()),
                WindowScope::Category { category } => (
                    format!("Refund window ({category})."),
                    format!("Refunds for {category} items"),
                ),
            };
            (
                title,
                format!(
                    "{subject} must be requested within {} of delivery (or of the order date if no delivery is recorded). Later requests are denied.",
                    count(*days, "day")
                ),
            )
        }
        Rule::HumanReviewAbove { amount_cents, .. } => (
            "Human review for large refunds.".to_owned(),
            format!(
                "Refunds above {} are reviewed by a support agent before any approval.",
                format_cents(*amount_cents)
            ),
        ),
        Rule::DamagedOrIncorrectEligible { .. } => (
            "Damaged or incorrect items.".to_owned(),
            "Items that arrived damaged, or that differ from what was ordered, are eligible for a refund.".to_owned(),
        ),
        Rule::RepeatClaimLimit {
            max_claims,
            lookback_days,
            ..
        } => (
            "Repeat claims.".to_owned(),
            format!(
                "Customers with {max_claims} or more refund claims in the last {} have further claims reviewed by a support agent.",
                count(*lookback_days, "day")
            ),
        ),
        Rule::ConflictingClaimEscalates { .. } => (
            "Conflicting claims.".to_owned(),
            "Requests that conflict with our order records, or that contradict earlier statements, are reviewed by a support agent.".to_owned(),
        ),
    };
    RuleText { title, body }
}

/// The policy as Markdown. Disabled rules are left out and the numbering covers
/// enabled rules only. No version number, so the default renders to a stable file.
pub fn render_policy(policy: &Policy) -> String {
    let enabled: Vec<_> = policy.rules.iter().filter(|r| r.enabled()).collect();
    let mut out = format!("{TITLE}\n\n");
    if enabled.is_empty() {
        out.push_str(NO_ACTIVE_RULES);
        out.push_str("\n\n");
    } else {
        out.push_str(INTRO);
        out.push_str("\n\n");
        for (i, rule) in enabled.iter().enumerate() {
            let RuleText { title, body } = rule_text(rule);
            out.push_str(&format!("{}. **{title}** {body}\n", i + 1));
        }
        out.push('\n');
    }
    out.push_str(NOT_COVERED_REASON);
    out.push('\n');
    out
}

fn count(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(json: &str) -> Policy {
        Policy::parse(json).unwrap()
    }

    #[test]
    fn disabled_rules_are_omitted_and_numbering_closes_up() {
        let p = policy(
            r#"{"rules":[
                {"kind":"final_sale_not_refundable","enabled":false},
                {"kind":"damaged_or_incorrect_eligible","enabled":true},
                {"kind":"conflicting_claim_escalates","enabled":true}
            ]}"#,
        );
        let text = render_policy(&p);
        assert!(!text.contains("Final sale"));
        assert!(text.contains("1. **Damaged or incorrect items.**"));
        assert!(text.contains("2. **Conflicting claims.**"));
    }

    #[test]
    fn category_window_and_singular_units() {
        let p = policy(
            r#"{"rules":[
                {"kind":"refund_window","enabled":true,"days":1,"scope":{"kind":"category","category":"electronics"}},
                {"kind":"repeat_claim_limit","enabled":true,"max_claims":1,"lookback_days":1}
            ]}"#,
        );
        let text = render_policy(&p);
        assert!(text.contains(
            "**Refund window (electronics).** Refunds for electronics items must be requested within 1 day of delivery"
        ));
        assert!(text.contains("1 or more refund claims in the last 1 day have"));
    }

    #[test]
    fn threshold_uses_formatted_money() {
        let p = policy(
            r#"{"rules":[{"kind":"human_review_above","enabled":true,"amount_cents":125050}]}"#,
        );
        assert!(render_policy(&p).contains("Refunds above $1,250.50 are reviewed"));
    }

    #[test]
    fn all_rules_disabled_says_so() {
        let p = policy(r#"{"rules":[{"kind":"final_sale_not_refundable","enabled":false}]}"#);
        assert_eq!(
            render_policy(&p),
            format!("{TITLE}\n\n{NO_ACTIVE_RULES}\n\n{NOT_COVERED_REASON}\n")
        );
    }

    #[test]
    fn customer_reasons_never_state_a_verdict_word() {
        // Reasons are passed to the responder, whose reply is rejected if it
        // contains a verdict word other than the decided one.
        let p = policy(include_str!("../../../../policy/default-policy.json"));
        let mut texts: Vec<String> = p.rules.iter().map(|r| rule_text(r).body).collect();
        texts.retain(|t| !t.contains("Later requests are denied"));
        texts.extend(
            [
                NOT_COVERED_REASON,
                CLOSER_LOOK_REASON,
                ALREADY_REFUNDED_REASON,
            ]
            .map(String::from),
        );
        for t in texts {
            let lower = t.to_lowercase();
            for word in ["approved", "denied", "escalated"] {
                assert!(!lower.contains(word), "{t:?} contains {word:?}");
            }
        }
    }
}

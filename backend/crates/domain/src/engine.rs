//! The refund decision (ADR-001, ADR-017). `decide` is a pure function of the
//! policy and the facts: no clock, no I/O, no LLM. Every enabled rule is
//! evaluated, the most severe verdict wins, and nothing fired means Escalated.
//!
//! Two checks sit outside the configurable policy, so no admin setting can
//! switch them off: any flag fails closed (ADR-003), and an item that already
//! has an approved refund is never approved again (ADR-013).

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::money::format_cents;
use crate::policy::{Policy, Rule, WindowScope};
use crate::prose::{self, ALREADY_REFUNDED_REASON, CLOSER_LOOK_REASON, NOT_COVERED_REASON};
use crate::types::{Flag, OrderStatus, ReasonCategory, RequestState, Verdict};

/// Kind recorded when flags force an escalation.
pub const FAIL_CLOSED: &str = "fail_closed";
/// Kind recorded when the item already has an approved refund.
pub const ACTIVE_REFUND_EXISTS: &str = "active_refund_exists";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemFacts {
    pub order_item_id: Uuid,
    pub name: String,
    pub category: String,
    pub amount_cents: i64,
    pub final_sale: bool,
    pub has_active_refund: bool,
}

/// The order and the one item the request is about, from the database.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderFacts {
    pub order_id: Uuid,
    pub order_ref: String,
    pub placed_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub status: OrderStatus,
    pub item: ItemFacts,
}

/// An earlier refund request by the same customer, in any state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PriorClaim {
    pub decided_at: DateTime<Utc>,
    pub state: RequestState,
}

/// What the customer says, as extracted by intake. Never trusted for identity or amounts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Claims {
    pub reason: Option<ReasonCategory>,
    pub claimed_amount_cents: Option<i64>,
    pub contradictory_statements: bool,
}

/// Everything `decide` looks at. Stored verbatim in `decision_audit.facts`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Facts {
    pub now: DateTime<Utc>,
    pub order: Option<OrderFacts>,
    pub claims: Claims,
    pub prior_claims: Vec<PriorClaim>,
    pub flags: Vec<Flag>,
}

/// One entry of the rule trace, stored in `decision_audit.rule_trace`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FiredRule {
    /// A `Rule::kind()`, or `FAIL_CLOSED` / `ACTIVE_REFUND_EXISTS`.
    pub kind: String,
    pub verdict: Verdict,
    /// For the admin trace: names the numbers and conditions that matched.
    pub explanation: String,
    /// For the customer: the policy sentence behind this rule. Reveals nothing
    /// about screening.
    pub customer_reason: String,
    pub detail: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub verdict: Verdict,
    /// Most severe first; the order never depends on the order of rules in the policy.
    pub fired: Vec<FiredRule>,
    /// Sorted and deduplicated; includes `NoRuleFired` when nothing fired.
    pub flags: Vec<Flag>,
}

impl Decision {
    /// Customer-safe reasons for the final verdict: the sentences of the rules
    /// that set it, without duplicates. Never empty.
    pub fn customer_reasons(&self) -> Vec<String> {
        let mut reasons: Vec<String> = Vec::new();
        for f in self.fired.iter().filter(|f| f.verdict == self.verdict) {
            if !reasons.contains(&f.customer_reason) {
                reasons.push(f.customer_reason.clone());
            }
        }
        if reasons.is_empty() {
            reasons.push(NOT_COVERED_REASON.to_owned());
        }
        reasons
    }
}

pub fn decide(policy: &Policy, facts: &Facts) -> Decision {
    let mut fired: Vec<FiredRule> = policy
        .rules
        .iter()
        .filter(|r| r.enabled())
        .filter_map(|r| evaluate(r, facts))
        .collect();

    let mut flags = facts.flags.clone();
    flags.sort();
    flags.dedup();
    if !flags.is_empty() {
        fired.push(fail_closed(&flags));
    }
    if let Some(order) = &facts.order
        && order.item.has_active_refund
    {
        fired.push(active_refund(order));
    }

    let verdict = match fired.iter().map(|f| f.verdict).reduce(Verdict::max) {
        Some(v) => v,
        None => {
            flags.push(Flag::NoRuleFired);
            Verdict::Escalated
        }
    };

    fired.sort_by_cached_key(|f| {
        (
            std::cmp::Reverse(f.verdict.severity()),
            f.kind.clone(),
            f.detail.to_string(),
        )
    });
    Decision {
        verdict,
        fired,
        flags,
    }
}

fn evaluate(rule: &Rule, facts: &Facts) -> Option<FiredRule> {
    let order = facts.order.as_ref();
    let fire = |verdict, explanation: String, detail: Value| FiredRule {
        kind: rule.kind().to_owned(),
        verdict,
        explanation,
        customer_reason: prose::rule_text(rule).body,
        detail,
    };

    match rule {
        Rule::FinalSaleNotRefundable { .. } => {
            let item = &order?.item;
            item.final_sale.then(|| {
                fire(
                    Verdict::Denied,
                    format!("\"{}\" was sold as final sale.", item.name),
                    json!({ "final_sale": true }),
                )
            })
        }

        Rule::RefundWindow { days, scope, .. } => {
            let order = order?;
            if let WindowScope::Category { category } = scope
                && !category
                    .trim()
                    .eq_ignore_ascii_case(order.item.category.trim())
            {
                return None;
            }
            let (anchor, anchor_at) = match order.delivered_at {
                Some(at) => ("delivered_at", at),
                None => ("placed_at", order.placed_at),
            };
            let age = facts.now - anchor_at;
            let limit = TimeDelta::try_days(*days).unwrap_or(TimeDelta::MAX);
            (age > limit).then(|| {
                let when = if order.delivered_at.is_some() {
                    "Delivered"
                } else {
                    "Ordered (no delivery recorded)"
                };
                fire(
                    Verdict::Denied,
                    format!(
                        "{when} {} days ago; the refund window is {days} days.",
                        age.num_days()
                    ),
                    json!({
                        "days": days,
                        "scope": scope,
                        "anchor": anchor,
                        "anchor_at": anchor_at,
                        "age_days": age.num_days(),
                    }),
                )
            })
        }

        Rule::HumanReviewAbove { amount_cents, .. } => {
            let item = &order?.item;
            (item.amount_cents > *amount_cents).then(|| {
                fire(
                    Verdict::Escalated,
                    format!(
                        "Item amount {} is above the {} review threshold.",
                        format_cents(item.amount_cents),
                        format_cents(*amount_cents)
                    ),
                    json!({ "amount_cents": item.amount_cents, "threshold_cents": amount_cents }),
                )
            })
        }

        Rule::DamagedOrIncorrectEligible { .. } => {
            order?;
            let reason = facts.claims.reason?;
            matches!(reason, ReasonCategory::Damaged | ReasonCategory::WrongItem).then(|| {
                fire(
                    Verdict::Approved,
                    format!("Reason is {reason}; damaged or incorrect items are eligible."),
                    json!({ "reason": reason }),
                )
            })
        }

        Rule::RepeatClaimLimit {
            max_claims,
            lookback_days,
            ..
        } => {
            let since = TimeDelta::try_days(*lookback_days)
                .and_then(|d| facts.now.checked_sub_signed(d))
                .unwrap_or(DateTime::<Utc>::MIN_UTC);
            let recent = facts
                .prior_claims
                .iter()
                .filter(|c| c.decided_at >= since)
                .count() as i64;
            (recent >= *max_claims).then(|| {
                fire(
                    Verdict::Escalated,
                    format!(
                        "{recent} prior refund claims in the last {lookback_days} days; the limit is {max_claims}."
                    ),
                    json!({
                        "recent_claims": recent,
                        "max_claims": max_claims,
                        "lookback_days": lookback_days,
                    }),
                )
            })
        }

        Rule::ConflictingClaimEscalates { .. } => {
            let claims = &facts.claims;
            let mut matched: Vec<(&str, String)> = Vec::new();
            if let Some(order) = order {
                if claims.reason == Some(ReasonCategory::NotReceived)
                    && order.status == OrderStatus::Delivered
                {
                    matched.push((
                        "not_received_but_delivered",
                        "Customer says the order never arrived, but it is recorded as delivered."
                            .to_owned(),
                    ));
                }
                if let Some(claimed) = claims.claimed_amount_cents
                    && claimed > order.item.amount_cents
                {
                    matched.push((
                        "claimed_amount_exceeds_paid",
                        format!(
                            "Customer claims {} but the item cost {}.",
                            format_cents(claimed),
                            format_cents(order.item.amount_cents)
                        ),
                    ));
                }
            }
            if claims.contradictory_statements {
                matched.push((
                    "contradictory_statements",
                    "The customer's messages contradict each other.".to_owned(),
                ));
            }
            (!matched.is_empty()).then(|| {
                let (conditions, sentences): (Vec<_>, Vec<_>) = matched.into_iter().unzip();
                fire(
                    Verdict::Escalated,
                    sentences.join(" "),
                    json!({ "conditions": conditions }),
                )
            })
        }
    }
}

fn fail_closed(flags: &[Flag]) -> FiredRule {
    let names: Vec<_> = flags.iter().map(|f| f.as_str()).collect();
    FiredRule {
        kind: FAIL_CLOSED.to_owned(),
        verdict: Verdict::Escalated,
        explanation: format!(
            "Flags raised: {}. Uncertain requests always go to a person.",
            names.join(", ")
        ),
        customer_reason: CLOSER_LOOK_REASON.to_owned(),
        detail: json!({ "flags": flags }),
    }
}

fn active_refund(order: &OrderFacts) -> FiredRule {
    FiredRule {
        kind: ACTIVE_REFUND_EXISTS.to_owned(),
        verdict: Verdict::Escalated,
        explanation: format!(
            "\"{}\" on {} already has an approved refund.",
            order.item.name, order.order_ref
        ),
        customer_reason: ALREADY_REFUNDED_REASON.to_owned(),
        detail: json!({ "order_item_id": order.item.order_item_id }),
    }
}

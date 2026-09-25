//! Responder stage contract. The responder only words a decision that is
//! already made; `validate_reply` rejects any reply that names a different
//! outcome, and `fallback_reply` is the Rust template used when the model fails.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::intake::MissingField;
use crate::money::format_cents;
use crate::types::{RequestState, Verdict};

pub const MAX_REPLY_CHARS: usize = 1200;

/// What the refund is for. `amount` is preformatted so the model can copy it exactly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub order_ref: String,
    pub item_name: String,
    pub amount_cents: i64,
    pub amount: String,
}

impl Target {
    pub fn new(
        order_ref: impl Into<String>,
        item_name: impl Into<String>,
        amount_cents: i64,
    ) -> Self {
        Self {
            order_ref: order_ref.into(),
            item_name: item_name.into(),
            amount_cents,
            amount: format_cents(amount_cents),
        }
    }

    fn phrase(&self) -> String {
        format!("{} (order {})", self.item_name, self.order_ref)
    }
}

/// Serialized as the responder's user content. Customer text is never included.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ResponderInput {
    Clarify {
        missing: Vec<MissingField>,
        clarify_turn: u8,
        policy_prose: String,
    },
    Verdict {
        verdict: Verdict,
        target: Option<Target>,
        /// Customer-safe sentences from `Decision::customer_reasons`.
        reasons: Vec<String>,
        policy_prose: String,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ReplyExpectation {
    Clarify,
    Verdict {
        verdict: Verdict,
        amount_cents: Option<i64>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyViolation(pub String);

impl fmt::Display for ReplyViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReplyViolation {}

/// Case-insensitive word checks on the whole reply, so a model that drifts
/// from the decided outcome is caught before the customer sees it.
pub fn validate_reply(reply: &str, expectation: &ReplyExpectation) -> Result<(), ReplyViolation> {
    let chars = reply.trim().chars().count();
    if chars == 0 {
        return Err(ReplyViolation("reply is empty".into()));
    }
    if chars > MAX_REPLY_CHARS {
        return Err(ReplyViolation(format!(
            "reply is {chars} chars; the limit is {MAX_REPLY_CHARS}"
        )));
    }

    let lower = reply.to_lowercase();
    let (required, forbidden): (&str, &[&str]) = match expectation {
        ReplyExpectation::Clarify => ("?", &["approved", "denied", "escalated"]),
        ReplyExpectation::Verdict { verdict, .. } => match verdict {
            Verdict::Approved => ("approved", &["not approved", "denied", "escalated"]),
            Verdict::Denied => ("denied", &["approved", "escalated"]),
            Verdict::Escalated => ("escalated", &["approved", "denied"]),
        },
    };
    if !lower.contains(required) {
        return Err(ReplyViolation(format!("reply must contain \"{required}\"")));
    }
    if let Some(word) = forbidden.iter().find(|w| lower.contains(*w)) {
        return Err(ReplyViolation(format!("reply must not contain \"{word}\"")));
    }
    if let ReplyExpectation::Verdict {
        verdict: Verdict::Approved,
        amount_cents: Some(cents),
    } = expectation
    {
        let amount = format_cents(*cents);
        if !reply.contains(&amount) {
            return Err(ReplyViolation(format!(
                "reply must state the amount {amount}"
            )));
        }
    }
    Ok(())
}

/// Template reply for when the responder model fails twice. Always passes
/// `validate_reply` for the same expectation.
pub fn fallback_reply(expectation: &ReplyExpectation, target: Option<&Target>) -> String {
    let subject = target.map_or_else(|| "your request".to_owned(), Target::phrase);
    match expectation {
        ReplyExpectation::Clarify => {
            "Could you tell me which order and item this is about, and what went wrong with it?"
                .to_owned()
        }
        ReplyExpectation::Verdict {
            verdict,
            amount_cents,
        } => match verdict {
            Verdict::Approved => match amount_cents.or(target.map(|t| t.amount_cents)) {
                Some(cents) => format!(
                    "Good news: your refund of {} for {subject} has been approved.",
                    format_cents(cents)
                ),
                None => format!("Good news: your refund for {subject} has been approved."),
            },
            Verdict::Denied => format!(
                "Unfortunately, your refund request for {subject} has been denied under our refund policy."
            ),
            Verdict::Escalated => format!(
                "Your refund request for {subject} has been escalated to our support team for review. A support agent will follow up with you."
            ),
        },
    }
}

/// Reply to a message sent after the request was decided. Not model-generated.
pub fn holding_reply(request_ref: &str, state: RequestState) -> String {
    let outcome = match state {
        RequestState::Approved => "approved",
        RequestState::Denied => "denied",
        RequestState::Escalated => "escalated to our support team",
        RequestState::ResolvedApproved => "approved after review",
        RequestState::ResolvedDenied => "denied after review",
    };
    format!(
        "Thanks for the update. Your request {request_ref} has already been {outcome}; a support agent will see this message."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ReplyExpectation as E;

    fn verdict(v: Verdict, amount_cents: Option<i64>) -> E {
        E::Verdict {
            verdict: v,
            amount_cents,
        }
    }

    fn check(reply: &str, e: E) -> Result<(), String> {
        validate_reply(reply, &e).map_err(|v| v.0)
    }

    #[test]
    fn replies_that_match_the_decision_pass() {
        let ok = [
            (
                "Good news: your refund of $89.99 for Wireless headphones has been approved.",
                verdict(Verdict::Approved, Some(8999)),
            ),
            (
                "Unfortunately, your refund request for Winter coat has been denied. Final sale items cannot be refunded.",
                verdict(Verdict::Denied, None),
            ),
            (
                "Your refund request for 4K TV has been ESCALATED to our support team for review.",
                verdict(Verdict::Escalated, None),
            ),
            ("Which order is this about?", E::Clarify),
        ];
        for (reply, e) in ok {
            assert_eq!(check(reply, e), Ok(()), "{reply}");
        }
    }

    #[test]
    fn replies_that_drift_from_the_decision_fail() {
        let bad = [
            (
                "Your refund has been denied.",
                verdict(Verdict::Approved, None),
                "must contain \"approved\"",
            ),
            (
                "Your refund was not approved.",
                verdict(Verdict::Approved, None),
                "\"not approved\"",
            ),
            (
                "Approved! It was escalated first.",
                verdict(Verdict::Approved, None),
                "\"escalated\"",
            ),
            (
                "Your refund of $90.00 has been approved.",
                verdict(Verdict::Approved, Some(8999)),
                "amount $89.99",
            ),
            (
                "It has been denied, then approved.",
                verdict(Verdict::Denied, None),
                "\"approved\"",
            ),
            (
                "This is escalated but will be denied.",
                verdict(Verdict::Escalated, None),
                "\"denied\"",
            ),
            (
                "Your refund is approved, right?",
                E::Clarify,
                "\"approved\"",
            ),
            (
                "Tell me the order number.",
                E::Clarify,
                "must contain \"?\"",
            ),
            ("   ", E::Clarify, "empty"),
        ];
        for (reply, e, why) in bad {
            let err = check(reply, e).expect_err(reply);
            assert!(err.contains(why), "{reply}: {err}");
        }
        let long = format!("{}?", "a".repeat(MAX_REPLY_CHARS));
        assert!(check(&long, E::Clarify).unwrap_err().contains("limit"));
    }

    #[test]
    fn fallback_always_passes_its_own_validation() {
        let target = Target::new("ORD-1006", "4K OLED TV", 129_900);
        let mut expectations = vec![E::Clarify];
        for v in Verdict::ALL {
            expectations.push(verdict(*v, None));
            expectations.push(verdict(*v, Some(129_900)));
        }
        for e in expectations {
            for t in [None, Some(&target)] {
                let reply = fallback_reply(&e, t);
                assert_eq!(check(&reply, e), Ok(()), "{e:?} {t:?}: {reply}");
            }
        }
        let approved = fallback_reply(&verdict(Verdict::Approved, None), Some(&target));
        assert!(approved.contains("$1,299.00") && approved.contains("ORD-1006"));
    }

    #[test]
    fn holding_reply_names_the_request_and_its_state() {
        assert_eq!(
            holding_reply("RR-1001", RequestState::ResolvedDenied),
            "Thanks for the update. Your request RR-1001 has already been denied after review; a support agent will see this message."
        );
    }
}

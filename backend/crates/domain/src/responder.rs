//! Responder stage contract. The responder only words a decision that is
//! already made; `validate_reply` rejects any reply that names a different
//! outcome, and `fallback_reply` is the Rust template used when the model fails.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::intake::MissingField;
use crate::money::format_cents;
use crate::types::{RequestState, Verdict};

pub const MAX_REPLY_CHARS: usize = 1200;

/// Fixed replies that need no model: nothing in them depends on the customer's records.
pub const GREETING_REPLY: &str = "Hi, I'm your refund assistant. How may I help you with refund requests for your Worknoon orders?";
pub const ORDER_LIST_REPLY: &str = "Here are your orders. Pick one to continue.";

fn on(at: DateTime<Utc>) -> String {
    at.format("%b %-d, %Y").to_string()
}

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

/// An earlier request for the item the customer is asking about. `status` is
/// a preformatted clause ("was approved on Sep 18, 2026") the model copies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorRequest {
    #[serde(rename = "ref")]
    pub request_ref: String,
    pub order_ref: String,
    pub item_name: String,
    pub state: RequestState,
    pub status: String,
}

impl PriorRequest {
    pub fn new(
        request_ref: impl Into<String>,
        order_ref: impl Into<String>,
        item_name: impl Into<String>,
        state: RequestState,
        decided_at: DateTime<Utc>,
    ) -> Self {
        let on = on(decided_at);
        let status = match state {
            RequestState::Approved => format!("was approved on {on}"),
            RequestState::Denied => format!("was denied on {on}"),
            RequestState::Escalated => {
                format!("has been with our support team for review since {on}")
            }
            RequestState::ResolvedApproved => format!("was approved after review on {on}"),
            RequestState::ResolvedDenied => format!("was denied after review on {on}"),
        };
        Self {
            request_ref: request_ref.into(),
            order_ref: order_ref.into(),
            item_name: item_name.into(),
            state,
            status,
        }
    }
}

/// What is on record for an order the customer asked about (trusted). Dates
/// and amounts are preformatted so the model can copy them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRecord {
    pub order_ref: String,
    pub placed_on: String,
    pub delivered_on: Option<String>,
    pub items: Vec<ItemRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRecord {
    pub name: String,
    pub amount: String,
    /// The item's newest refund request, or `None` when it has none.
    pub request: Option<PriorRequest>,
}

impl OrderRecord {
    pub fn new(
        order_ref: impl Into<String>,
        placed_at: DateTime<Utc>,
        delivered_at: Option<DateTime<Utc>>,
        items: Vec<ItemRecord>,
    ) -> Self {
        Self {
            order_ref: order_ref.into(),
            placed_on: on(placed_at),
            delivered_on: delivered_at.map(on),
            items,
        }
    }

    fn requests(&self) -> impl Iterator<Item = &PriorRequest> {
        self.items.iter().filter_map(|i| i.request.as_ref())
    }

    /// Items that could still get a request.
    fn open_items(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter(|i| i.request.is_none())
            .map(|i| i.name.as_str())
            .collect()
    }
}

impl ItemRecord {
    pub fn new(name: impl Into<String>, amount_cents: i64, request: Option<PriorRequest>) -> Self {
        Self {
            name: name.into(),
            amount: format_cents(amount_cents),
            request,
        }
    }
}

/// The outcome word a request's state allows in a reply.
fn outcome_word(state: RequestState) -> &'static str {
    match state {
        RequestState::Approved | RequestState::ResolvedApproved => "approved",
        RequestState::Denied | RequestState::ResolvedDenied => "denied",
        RequestState::Escalated => "escalated",
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
    /// The item already has a request: say where it stands and ask whether
    /// there is anything else. Nothing is filed.
    ExistingRequest { request: PriorRequest },
    /// The request is complete: ask once whether there is anything else to
    /// add before it is checked. Nothing is decided yet.
    FinalCheck { target: Option<Target> },
    /// The customer asked about an order: say what is on record and offer a
    /// request for items without one. Nothing is filed.
    OrderStatus { order: OrderRecord },
    /// The customer needs nothing else. Nothing is filed.
    Closing,
    /// An off-topic message: decline it and steer back to refunds.
    Redirect,
}

impl ResponderInput {
    pub fn target(&self) -> Option<&Target> {
        match self {
            ResponderInput::Verdict { target, .. } => target.as_ref(),
            _ => None,
        }
    }

    /// What `validate_reply` checks a reply to this input against. Only an
    /// approval must state the amount.
    pub fn expectation(&self) -> ReplyExpectation {
        match self {
            ResponderInput::Clarify { .. } => ReplyExpectation::Clarify,
            ResponderInput::Verdict {
                verdict, target, ..
            } => ReplyExpectation::Verdict {
                verdict: *verdict,
                amount_cents: target
                    .as_ref()
                    .filter(|_| *verdict == Verdict::Approved)
                    .map(|t| t.amount_cents),
            },
            ResponderInput::ExistingRequest { request } => {
                ReplyExpectation::ExistingRequest(request.clone())
            }
            ResponderInput::FinalCheck { .. } => ReplyExpectation::FinalCheck,
            ResponderInput::OrderStatus { order } => ReplyExpectation::OrderStatus(order.clone()),
            ResponderInput::Closing => ReplyExpectation::Closing,
            ResponderInput::Redirect => ReplyExpectation::Redirect,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplyExpectation {
    Clarify,
    Verdict {
        verdict: Verdict,
        amount_cents: Option<i64>,
    },
    ExistingRequest(PriorRequest),
    FinalCheck,
    OrderStatus(OrderRecord),
    Closing,
    Redirect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyViolation(pub String);

impl fmt::Display for ReplyViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReplyViolation {}

/// Removes invisible formatting characters (soft hyphen, zero-width and bidi
/// controls, word joiners, BOM, tag characters) and trims. Models sometimes
/// emit them; run before `validate_reply` so a verdict word cannot hide from
/// the checks behind one, and the customer never receives them.
pub fn clean_reply(reply: &str) -> String {
    let invisible = |c: char| {
        matches!(c,
            '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
            | '\u{E0000}'..='\u{E007F}')
    };
    let cleaned: String = reply.chars().filter(|c| !invisible(*c)).collect();
    cleaned.trim().to_owned()
}

/// Case-insensitive word checks on the whole reply, so a model that drifts
/// from the decided outcome is caught before the customer sees it. Expects
/// text that went through `clean_reply`.
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
    const OUTCOMES: &[&str] = &["approved", "denied", "escalated"];
    let (required, forbidden): (Vec<&str>, Vec<&str>) = match expectation {
        ReplyExpectation::Clarify | ReplyExpectation::FinalCheck => (vec!["?"], OUTCOMES.to_vec()),
        ReplyExpectation::Verdict { verdict, .. } => match verdict {
            Verdict::Approved => (
                vec!["approved"],
                vec!["not approved", "denied", "escalated"],
            ),
            Verdict::Denied => (vec!["denied"], vec!["approved", "escalated"]),
            Verdict::Escalated => (vec!["escalated"], vec!["approved", "denied"]),
        },
        // Names the earlier request and its real outcome, then asks a question.
        ReplyExpectation::ExistingRequest(prior) => {
            let mut required = vec![prior.request_ref.as_str(), "?"];
            let forbidden = match prior.state {
                RequestState::Approved | RequestState::ResolvedApproved => {
                    required.push("approved");
                    vec!["not approved", "denied", "escalated"]
                }
                RequestState::Denied | RequestState::ResolvedDenied => {
                    required.push("denied");
                    vec!["approved", "escalated"]
                }
                RequestState::Escalated => vec!["approved", "denied"],
            };
            (required, forbidden)
        }
        // Names the order and every earlier request; an outcome word only
        // where one of those requests has it; a question when an item is open.
        ReplyExpectation::OrderStatus(order) => {
            let mut required = vec![order.order_ref.as_str()];
            required.extend(order.requests().map(|r| r.request_ref.as_str()));
            if !order.open_items().is_empty() {
                required.push("?");
            }
            let allowed: Vec<&str> = order.requests().map(|r| outcome_word(r.state)).collect();
            let forbidden = OUTCOMES
                .iter()
                .copied()
                .filter(|w| !allowed.contains(w))
                .collect();
            (required, forbidden)
        }
        ReplyExpectation::Closing | ReplyExpectation::Redirect => (vec![], OUTCOMES.to_vec()),
    };
    if let Some(word) = required.iter().find(|w| !lower.contains(&w.to_lowercase())) {
        return Err(ReplyViolation(format!("reply must contain \"{word}\"")));
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
    let about = target.map_or_else(String::new, |t| format!(" for {}", t.phrase()));
    match expectation {
        ReplyExpectation::ExistingRequest(r) => format!(
            "Your refund request {} for {} (order {}) {}. Is there anything else I can help you with?",
            r.request_ref, r.item_name, r.order_ref, r.status
        ),
        ReplyExpectation::OrderStatus(order) => order_status_reply(order),
        ReplyExpectation::FinalCheck => {
            "Got it. Anything else I should know before I check this?".to_owned()
        }
        ReplyExpectation::Closing => {
            "Thanks for getting in touch. Message here any time if something else comes up with an order."
                .to_owned()
        }
        ReplyExpectation::Redirect => {
            "I can only help with refund requests for your Worknoon orders. Which order do you need help with?"
                .to_owned()
        }
        ReplyExpectation::Clarify => {
            "Which order and item is this about, and what went wrong?".to_owned()
        }
        ReplyExpectation::Verdict {
            verdict,
            amount_cents,
        } => match verdict {
            Verdict::Approved => {
                let amount = amount_cents
                    .or(target.map(|t| t.amount_cents))
                    .map_or_else(String::new, |c| format!(" of {}", format_cents(c)));
                format!("Good news: your refund{amount}{about} has been approved.")
            }
            Verdict::Denied => {
                format!("Your refund request{about} has been denied under our refund policy.")
            }
            Verdict::Escalated => format!(
                "Your refund request{about} has been escalated to our support team for review. A support agent will follow up with you."
            ),
        },
    }
}

fn order_status_reply(order: &OrderRecord) -> String {
    let mut out = format!(
        "Order {} was placed on {}",
        order.order_ref, order.placed_on
    );
    if let Some(delivered) = &order.delivered_on {
        out.push_str(&format!(" and delivered on {delivered}"));
    }
    out.push('.');
    for item in &order.items {
        match &item.request {
            Some(r) => out.push_str(&format!(
                " {} ({}): refund request {} {}.",
                item.name, item.amount, r.request_ref, r.status
            )),
            None => out.push_str(&format!(
                " {} ({}): no refund request.",
                item.name, item.amount
            )),
        }
    }
    let open = order.open_items();
    if open.is_empty() {
        out.push_str(" Is there anything else I can help you with?");
    } else {
        out.push_str(&format!(
            " Would you like to request a refund for {}?",
            open.join(" or ")
        ));
    }
    out
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

    fn prior(state: RequestState) -> PriorRequest {
        let decided = DateTime::parse_from_rfc3339("2026-09-18T10:00:00Z").unwrap();
        PriorRequest::new(
            "RR-0903",
            "ORD-10340",
            "Day Pass, 5-pack",
            state,
            decided.with_timezone(&Utc),
        )
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
        let mut expectations = vec![E::Clarify, E::FinalCheck, E::Closing, E::Redirect];
        for v in Verdict::ALL {
            expectations.push(verdict(*v, None));
            expectations.push(verdict(*v, Some(129_900)));
        }
        for state in RequestState::ALL {
            expectations.push(E::ExistingRequest(prior(*state)));
            expectations.push(E::OrderStatus(order(Some(*state))));
        }
        expectations.push(E::OrderStatus(order(None)));
        for e in expectations {
            for t in [None, Some(&target)] {
                let reply = fallback_reply(&e, t);
                assert_eq!(check(&reply, e.clone()), Ok(()), "{e:?} {t:?}: {reply}");
            }
        }
        let approved = fallback_reply(&verdict(Verdict::Approved, None), Some(&target));
        assert!(approved.contains("$1,299.00") && approved.contains("ORD-1006"));
        assert_eq!(
            fallback_reply(&verdict(Verdict::Escalated, None), None),
            "Your refund request has been escalated to our support team for review. A support agent will follow up with you."
        );
    }

    #[test]
    fn expectation_requires_the_amount_only_for_approvals() {
        let target = Some(Target::new("ORD-1001", "Wireless headphones", 8999));
        let input = |v| ResponderInput::Verdict {
            verdict: v,
            target: target.clone(),
            reasons: vec![],
            policy_prose: String::new(),
        };
        assert_eq!(
            input(Verdict::Approved).expectation(),
            verdict(Verdict::Approved, Some(8999))
        );
        assert_eq!(
            input(Verdict::Denied).expectation(),
            verdict(Verdict::Denied, None)
        );
        let clarify = ResponderInput::Clarify {
            missing: vec![MissingField::Order],
            clarify_turn: 1,
            policy_prose: String::new(),
        };
        assert_eq!(clarify.expectation(), E::Clarify);
        assert_eq!(clarify.target(), None);
    }

    #[test]
    fn invisible_characters_are_removed_before_validation() {
        assert_eq!(
            clean_reply(" Clear\u{AD}ance coat\u{200B} was de\u{2060}nied.\u{FEFF}\n"),
            "Clearance coat was denied."
        );
        // Hidden inside a forbidden word, it would otherwise pass the check.
        let sneaky = "Your refund request has been denied, but it may be ap\u{AD}proved later.";
        assert_eq!(check(sneaky, verdict(Verdict::Denied, None)), Ok(()));
        let err = check(&clean_reply(sneaky), verdict(Verdict::Denied, None)).unwrap_err();
        assert!(err.contains("\"approved\""), "{err}");
    }

    #[test]
    fn an_existing_request_reply_names_it_asks_and_keeps_its_outcome() {
        let approved = E::ExistingRequest(prior(RequestState::ResolvedApproved));
        assert_eq!(
            prior(RequestState::ResolvedApproved).status,
            "was approved after review on Sep 18, 2026"
        );
        let ok = "Your refund request rr-0903 for the Day Pass was approved after review on Sep 18. Anything else?";
        assert_eq!(check(ok, approved.clone()), Ok(()));
        let bad = [
            ("Your request was approved. Anything else?", "\"RR-0903\""),
            ("RR-0903 was approved on Sep 18.", "\"?\""),
            ("RR-0903 is being reviewed. Anything else?", "\"approved\""),
            (
                "RR-0903 was approved, not denied. Anything else?",
                "\"denied\"",
            ),
        ];
        for (reply, why) in bad {
            let err = check(reply, approved.clone()).expect_err(reply);
            assert!(err.contains(why), "{reply}: {err}");
        }
        let open = E::ExistingRequest(prior(RequestState::Escalated));
        assert_eq!(
            check(
                "RR-0903 is with our support team. Anything else?",
                open.clone()
            ),
            Ok(())
        );
        assert!(check("RR-0903 will be approved soon?", open).is_err());
    }

    /// ORD-10340 with the day passes under `pass` and a locker with no request.
    fn order(pass: Option<RequestState>) -> OrderRecord {
        let at = |d: &str| DateTime::parse_from_rfc3339(d).unwrap().with_timezone(&Utc);
        OrderRecord::new(
            "ORD-10340",
            at("2026-09-10T10:00:00Z"),
            Some(at("2026-09-11T10:00:00Z")),
            vec![
                ItemRecord::new("Day Pass, 5-pack", 7500, pass.map(prior)),
                ItemRecord::new("Locker Rental", 2000, None),
            ],
        )
    }

    #[test]
    fn an_order_status_reply_states_the_record_and_offers_open_items() {
        let e = E::OrderStatus(order(Some(RequestState::ResolvedApproved)));
        assert_eq!(
            fallback_reply(&e, None),
            "Order ORD-10340 was placed on Sep 10, 2026 and delivered on Sep 11, 2026. Day Pass, 5-pack ($75.00): refund request RR-0903 was approved after review on Sep 18, 2026. Locker Rental ($20.00): no refund request. Would you like to request a refund for Locker Rental?"
        );
        let ok = "ORD-10340: the day passes were refunded under RR-0903 (approved). Want a refund for the locker?";
        assert_eq!(check(ok, e.clone()), Ok(()));
        let bad = [
            ("RR-0903 was approved. Anything else?", "\"ORD-10340\""),
            ("ORD-10340: the passes were approved.", "\"RR-0903\""),
            (
                "ORD-10340: the passes were approved under RR-0903.",
                "\"?\"",
            ),
            (
                "ORD-10340: RR-0903 was approved; the locker would be denied?",
                "\"denied\"",
            ),
        ];
        for (reply, why) in bad {
            let err = check(reply, e.clone()).expect_err(reply);
            assert!(err.contains(why), "{reply}: {err}");
        }
        // No request on record: no outcome word at all.
        let fresh = E::OrderStatus(order(None));
        assert!(
            check(
                "ORD-10340 has no requests. Approved, want one?",
                fresh.clone()
            )
            .unwrap_err()
            .contains("\"approved\"")
        );
        assert!(fallback_reply(&fresh, None).ends_with("for Day Pass, 5-pack or Locker Rental?"));
    }

    #[test]
    fn the_final_question_asks_and_names_no_outcome() {
        let target = Some(Target::new("ORD-10437", "Worknoon Desk Lamp", 6200));
        let input = ResponderInput::FinalCheck { target };
        assert_eq!(input.expectation(), E::FinalCheck);
        assert_eq!(
            check(
                "Got it: the desk lamp from ORD-10437. Anything else before I check this?",
                E::FinalCheck
            ),
            Ok(())
        );
        assert!(check("Got it, I'll check this now.", E::FinalCheck).is_err());
        assert!(check("This will be approved, anything else?", E::FinalCheck).is_err());
    }

    #[test]
    fn closing_and_redirect_replies_never_name_an_outcome() {
        for e in [E::Closing, E::Redirect] {
            assert_eq!(
                check("I can only help with refunds for your orders.", e.clone()),
                Ok(())
            );
            let err = check("Your refund was approved, goodbye.", e).unwrap_err();
            assert!(err.contains("\"approved\""), "{err}");
        }
    }

    #[test]
    fn holding_reply_names_the_request_and_its_state() {
        assert_eq!(
            holding_reply("RR-1001", RequestState::ResolvedDenied),
            "Thanks for the update. Your request RR-1001 has already been denied after review; a support agent will see this message."
        );
    }
}

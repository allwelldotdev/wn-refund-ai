//! Resolution notice contract. When an admin resolves an escalation, a model
//! turns the admin's note into a message to the customer ("Dear …", how and
//! why) and a one-line summary shown as a note in the chat. The admin previews
//! both before sending; `validate_notice` rejects text that contradicts the
//! resolution. There is no template fallback: without a valid notice the
//! resolution is not sent.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::responder::ReplyViolation;

pub const MAX_MESSAGE_CHARS: usize = 600;
pub const MAX_SUMMARY_CHARS: usize = 200;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Approved,
    Denied,
}

impl Outcome {
    fn word(self) -> &'static str {
        match self {
            Outcome::Approved => "approved",
            Outcome::Denied => "denied",
        }
    }
}

/// Everything here comes from our records or from the admin (trusted).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NoticeInput {
    pub outcome: Outcome,
    pub first_name: String,
    #[serde(rename = "ref")]
    pub request_ref: String,
    pub item_name: Option<String>,
    pub order_ref: Option<String>,
    /// Preformatted, e.g. "$45.00", so the model can copy it exactly.
    pub amount: Option<String>,
    /// The admin's own words on how and why they decided.
    pub note: String,
}

/// What the notice model must return, as a strict JSON schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoticeOutput {
    /// To the customer, starting "Dear {first name},".
    pub message: String,
    /// One line in the third person, shown as a note in the chat.
    pub summary: String,
}

/// Case-insensitive checks, like `validate_reply`; run on text that went
/// through `responder::clean_reply`.
pub fn validate_notice(out: &NoticeOutput, input: &NoticeInput) -> Result<(), ReplyViolation> {
    let fail = |m: String| Err(ReplyViolation(m));
    let word = input.outcome.word();
    let other = match input.outcome {
        Outcome::Approved => "denied",
        Outcome::Denied => "approved",
    };
    let message = out.message.trim();
    let summary = out.summary.trim();
    if message.is_empty() || summary.is_empty() {
        return fail("message and summary must not be empty".into());
    }
    if message.chars().count() > MAX_MESSAGE_CHARS {
        return fail(format!("message is over {MAX_MESSAGE_CHARS} characters"));
    }
    if summary.chars().count() > MAX_SUMMARY_CHARS || summary.contains('\n') {
        return fail(format!(
            "summary must be one line of at most {MAX_SUMMARY_CHARS} characters"
        ));
    }
    let greeting = format!("dear {}", input.first_name.to_lowercase());
    if !message.to_lowercase().starts_with(&greeting) {
        return fail(format!(
            "message must start with \"Dear {}\"",
            input.first_name
        ));
    }
    for (name, text) in [("message", message), ("summary", summary)] {
        let lower = text.to_lowercase();
        if !lower.contains(word) {
            return fail(format!("{name} must contain \"{word}\""));
        }
        if let Some(bad) = [other, "escalated"].iter().find(|w| lower.contains(*w)) {
            return fail(format!("{name} must not contain \"{bad}\""));
        }
    }
    if input.outcome == Outcome::Approved
        && let Some(amount) = &input.amount
        && !message.contains(amount.as_str())
    {
        return fail(format!("message must state the amount {amount}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(outcome: Outcome) -> NoticeInput {
        NoticeInput {
            outcome,
            first_name: "Tomás".into(),
            request_ref: "RR-1006".into(),
            item_name: Some("Locker Rental, 3 months".into()),
            order_ref: Some("ORD-10397".into()),
            amount: Some("$45.00".into()),
            note: "Checked the mobile checkout: the final-sale notice was missing.".into(),
        }
    }

    fn out(message: &str, summary: &str) -> NoticeOutput {
        NoticeOutput {
            message: message.into(),
            summary: summary.into(),
        }
    }

    fn check(o: &NoticeOutput, outcome: Outcome) -> Result<(), String> {
        validate_notice(o, &input(outcome)).map_err(|v| v.0)
    }

    #[test]
    fn a_notice_that_matches_the_resolution_passes() {
        let approved = out(
            "Dear Tomás, thank you for your patience. We checked the mobile checkout and the final-sale notice was missing, so your refund of $45.00 is approved.",
            "A support specialist approved this refund after finding the final-sale notice missing at checkout.",
        );
        assert_eq!(check(&approved, Outcome::Approved), Ok(()));
        let denied = out(
            "dear tomás, we looked again at your locker rental. It was clearly marked final sale, so the refund stays denied. We're sorry.",
            "A support specialist reviewed the dispute and kept the refund denied.",
        );
        assert_eq!(check(&denied, Outcome::Denied), Ok(()));
    }

    #[test]
    fn a_notice_that_drifts_is_rejected() {
        let cases = [
            (
                out("Hi Tomás, approved: $45.00.", "Approved."),
                Outcome::Approved,
                "start with",
            ),
            (
                out("Dear Tomás, approved.", "Approved."),
                Outcome::Approved,
                "amount $45.00",
            ),
            (
                out("Dear Tomás, it is denied.", "Denied, not approved."),
                Outcome::Denied,
                "summary must not contain \"approved\"",
            ),
            (
                out("Dear Tomás, we looked at it.", "Denied."),
                Outcome::Denied,
                "message must contain \"denied\"",
            ),
            (
                out(
                    "Dear Tomás, approved $45.00.",
                    "Line one\nline two approved",
                ),
                Outcome::Approved,
                "one line",
            ),
            (
                out(
                    "Dear Tomás, $45.00 approved after it was escalated.",
                    "Approved.",
                ),
                Outcome::Approved,
                "\"escalated\"",
            ),
        ];
        for (o, outcome, why) in cases {
            let err = check(&o, outcome).expect_err(&o.message);
            assert!(err.contains(why), "{}: {err}", o.message);
        }
    }

    #[test]
    fn output_schema_is_closed() {
        let schema = serde_json::to_value(schemars::schema_for!(NoticeOutput)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            serde_json::from_value::<NoticeOutput>(serde_json::json!({
                "message": "Dear A, approved.", "summary": "Approved.", "verdict": "approved"
            }))
            .is_err()
        );
    }
}

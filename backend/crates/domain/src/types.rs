//! Shared vocabulary. Each enum's string form is its serde name and equals the
//! value stored in the database (checked against the migration in the tests).

use std::fmt;

/// A stored string that is not a known variant of the named type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownVariant {
    pub type_name: &'static str,
    pub value: String,
}

impl fmt::Display for UnknownVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown {} `{}`", self.type_name, self.value)
    }
}

impl std::error::Error for UnknownVariant {}

string_enum! {
    /// Outcome of the policy engine. Severity: Denied > Escalated > Approved.
    pub enum Verdict {
        Approved = "approved",
        Denied = "denied",
        Escalated = "escalated",
    }
}

impl Verdict {
    pub fn severity(self) -> u8 {
        match self {
            Verdict::Approved => 0,
            Verdict::Escalated => 1,
            Verdict::Denied => 2,
        }
    }

    /// The more severe of the two verdicts.
    pub fn max(self, other: Verdict) -> Verdict {
        if other.severity() > self.severity() {
            other
        } else {
            self
        }
    }
}

string_enum! {
    /// Lifecycle of a refund request. Only `Escalated` can move, to a `Resolved*` state.
    pub enum RequestState {
        Approved = "approved",
        Denied = "denied",
        Escalated = "escalated",
        ResolvedApproved = "resolved_approved",
        ResolvedDenied = "resolved_denied",
    }
}

impl RequestState {
    /// Money is paid or will be paid; at most one such request per order item.
    pub fn is_active_refund(self) -> bool {
        matches!(
            self,
            RequestState::Approved | RequestState::ResolvedApproved
        )
    }
}

impl From<Verdict> for RequestState {
    fn from(v: Verdict) -> Self {
        match v {
            Verdict::Approved => RequestState::Approved,
            Verdict::Denied => RequestState::Denied,
            Verdict::Escalated => RequestState::Escalated,
        }
    }
}

string_enum! {
    pub enum OrderStatus {
        Processing = "processing",
        Shipped = "shipped",
        Delivered = "delivered",
    }
}

string_enum! {
    /// How far along an order is: a product that arrived, a booking or pass
    /// already used, a booking that starts later, or a plan still running.
    pub enum Fulfilment {
        Delivered = "delivered",
        Used = "used",
        Confirmed = "confirmed",
        Active = "active",
    }
}

string_enum! {
    /// `Admin` carries an admin's decision to the customer; `System` is a note
    /// such as "You disputed this decision".
    pub enum MessageRole {
        Customer = "customer",
        Assistant = "assistant",
        Admin = "admin",
        System = "system",
    }
}

string_enum! {
    /// The last three file no request: the item already has one (said where it
    /// stands), the customer needs nothing else, or the message was off-topic.
    pub enum AssistantKind {
        Clarify = "clarify",
        Verdict = "verdict",
        Holding = "holding",
        ExistingRequest = "existing_request",
        Closing = "closing",
        Redirect = "redirect",
        Greeting = "greeting",
        OrderStatus = "order_status",
        OrderList = "order_list",
        FinalCheck = "final_check",
        RequestLink = "request_link",
    }
}

string_enum! {
    /// Why the customer wants a refund, as classified by the intake stage.
    #[derive(schemars::JsonSchema)]
    pub enum ReasonCategory {
        Damaged = "damaged",
        WrongItem = "wrong_item",
        NotReceived = "not_received",
        ChangedMind = "changed_mind",
        NotAsDescribed = "not_as_described",
        Other = "other",
    }
}

string_enum! {
    /// Reasons the pipeline did not trust its own inputs. Any flag escalates,
    /// even over a denial; a responder failure alone keeps a denial (ADR-032).
    #[derive(PartialOrd, Ord)]
    pub enum Flag {
        PrescanSignal = "prescan_signal",
        IntakeInjectionSignal = "intake_injection_signal",
        LowConfidence = "low_confidence",
        ForeignOrderReference = "foreign_order_reference",
        LlmFailure = "llm_failure",
        ResponderFailure = "responder_failure",
        ClarificationLimit = "clarification_limit",
        NoRuleFired = "no_rule_fired",
    }
}

string_enum! {
    /// Whether a pre-scan signal was found in one message or across the recent window.
    pub enum SignalScope {
        Message = "message",
        Window = "window",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    /// Oldest first. A later migration that redefines a CHECK replaces it.
    const MIGRATIONS: &[&str] = &[
        include_str!("../../../migrations/0001_initial.sql"),
        include_str!("../../../migrations/0002_assistant_scope.sql"),
        include_str!("../../../migrations/0003_disputes.sql"),
        include_str!("../../../migrations/0004_test_orders.sql"),
        include_str!("../../../migrations/0005_order_questions.sql"),
        include_str!("../../../migrations/0006_final_check.sql"),
        include_str!("../../../migrations/0008_request_link.sql"),
    ];

    /// The quoted values of the first `CHECK (<column> IN (...))` in the newest
    /// migration that has one.
    fn check_values(column: &str) -> Vec<String> {
        let needle = format!("CHECK ({column} IN");
        let (sql, at) = MIGRATIONS
            .iter()
            .rev()
            .find_map(|sql| sql.find(&needle).map(|at| (sql, at)))
            .unwrap_or_else(|| panic!("no `{needle}` in migrations"));
        let rest = &sql[at + needle.len()..];
        let open = rest.find('(').unwrap();
        let close = rest.find(')').unwrap();
        rest[open + 1..close]
            .split(',')
            .map(|v| v.trim().trim_matches('\'').to_owned())
            .collect()
    }

    fn strings<T: Copy + serde::Serialize>(all: &[T]) -> Vec<String> {
        all.iter()
            .map(|v| {
                serde_json::to_value(v)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn serde_names_match_database_check_constraints() {
        assert_eq!(strings(Verdict::ALL), check_values("verdict"));
        assert_eq!(strings(RequestState::ALL), check_values("state"));
        assert_eq!(strings(OrderStatus::ALL), check_values("status"));
        assert_eq!(strings(MessageRole::ALL), check_values("role"));
        assert_eq!(strings(Fulfilment::ALL), check_values("fulfilment"));
        assert_eq!(strings(AssistantKind::ALL), check_values("assistant_kind"));
        assert_eq!(
            strings(ReasonCategory::ALL),
            check_values("reason_category")
        );
        assert_eq!(strings(SignalScope::ALL), check_values("scope"));
    }

    #[test]
    fn as_str_and_from_str_round_trip() {
        for f in Flag::ALL {
            assert_eq!(Flag::from_str(f.as_str()), Ok(*f));
            assert_eq!(serde_json::to_value(f).unwrap(), f.as_str());
        }
        let err = Verdict::from_str("maybe").unwrap_err();
        assert_eq!(err.to_string(), "unknown Verdict `maybe`");
    }

    #[test]
    fn most_severe_verdict_wins_in_any_order() {
        use Verdict::*;
        for a in Verdict::ALL {
            for b in Verdict::ALL {
                assert_eq!(a.max(*b), b.max(*a));
            }
        }
        assert_eq!(Approved.max(Escalated), Escalated);
        assert_eq!(Escalated.max(Denied), Denied);
        assert_eq!(Approved.max(Denied), Denied);
        assert_eq!(Approved.max(Approved), Approved);
    }

    #[test]
    fn only_approved_states_are_active_refunds() {
        let active: Vec<_> = RequestState::ALL
            .iter()
            .filter(|s| s.is_active_refund())
            .collect();
        assert_eq!(
            active,
            [&RequestState::Approved, &RequestState::ResolvedApproved]
        );
    }
}

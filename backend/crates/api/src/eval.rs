//! Red-team and model-eval cases (`backend/eval/cases.json`) and the decision
//! path they share with the pipeline: the real pre-scan, screening, gate and
//! engine. The offline suite gives each case a stub intake and the live tools
//! give it the model's; either way the verdict comes from `decide`, never
//! from a model.

use std::collections::HashMap;
use std::fmt;

use anyhow::Context;
use chrono::Utc;
use db::Db;
use db::messages::Message;
use db::orders::Order;
use db::seed::{SCENARIOS, item_id, stable_id};
use domain::engine::decide;
use domain::intake::{
    ExistingRequest, InjectionSignal, IntakeInput, IntakeOutput, IntakeStatus, Intent, MissingField,
};
use domain::policy::Policy;
use domain::prescan::{Detector, WindowMessage, prescan, prescan_window};
use domain::types::{AssistantKind, Flag, MessageRole, ReasonCategory, Verdict};
use serde::Deserialize;
use uuid::Uuid;

use crate::pipeline::{Gate, build_facts, gate, intake_input, screen_intake};

/// Unknown fields are errors, so a typo cannot silently drop an expectation.
pub fn parse_cases(json: &str) -> serde_json::Result<Vec<Case>> {
    serde_json::from_str(json)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub kind: CaseKind,
    /// A seeded scenario key; the case runs as that customer, on their orders.
    pub customer: String,
    /// The order picked in the UI, sent with the first customer message.
    #[serde(default)]
    pub selected_order_ref: Option<String>,
    pub turns: Vec<Turn>,
    pub expect: Expect,
    /// What a correct intake returns for the conversation.
    #[serde(default)]
    pub intake_stub: Option<StubIntake>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseKind {
    Attack,
    Legit,
}

/// One message, in conversation order. Assistant turns are our own earlier
/// replies, which intake reads as trusted (ADR-055), so their bodies are our
/// wording, never attack text.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Turn {
    Customer(CustomerTurn),
    Assistant(AssistantTurn),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomerTurn {
    pub customer: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantTurn {
    pub assistant: AssistantKind,
    pub body: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    /// The right outcomes when the pre-scan lets the conversation through. A
    /// pre-scan hit always escalates, whatever this says.
    pub outcomes: Vec<Outcome>,
    /// For an attack: the flags that show it was spotted. Empty when a rule,
    /// not a flag, is what stops it.
    #[serde(default)]
    pub flags_any: Vec<Flag>,
    /// The detectors the pre-scan fires on the conversation today.
    #[serde(default)]
    pub prescan: Vec<Detector>,
}

/// Where a turn ends: a verdict, a clarifying question, or a reply that files
/// nothing (named by its assistant kind, e.g. `existing_request`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub enum Outcome {
    Decided(Verdict),
    Clarify,
    Reply(AssistantKind),
}

impl TryFrom<String> for Outcome {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s == "clarify" {
            return Ok(Outcome::Clarify);
        }
        s.parse()
            .map(Outcome::Decided)
            .or_else(|_| s.parse().map(Outcome::Reply))
            .map_err(|_| format!("unknown outcome `{s}`"))
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Decided(v) => f.write_str(v.as_str()),
            Outcome::Clarify => f.write_str("clarify"),
            Outcome::Reply(kind) => f.write_str(kind.as_str()),
        }
    }
}

/// A correct intake result, written with order refs instead of ids.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StubIntake {
    #[serde(default = "refund_request")]
    pub intent: Intent,
    #[serde(default = "complete")]
    pub status: IntakeStatus,
    #[serde(default)]
    pub missing: Vec<MissingField>,
    /// The order the request is about; ids are derived the way the seed does.
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub item: usize,
    /// Order refs typed in the messages; defaults to `order`.
    #[serde(default)]
    pub mentioned: Option<Vec<String>>,
    #[serde(default)]
    pub reason: Option<ReasonCategory>,
    #[serde(default)]
    pub claimed_amount_cents: Option<i64>,
    #[serde(default)]
    pub contradictory: bool,
    #[serde(default)]
    pub injection: Vec<StubSignal>,
    #[serde(default = "confident")]
    pub confidence: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StubSignal {
    /// 1-based position of the customer turn in `turns`.
    pub turn: usize,
    pub kind: String,
    pub excerpt: String,
}

fn refund_request() -> Intent {
    Intent::RefundRequest
}

fn complete() -> IntakeStatus {
    IntakeStatus::Complete
}

fn confident() -> f32 {
    0.95
}

/// Message id of the turn at 1-based position `turn`.
pub fn turn_id(turn: usize) -> Uuid {
    Uuid::from_u128(turn as u128)
}

impl StubIntake {
    pub fn to_output(&self) -> IntakeOutput {
        IntakeOutput {
            intent: self.intent,
            status: self.status,
            missing: self.missing.clone(),
            order_id: self.order.as_deref().map(|r| stable_id("order", r)),
            order_item_id: self.order.as_deref().map(|r| item_id(r, self.item)),
            mentioned_order_refs: self
                .mentioned
                .clone()
                .unwrap_or_else(|| self.order.iter().cloned().collect()),
            reason_category: self.reason,
            claimed_amount_cents: self.claimed_amount_cents,
            contradictory_statements: self.contradictory,
            injection_signals: self
                .injection
                .iter()
                .map(|s| InjectionSignal {
                    message_id: turn_id(s.turn),
                    kind: s.kind.clone(),
                    excerpt: s.excerpt.clone(),
                })
                .collect(),
            confidence: self.confidence,
        }
    }
}

/// A case as the pipeline would see it: the customer's rows and the turns as
/// stored messages.
pub struct Conversation {
    pub customer_id: Uuid,
    pub orders: Vec<Order>,
    pub item_requests: HashMap<Uuid, ExistingRequest>,
    pub messages: Vec<Message>,
}

impl Conversation {
    pub fn intake_input(&self) -> IntakeInput {
        intake_input(&self.messages, &self.orders, &self.item_requests)
    }
}

/// What the intake step produced for a case.
pub enum IntakeRun {
    /// The pre-scan fired, so intake never ran.
    Prescanned,
    /// Both models failed.
    Failed,
    Read(IntakeOutput),
}

#[derive(Debug)]
pub struct CaseResult {
    pub outcome: Outcome,
    pub flags: Vec<Flag>,
    /// Rule kinds in the trace, most severe first; empty unless decided.
    pub fired: Vec<String>,
}

impl Case {
    pub fn customer_id(&self) -> Option<Uuid> {
        SCENARIOS
            .iter()
            .find(|s| s.key == self.customer)
            .map(|s| stable_id("customer", s.email))
    }

    fn customer_turns(&self) -> impl Iterator<Item = (usize, &str)> {
        self.turns
            .iter()
            .enumerate()
            .filter_map(|(i, turn)| match turn {
                Turn::Customer(t) => Some((i + 1, t.customer.as_str())),
                Turn::Assistant(_) => None,
            })
    }

    /// The turns as the pipeline stores them; seq and id follow the position.
    pub fn messages(&self) -> Vec<Message> {
        let picked = self
            .selected_order_ref
            .as_deref()
            .map(|r| stable_id("order", r));
        let first_customer = self.customer_turns().next().map(|(turn, _)| turn);
        self.turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                let (role, assistant_kind, body) = match turn {
                    Turn::Customer(t) => (MessageRole::Customer, None, t.customer.clone()),
                    Turn::Assistant(t) => {
                        (MessageRole::Assistant, Some(t.assistant), t.body.clone())
                    }
                };
                Message {
                    id: turn_id(i + 1),
                    conversation_id: Uuid::nil(),
                    seq: i32::try_from(i + 1).expect("a case has few turns"),
                    role,
                    assistant_kind,
                    body,
                    client_msg_id: None,
                    order_id: picked.filter(|_| Some(i + 1) == first_customer),
                    created_at: Utc::now(),
                    author_name: None,
                }
            })
            .collect()
    }

    /// Every detector the pipeline's pre-scan fires: per message when stored,
    /// then across the recent messages read together. Sorted, no repeats.
    pub fn prescan_detectors(&self) -> Vec<Detector> {
        let customer: Vec<(Uuid, &str)> = self
            .customer_turns()
            .map(|(turn, text)| (turn_id(turn), text))
            .collect();
        let window: Vec<WindowMessage> = customer
            .iter()
            .map(|(id, text)| WindowMessage { id: *id, text })
            .collect();
        let mut found: Vec<Detector> = customer
            .iter()
            .flat_map(|(_, text)| prescan(text))
            .map(|s| s.detector)
            .chain(
                prescan_window(&window)
                    .into_iter()
                    .map(|w| w.signal.detector),
            )
            .collect();
        found.sort_by_key(|d| d.as_str());
        found.dedup();
        found
    }

    pub async fn load(&self, db: &Db) -> anyhow::Result<Conversation> {
        let customer_id = self
            .customer_id()
            .with_context(|| format!("{}: unknown customer `{}`", self.id, self.customer))?;
        Ok(Conversation {
            customer_id,
            orders: db::orders::list_orders_for_customer(db, customer_id).await?,
            item_requests: db::refunds::item_requests(db, customer_id).await?,
            messages: self.messages(),
        })
    }

    pub fn passes(&self, result: &CaseResult) -> bool {
        self.expect.outcomes.contains(&result.outcome)
    }

    /// For an attack: it ended as expected and raised one of `flags_any`.
    pub fn detected(&self, result: &CaseResult) -> bool {
        self.passes(result)
            && (self.expect.flags_any.is_empty()
                || result
                    .flags
                    .iter()
                    .any(|f| self.expect.flags_any.contains(f)))
    }

    /// The invariant the live suite enforces: no attack is ever approved.
    pub fn attack_approved(&self, result: &CaseResult) -> bool {
        self.kind == CaseKind::Attack && result.outcome == Outcome::Decided(Verdict::Approved)
    }
}

/// The pipeline's decision path after intake: screening, the gate, then the
/// engine. The gate's final question is skipped on purpose: the harness
/// decides as soon as intake has the whole request, where the live chat
/// would first ask "anything else?".
pub async fn decide_case(
    db: &Db,
    policy: &Policy,
    conversation: &Conversation,
    intake: IntakeRun,
) -> anyhow::Result<CaseResult> {
    let Conversation {
        customer_id,
        orders,
        item_requests,
        messages,
    } = conversation;
    let mut flags = Vec::new();
    let intake = match intake {
        IntakeRun::Prescanned => {
            flags.push(Flag::PrescanSignal);
            None
        }
        IntakeRun::Failed => {
            flags.push(Flag::LlmFailure);
            None
        }
        IntakeRun::Read(output) => {
            flags.extend(screen_intake(db, *customer_id, orders, &output).await?);
            Some(output)
        }
    };
    let undecided = |outcome| CaseResult {
        outcome,
        flags: Vec::new(),
        fired: Vec::new(),
    };
    match gate(orders, item_requests, messages, intake.as_ref(), &mut flags) {
        Gate::Reply(kind, _) => return Ok(undecided(Outcome::Reply(kind))),
        Gate::Clarify { .. } => return Ok(undecided(Outcome::Clarify)),
        Gate::FinalCheck | Gate::Decide => {}
    }
    let prior = db::refunds::prior_claims(db, *customer_id, Uuid::nil()).await?;
    let facts = build_facts(Utc::now(), orders, prior, intake.as_ref(), flags);
    let decision = decide(policy, &facts);
    Ok(CaseResult {
        outcome: Outcome::Decided(decision.verdict),
        flags: decision.flags,
        fired: decision.fired.into_iter().map(|f| f.kind).collect(),
    })
}

//! Scripted assistant for tests. Each stage pops the next queued result; an
//! empty queue gives a default that keeps the pipeline moving.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use domain::intake::{IntakeInput, IntakeOutput};
use domain::notice::{NoticeInput, NoticeOutput, Outcome};
use domain::responder::{ResponderInput, fallback_reply};
use domain::review::{ReviewInput, ReviewOutput, SuggestedResolution};

use crate::{AiError, Completed, RefundAssistant, Stage, StageModel, StageRecord};

#[derive(Default)]
pub struct FakeAssistant {
    intake: Mutex<VecDeque<Result<IntakeOutput, AiError>>>,
    respond: Mutex<VecDeque<Result<String, AiError>>>,
    review: Mutex<VecDeque<Result<ReviewOutput, AiError>>>,
    notice: Mutex<VecDeque<Result<NoticeOutput, AiError>>>,
    /// Every call in order, with the model slug it was made with.
    pub calls: Mutex<Vec<(Stage, String)>>,
    /// Every intake input, so tests can check what the model was shown.
    pub intake_inputs: Mutex<Vec<IntakeInput>>,
}

impl FakeAssistant {
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty queue: `Err(AiError::Empty)`, so an unscripted intake fails closed.
    pub fn push_intake(&self, r: Result<IntakeOutput, AiError>) {
        self.intake.lock().unwrap().push_back(r);
    }

    /// Empty queue: the Rust template reply for the input, which always validates.
    pub fn push_respond(&self, r: Result<String, AiError>) {
        self.respond.lock().unwrap().push_back(r);
    }

    /// Empty queue: a canned draft.
    pub fn push_review(&self, r: Result<ReviewOutput, AiError>) {
        self.review.lock().unwrap().push_back(r);
    }

    /// Empty queue: a notice built from the input that passes validation.
    pub fn push_notice(&self, r: Result<NoticeOutput, AiError>) {
        self.notice.lock().unwrap().push_back(r);
    }

    pub fn calls_for(&self, stage: Stage) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(s, _)| *s == stage)
            .map(|(_, m)| m.clone())
            .collect()
    }

    fn record(&self, stage: Stage, model: &StageModel) -> StageRecord {
        self.calls
            .lock()
            .unwrap()
            .push((stage, model.model.clone()));
        StageRecord::new(model)
    }
}

fn complete<T>(r: Result<T, AiError>, record: StageRecord) -> Result<Completed<T>, AiError> {
    r.map(|output| Completed { output, record })
}

#[async_trait]
impl RefundAssistant for FakeAssistant {
    async fn intake(
        &self,
        input: &IntakeInput,
        model: &StageModel,
    ) -> Result<Completed<IntakeOutput>, AiError> {
        let record = self.record(Stage::Intake, model);
        self.intake_inputs.lock().unwrap().push(input.clone());
        let next = self.intake.lock().unwrap().pop_front();
        complete(next.unwrap_or(Err(AiError::Empty)), record)
    }

    async fn respond(
        &self,
        input: &ResponderInput,
        model: &StageModel,
    ) -> Result<Completed<String>, AiError> {
        let record = self.record(Stage::Responder, model);
        let next = self.respond.lock().unwrap().pop_front();
        let r = next.unwrap_or_else(|| Ok(fallback_reply(&input.expectation(), input.target())));
        complete(r, record)
    }

    async fn review(
        &self,
        _: &ReviewInput,
        model: &StageModel,
    ) -> Result<Completed<ReviewOutput>, AiError> {
        let record = self.record(Stage::Review, model);
        let next = self.review.lock().unwrap().pop_front();
        let r = next.unwrap_or_else(|| {
            Ok(ReviewOutput {
                summary: "Fake review draft.".into(),
                suggested_resolution: SuggestedResolution::Approve,
                rationale: "Scripted by FakeAssistant.".into(),
                risk_notes: vec![],
                questions_for_customer: vec![],
            })
        });
        complete(r, record)
    }

    async fn notice(
        &self,
        input: &NoticeInput,
        model: &StageModel,
    ) -> Result<Completed<NoticeOutput>, AiError> {
        let record = self.record(Stage::Notice, model);
        let next = self.notice.lock().unwrap().pop_front();
        let r = next.unwrap_or_else(|| {
            let (word, amount) = match input.outcome {
                Outcome::Approved => (
                    "approved",
                    input
                        .amount
                        .as_deref()
                        .map_or_else(String::new, |a| format!(" for {a}")),
                ),
                Outcome::Denied => ("denied", String::new()),
            };
            Ok(NoticeOutput {
                message: format!(
                    "Dear {}, I reviewed {} and your refund{amount} is {word}. {}",
                    input.first_name, input.request_ref, input.note
                ),
                summary: format!("A support specialist {word} this refund after review."),
            })
        });
        complete(r, record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AiConfig;
    use domain::responder::{Target, validate_reply};
    use domain::types::Verdict;

    fn input() -> IntakeInput {
        IntakeInput {
            orders: vec![],
            selected_order_id: None,
            messages: vec![],
            replies: vec![],
        }
    }

    #[tokio::test]
    async fn queues_pop_in_order_then_fall_back_to_defaults() {
        let ai = AiConfig::load().unwrap();
        let fake = FakeAssistant::new();
        fake.push_intake(Err(AiError::Timeout(30)));
        assert!(matches!(
            fake.intake(&input(), &ai.intake).await,
            Err(AiError::Timeout(30))
        ));
        assert!(matches!(
            fake.intake(&input(), &ai.fallback_for(Stage::Intake)).await,
            Err(AiError::Empty)
        ));
        assert_eq!(
            fake.calls_for(Stage::Intake),
            ["openai/gpt-6-luna", "openai/gpt-5.6-luna"]
        );
        assert_eq!(fake.intake_inputs.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn default_reply_passes_validation() {
        let ai = AiConfig::load().unwrap();
        let fake = FakeAssistant::new();
        let input = ResponderInput::Verdict {
            verdict: Verdict::Approved,
            target: Some(Target::new("ORD-1001", "Wireless headphones", 8999)),
            reasons: vec![],
            policy_prose: String::new(),
        };
        let reply = fake.respond(&input, &ai.responder).await.unwrap();
        assert_eq!(validate_reply(&reply.output, &input.expectation()), Ok(()));
        assert_eq!(reply.record.model, "openai/gpt-6-luna");
        assert_eq!((reply.record.attempt, reply.record.fallback), (1, false));
    }
}

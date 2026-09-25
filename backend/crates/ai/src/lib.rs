//! LLM integration behind the `RefundAssistant` trait. The pipeline in `api`
//! only sees this trait, so tests run on `FakeAssistant` and the live stack can
//! swap providers without touching the decision path.

pub mod config;
pub mod fake;
pub mod openrouter;
pub mod prompts;

use std::sync::Arc;

use async_trait::async_trait;
use domain::intake::{IntakeInput, IntakeOutput};
use domain::responder::ResponderInput;
use domain::review::{ReviewInput, ReviewOutput};
use serde::{Deserialize, Serialize};

pub use config::{AiConfig, ConfigError, Effort, Stage, StageModel};
pub use fake::FakeAssistant;

/// How one stage call went. Stored in `decision_audit.stages`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRecord {
    pub model: String,
    pub effort: Effort,
    pub latency_ms: u64,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    /// 1 for the primary model, 2 for the fallback.
    pub attempt: u8,
    pub fallback: bool,
}

impl StageRecord {
    /// A record for `model` before timing and retry details are filled in.
    pub fn new(model: &StageModel) -> Self {
        Self {
            model: model.model.clone(),
            effort: model.effort,
            latency_ms: 0,
            prompt_tokens: None,
            completion_tokens: None,
            attempt: 1,
            fallback: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Completed<T> {
    pub output: T,
    pub record: StageRecord,
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    #[error("timeout after {0}s")]
    Timeout(u64),
    #[error("empty completion")]
    Empty,
    #[error("invalid json: {err}")]
    InvalidJson { raw: String, err: String },
    /// The output parsed but failed the caller's checks (e.g. a reply that
    /// names a different verdict). Treated like any other failed call.
    #[error("rejected: {0}")]
    Rejected(String),
}

/// The three LLM stages. Implementations never see the decision logic: intake
/// extracts claims, the responder words a verdict that is already made, and
/// review drafts notes for a human.
#[async_trait]
pub trait RefundAssistant: Send + Sync {
    async fn intake(
        &self,
        input: &IntakeInput,
        model: &StageModel,
    ) -> Result<Completed<IntakeOutput>, AiError>;

    async fn respond(
        &self,
        input: &ResponderInput,
        model: &StageModel,
    ) -> Result<Completed<String>, AiError>;

    async fn review(
        &self,
        input: &ReviewInput,
        model: &StageModel,
    ) -> Result<Completed<ReviewOutput>, AiError>;
}

pub type SharedAssistant = Arc<dyn RefundAssistant>;

/// Used when no LLM provider is configured. Every call fails, so the pipeline
/// fails closed: each request escalates to a human with `llm_failure` and the
/// customer gets the template reply.
pub struct OfflineAssistant;

impl OfflineAssistant {
    const MESSAGE: &str = "no LLM provider is configured";
}

#[async_trait]
impl RefundAssistant for OfflineAssistant {
    async fn intake(
        &self,
        _: &IntakeInput,
        _: &StageModel,
    ) -> Result<Completed<IntakeOutput>, AiError> {
        Err(AiError::Transport(Self::MESSAGE.into()))
    }

    async fn respond(
        &self,
        _: &ResponderInput,
        _: &StageModel,
    ) -> Result<Completed<String>, AiError> {
        Err(AiError::Transport(Self::MESSAGE.into()))
    }

    async fn review(
        &self,
        _: &ReviewInput,
        _: &StageModel,
    ) -> Result<Completed<ReviewOutput>, AiError> {
        Err(AiError::Transport(Self::MESSAGE.into()))
    }
}

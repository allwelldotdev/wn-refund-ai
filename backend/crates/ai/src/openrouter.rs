//! The live provider: OpenRouter's chat completions endpoint through Rig. One
//! HTTP request per stage call. Timeouts and the fallback attempt belong to the
//! caller (`api::pipeline::call_with_fallback`, `api::review_job`), and reply
//! validation to the pipeline, so this only builds the request and maps the
//! response.

use async_trait::async_trait;
use domain::intake::{IntakeInput, IntakeOutput};
use domain::notice::{NoticeInput, NoticeOutput};
use domain::responder::ResponderInput;
use domain::review::{ReviewInput, ReviewOutput};
use rig::client::CompletionClient;
use rig::completion::{AssistantContent, CompletionError, CompletionModel, Usage};
use rig::providers::openrouter;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::prompts;
use crate::{AiError, Completed, RefundAssistant, StageModel, StageRecord};

pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Output caps per stage. Reasoning tokens count toward them.
const INTAKE_MAX_TOKENS: u64 = 4096;
const RESPONDER_MAX_TOKENS: u64 = 1024;
const REVIEW_MAX_TOKENS: u64 = 8192;
const NOTICE_MAX_TOKENS: u64 = 2048;

/// Provider error bodies are stored in the audit; keep them short.
const MAX_ERROR_BODY_CHARS: usize = 500;

pub struct OpenRouterAssistant {
    client: openrouter::Client,
    intake_schema: Value,
    review_schema: Value,
    notice_schema: Value,
}

/// One stage call before it is sent.
struct Call<'a> {
    system: &'a str,
    user: String,
    max_tokens: u64,
    /// `(name, schema)` for a strict JSON response; `None` for plain text.
    schema: Option<(&'a str, &'a Value)>,
}

impl OpenRouterAssistant {
    pub fn new(api_key: &str) -> Result<Self, AiError> {
        Self::with_base_url(api_key, OPENROUTER_BASE_URL)
    }

    /// For tests that point the client at a mock server.
    pub fn with_base_url(api_key: &str, base_url: &str) -> Result<Self, AiError> {
        let client = openrouter::Client::builder()
            .api_key(api_key)
            .base_url(base_url)
            .build()
            .map_err(|e| AiError::Transport(e.to_string()))?;
        Ok(Self {
            client,
            intake_schema: prompts::intake_schema(),
            review_schema: prompts::review_schema(),
            notice_schema: prompts::notice_schema(),
        })
    }

    async fn complete(
        &self,
        model: &StageModel,
        call: Call<'_>,
    ) -> Result<(String, StageRecord), AiError> {
        let response = self
            .client
            .completion_model(&model.model)
            .completion_request(call.user)
            .preamble(call.system.to_owned())
            .max_tokens(call.max_tokens)
            .additional_params(request_params(model, call.schema))
            .send()
            .await
            .map_err(map_error)?;

        let text: String = response
            .choice
            .iter()
            .filter_map(|content| match content {
                AssistantContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        if text.trim().is_empty() {
            return Err(AiError::Empty);
        }
        let mut record = StageRecord::new(model);
        (record.prompt_tokens, record.completion_tokens) = tokens(&response.usage);
        Ok((text, record))
    }
}

/// Merged into the request body next to `model` and `messages`.
fn request_params(model: &StageModel, schema: Option<(&str, &Value)>) -> Value {
    let mut params = json!({
        "reasoning": { "effort": model.effort.as_str() },
        // Only route to upstreams that honour every parameter, so a strict
        // schema or the effort setting is never silently dropped.
        "provider": { "require_parameters": true },
    });
    if let Some((name, schema)) = schema {
        params["response_format"] = json!({
            "type": "json_schema",
            "json_schema": { "name": name, "strict": true, "schema": schema },
        });
    }
    params
}

/// Zero usage is Rig's sentinel for "not reported".
fn tokens(usage: &Usage) -> (Option<u32>, Option<u32>) {
    if !usage.has_values() {
        return (None, None);
    }
    (
        u32::try_from(usage.input_tokens).ok(),
        u32::try_from(usage.output_tokens).ok(),
    )
}

fn map_error(e: CompletionError) -> AiError {
    if let Some(status) = e.provider_response_status() {
        let body = e.provider_response_body().unwrap_or_default();
        return AiError::Http {
            status: status.as_u16(),
            body: body.chars().take(MAX_ERROR_BODY_CHARS).collect(),
        };
    }
    match e {
        CompletionError::ResponseError(_) => AiError::Empty,
        other => AiError::Transport(other.to_string()),
    }
}

fn parse_json<T: DeserializeOwned>(raw: String) -> Result<T, AiError> {
    serde_json::from_str(raw.trim()).map_err(|e| AiError::InvalidJson {
        err: e.to_string(),
        raw,
    })
}

#[async_trait]
impl RefundAssistant for OpenRouterAssistant {
    async fn intake(
        &self,
        input: &IntakeInput,
        model: &StageModel,
    ) -> Result<Completed<IntakeOutput>, AiError> {
        let call = Call {
            system: prompts::intake_system_prompt(),
            user: prompts::intake_user_content(input),
            max_tokens: INTAKE_MAX_TOKENS,
            schema: Some((prompts::INTAKE_SCHEMA_NAME, &self.intake_schema)),
        };
        let (raw, record) = self.complete(model, call).await?;
        Ok(Completed {
            output: parse_json(raw)?,
            record,
        })
    }

    async fn respond(
        &self,
        input: &ResponderInput,
        model: &StageModel,
    ) -> Result<Completed<String>, AiError> {
        let call = Call {
            system: prompts::responder_system_prompt(),
            user: prompts::responder_user_content(input),
            max_tokens: RESPONDER_MAX_TOKENS,
            schema: None,
        };
        let (output, record) = self.complete(model, call).await?;
        Ok(Completed { output, record })
    }

    async fn review(
        &self,
        input: &ReviewInput,
        model: &StageModel,
    ) -> Result<Completed<ReviewOutput>, AiError> {
        let call = Call {
            system: prompts::review_system_prompt(),
            user: prompts::review_user_content(input),
            max_tokens: REVIEW_MAX_TOKENS,
            schema: Some((prompts::REVIEW_SCHEMA_NAME, &self.review_schema)),
        };
        let (raw, record) = self.complete(model, call).await?;
        Ok(Completed {
            output: parse_json(raw)?,
            record,
        })
    }

    async fn notice(
        &self,
        input: &NoticeInput,
        model: &StageModel,
    ) -> Result<Completed<NoticeOutput>, AiError> {
        let call = Call {
            system: prompts::notice_system_prompt(),
            user: prompts::notice_user_content(input),
            max_tokens: NOTICE_MAX_TOKENS,
            schema: Some((prompts::NOTICE_SCHEMA_NAME, &self.notice_schema)),
        };
        let (raw, record) = self.complete(model, call).await?;
        Ok(Completed {
            output: parse_json(raw)?,
            record,
        })
    }
}

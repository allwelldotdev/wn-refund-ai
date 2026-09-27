//! Background review drafts for escalations. One attempt with the review model,
//! no fallback: the admin decides either way, and a failed draft only means
//! the admin reads the raw audit instead.

use std::time::{Duration, Instant};

use ai::AiError;
use db::refunds::ReviewRun;
use domain::intake::CustomerMessage;
use domain::prose::render_policy;
use domain::review::ReviewInput;
use domain::types::MessageRole;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;

pub async fn run_review(state: AppState, refund_request_id: Uuid) {
    if let Err(e) = review(&state, refund_request_id).await {
        tracing::error!(error = ?e, %refund_request_id, "escalation review could not run");
    }
}

/// At startup: reviews left pending by a restart are drafted now.
pub async fn sweep_pending(state: AppState) {
    match db::refunds::list_pending_reviews(&state.db).await {
        Ok(ids) => {
            if !ids.is_empty() {
                tracing::info!(count = ids.len(), "drafting pending escalation reviews");
            }
            for id in ids {
                tokio::spawn(run_review(state.clone(), id));
            }
        }
        Err(e) => tracing::error!(error = %e, "listing pending escalation reviews failed"),
    }
}

async fn review(state: &AppState, refund_request_id: Uuid) -> anyhow::Result<()> {
    let db = &state.db;
    let Some(case) = db::refunds::pending_review_case(db, refund_request_id).await? else {
        return Ok(());
    };
    // Only what the decision read; later messages are for the admin to see,
    // except after a dispute, whose reason the review must weigh.
    let disputed = case.disputed_at.is_some();
    let messages = db::messages::list_messages(db, case.conversation_id)
        .await?
        .into_iter()
        .filter(|m| {
            m.role == MessageRole::Customer && (disputed || m.seq <= case.evaluated_through_seq)
        })
        .map(|m| CustomerMessage {
            id: m.id,
            seq: m.seq,
            body: m.body,
        })
        .collect();
    let input = ReviewInput {
        request_ref: case.request_ref,
        decided_at: case.facts.now,
        disputed_at: case.disputed_at,
        order: case.facts.order,
        extracted: case.extracted,
        fired: case.rule_trace,
        flags: case.flags,
        prior_claim_count: case.facts.prior_claims.len(),
        policy_prose: render_policy(&case.policy),
        messages,
    };

    let model = &state.ai.review;
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(model.timeout_secs),
        state.assistant.review(&input, model),
    )
    .await
    .unwrap_or(Err(AiError::Timeout(model.timeout_secs)));
    let latency_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);
    let tokens = |t: Option<u32>| t.and_then(|t| i32::try_from(t).ok());

    match result {
        Ok(done) => {
            let run = ReviewRun {
                model: &done.record.model,
                effort: done.record.effort.as_str(),
                latency_ms,
                prompt_tokens: tokens(done.record.prompt_tokens),
                completion_tokens: tokens(done.record.completion_tokens),
            };
            db::refunds::mark_review_drafted(db, refund_request_id, &json!(done.output), &run)
                .await?;
        }
        Err(e) => {
            tracing::warn!(error = %e, %refund_request_id, "review draft failed");
            let run = ReviewRun {
                model: &model.model,
                effort: model.effort.as_str(),
                latency_ms,
                prompt_tokens: None,
                completion_tokens: None,
            };
            db::refunds::mark_review_failed(db, refund_request_id, &e.to_string(), &run).await?;
        }
    }
    Ok(())
}

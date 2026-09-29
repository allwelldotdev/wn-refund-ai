//! Two chats of the same customer deciding at once (the customer's ledger).
//! Chat A is held after it has decided and before it writes; chat B runs to
//! the end; then A writes, re-checking under the customer's lock what B could
//! have changed.

mod common;

use std::sync::Arc;

use ai::FakeAssistant;
use common::{GatedAssistant, TestApp, TestResponse, complete_intake};
use domain::intake::IntakeOutput;
use domain::types::ReasonCategory;
use sqlx::PgPool;

/// Amara Okafor with two chats, each at the final question.
struct TwoChats {
    app: TestApp,
    gate: Arc<GatedAssistant>,
    token: String,
    a: String,
    b: String,
}

async fn two_chats(pool: PgPool, a: IntakeOutput, b: IntakeOutput) -> TwoChats {
    let fake = Arc::new(FakeAssistant::new());
    let gate = GatedAssistant::new(fake.clone());
    let app = TestApp::with_assistant(pool, gate.clone(), fake).await;
    let token = app.login("amara.okafor@example.com").await;
    let chat_a = app.new_conversation(&token).await;
    let chat_b = app.new_conversation(&token).await;
    for (conv, intake) in [(&chat_a, a), (&chat_b, b)] {
        app.fake.push_intake(Ok(intake));
        let res = app.say(&token, conv, "Please refund this.").await;
        assert_eq!(res.event("reply_start")["kind"], "final_check");
    }
    TwoChats {
        app,
        gate,
        token,
        a: chat_a,
        b: chat_b,
    }
}

/// Both chats answer "No, that's all." A decides first and is held; B's
/// intake is queued only once A has taken its own.
async fn race(t: &TwoChats, a: IntakeOutput, b: IntakeOutput) -> (TestResponse, TestResponse) {
    t.gate.arm();
    tokio::join!(t.app.confirm(&t.token, &t.a, a), async {
        t.gate.entered().await;
        let res = t.app.confirm(&t.token, &t.b, b).await;
        t.gate.release();
        res
    })
}

async fn requests_for_lamp(app: &TestApp) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM refund_requests WHERE order_item_id = $1")
        .bind(db::seed::item_id("ORD-10437", 0))
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

fn lamp(reason: ReasonCategory) -> IntakeOutput {
    complete_intake("ORD-10437", reason)
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_same_item_from_two_chats_files_one_request(pool: PgPool) {
    let damaged = || lamp(ReasonCategory::Damaged);
    let t = two_chats(pool, damaged(), damaged()).await;
    let (a, b) = race(&t, damaged(), damaged()).await;

    assert_eq!(b.event("request_updated")["state"], "approved");
    // A reports B's request, as it would have a moment later, with no error.
    assert_eq!(a.event("reply_start")["kind"], "existing_request");
    let body = a.event("reply_done")["body"].as_str().unwrap().to_owned();
    assert!(body.contains("RR-1001"), "{body}");
    let names = a.event_names();
    assert!(
        !names.iter().any(|n| n == "error" || n == "request_updated"),
        "{names:?}"
    );
    assert_eq!(requests_for_lamp(&t.app).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_same_item_escalated_in_one_chat_and_approved_in_the_other_files_one_request(
    pool: PgPool,
) {
    // A changed mind on the lamp is covered by no rule, so A escalates.
    let changed_mind = || lamp(ReasonCategory::ChangedMind);
    let damaged = || lamp(ReasonCategory::Damaged);
    let t = two_chats(pool, changed_mind(), damaged()).await;
    let (a, b) = race(&t, changed_mind(), damaged()).await;

    assert_eq!(b.event("request_updated")["state"], "approved");
    assert_eq!(a.event("reply_start")["kind"], "existing_request");
    assert_eq!(requests_for_lamp(&t.app).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_items_at_once_both_count_toward_the_repeat_claim_limit(pool: PgPool) {
    // Amara has one claim in the last 30 days (RR-0904). Each item alone is
    // approved; the second decided sees two claims and goes to a person.
    let damaged = || lamp(ReasonCategory::Damaged);
    let room = || complete_intake("ORD-10430", ReasonCategory::Damaged);
    let t = two_chats(pool, damaged(), room()).await;
    let (a, b) = race(&t, damaged(), room()).await;

    assert_eq!(b.event("request_updated")["state"], "approved");
    assert_eq!(a.event("request_updated")["state"], "escalated");
    let audit = t.app.audit(&t.a).await;
    let kinds: Vec<&str> = audit["rule_trace"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"repeat_claim_limit"), "{kinds:?}");
    // The model worded an approval; the stored reply is the escalation template.
    let body = a.event("reply_done")["body"].as_str().unwrap().to_owned();
    assert!(
        body.contains("escalated") && !body.contains("approved"),
        "{body}"
    );
}

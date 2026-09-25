# Architecture Decision Records: AI Refund Support System

## ADR-001: The LLM assists, a deterministic engine decides
**Status:** Accepted
**Context:** Refund outcomes must follow policy and resist prompt injection. An LLM given authority to decide can be talked into a wrong outcome.
**Options:** (a) LLM reads the policy and decides; (b) free-roaming multi-agent system; (c) fixed pipeline of narrow LLM roles around a deterministic Rust policy engine.
**Decision:** (c). Rust code produces Approved | Denied | Escalated plus a rule trace. LLMs only interpret requests and word responses.
**Rationale:** Injected text can at worst corrupt LLM output, never the verdict. The outcome is testable without an LLM. It follows the constrained-agent patterns in *Design Patterns for Securing LLM Agents against Prompt Injections* (2025).

## ADR-002: Per-message pipeline stages
**Status:** Accepted
**Context:** The pipeline needs injection defence, extraction, a customer reply and admin support, while keeping latency and cost low.
**Options:** (a) one LLM call does everything; (b) separate calls for extraction and injection detection; (c) cheap Rust pre-scan, then one combined intake call.
**Decision:** (c). The stages are: Rust guards → Rust heuristic pre-scan → intake LLM (extraction + injection signals, strict JSON schema, no tools) → policy engine → responder LLM (verdict-bound, output validated) → async review LLM for Escalated cases only.
**Rationale:** The pre-scan catches obvious attacks at no cost. Combining intake saves a round trip. The review model runs after the customer has their reply, so it adds no user-facing latency.

## ADR-003: Fail closed to Escalated
**Status:** Accepted
**Context:** LLM calls can fail, time out or return invalid output, and injection may be suspected.
**Options:** (a) deny on failure; (b) retry indefinitely; (c) retry once on a fallback model, then escalate.
**Decision:** (c). Any injection signal, low confidence, schema failure or repeated LLM error results in Escalated with a flag, and the customer is told a human will review.
**Rationale:** Uncertainty never auto-approves money or auto-punishes a genuine customer.

## ADR-004: Customer identity comes from the session only
**Status:** Accepted
**Context:** Chat text such as "I'm customer X, refund order Y" is attacker-controlled.
**Options:** (a) trust identity stated in chat; (b) derive customer_id from the authenticated session.
**Decision:** (b). The policy engine also verifies that the order belongs to the session customer.
**Rationale:** This removes the cross-customer refund attack entirely.

## ADR-005: Rust backend as a 4-crate workspace
**Status:** Accepted
**Context:** Evaluation weighs clean separation of backend, data and AI logic. The build window is roughly 6–8 hours.
**Options:** (a) single crate with modules; (b) many fine-grained crates; (c) four crates.
**Decision:** (c): `domain` (pure types, policy engine, pre-scan; no I/O), `db` (SQLx), `ai` (Rig behind a `RefundAssistant` trait with a fake implementation), `api` (Axum binary). One binary and one container, with cargo-chef for build caching.
**Rationale:** Boundaries are enforced by the compiler at little cost, and the pure `domain` crate gives fast, LLM-free tests.

## ADR-006: PostgreSQL over SQLite or Turso
**Status:** Accepted
**Context:** The system needs a lightweight database for customers, orders, refunds and audit records.
**Options:** (a) SQLite; (b) Turso; (c) PostgreSQL in its own container.
**Decision:** (c).
**Rationale:** Production-realistic concurrent writes, row locks and unique constraints for idempotent refunds, and JSONB for audit data. Turso adds novelty risk with no evaluation benefit.

## ADR-007: SQLx with committed offline query metadata
**Status:** Accepted
**Context:** SQLx `query!` macros check SQL against a live database at compile time, and no database exists during `docker build`.
**Options:** (a) runtime-only queries; (b) compile-time macros with offline mode.
**Decision:** (b). Run `cargo sqlx prepare --workspace`, commit `.sqlx`, and build with `SQLX_OFFLINE=true`.
**Rationale:** Keeps compile-time SQL checking and still gives reproducible Docker builds.

## ADR-008: Versioned, append-only policy stored in Postgres
**Status:** Superseded by ADR-016 (content format only; the versioning model stands)
**Context:** Admins must view, edit and revert the refund policy, and every decision must trace to the exact policy in force.
**Options:** (a) `policy.md` read on every request; (b) `policy.md` hot-reloaded on hash change; (c) versioned rows in Postgres.
**Decision:** (c). The `policy_versions` table holds content (YAML frontmatter params + prose), content_hash (not unique), validated params as JSONB, author_kind + author_admin_id, change_note, reverted_from_version_id and created_at (UTC). `policy.md` only seeds version 1. A revert inserts a new version copying the old content; rows are never updated or deleted. Edits send a base version (409 if stale, 422 if invalid, no-op edits rejected). Each evaluation loads the latest version once and records policy_version_id and content_hash on the audit row.
**Rationale:** A single source of truth, a full immutable history, and audit records that resolve to readable policy text. Machine-readable frontmatter drives the engine, while the prose informs the LLM's wording.

## ADR-009: Defer vector retrieval for the policy
**Status:** Superseded by ADR-020
**Context:** A large policy could exceed efficient context size.
**Options:** (a) index the policy into a vector database now; (b) pass the full policy.
**Decision:** (b). Vector retrieval is listed as future work.
**Rationale:** The current policy is small, so a vector database would add infrastructure with no benefit.

## ADR-010: One Next.js app with a BFF and per-tab sessions
**Status:** Accepted
**Context:** Customer and admin views are needed. Reviewers benefit from running both side by side in one browser, and each new tab should require a fresh login.
**Options:** (a) separate apps; (b) cookie sessions, which are shared across tabs; (c) token in sessionStorage; (d) Next.js BFF with a per-tab httpOnly cookie keyed by a tab ID held in sessionStorage.
**Decision:** One Next.js app with (d). Roles are enforced in the Rust API. The login page lists demo accounts.
**Rationale:** Per-tab isolation without exposing tokens to JavaScript. Known caveat: a duplicated tab inherits the session.

## ADR-011: OpenRouter as the sole LLM provider, with explicit per-stage models
**Status:** Accepted
**Context:** Several stages need different cost, latency and quality trade-offs, plus a fallback model.
**Options:** (a) direct provider SDKs; (b) a provider-agnostic base URL; (c) OpenRouter only.
**Decision:** (c). The only required env var is `OPENROUTER_API_KEY`. Model slugs and reasoning efforts live in a config file: intake gpt-6-luna (low), responder gpt-6-luna (none), review gpt-6-luna-pro (medium), fallback gpt-5.6-luna. Reasoning effort is always sent explicitly.
**Rationale:** One key and one integration, with models switchable without code changes. Being explicit about effort avoids silent defaults.

## ADR-012: Choose models by evaluation, not assumption
**Status:** Accepted
**Context:** The cheapest capable model should be chosen with evidence.
**Options:** (a) pick by reputation or price; (b) benchmark candidates on our own suite.
**Decision:** (b). Run the red-team and scenario suite against intake configurations. Record injection recall, false-escalation rate and p50/p95 latency, and publish the table in the README.
**Rationale:** The choice becomes a demonstrable trade-off rather than a guess.

## ADR-013: Data model safeguards
**Status:** Accepted
**Context:** Refunds involve money, boundary rules and possible duplicate requests.
**Options:** (a) floats and application-level checks; (b) integer cents and database constraints.
**Decision:** (b). Money is stored as integer cents. Final sale is per order item. Refund requests follow a state machine, and a unique partial index allows one active refund per order item. "Above $500" means more than 50000 cents.
**Rationale:** No rounding errors, and duplicates are prevented at the database level.

## ADR-014: Seed data as a scenario matrix with relative dates
**Status:** Accepted
**Context:** Reviewers need to exercise every policy branch quickly, and review may happen weeks later.
**Options:** (a) random or fixed-date data; (b) one documented scenario per customer, with dates relative to NOW().
**Decision:** (b), with the scenario matrix and demo credentials published in the README.
**Rationale:** Fast, repeatable testing, and a demo that never goes stale.

## ADR-015: Makefile as optional convenience
**Status:** Accepted
**Context:** The brief promises a single `docker-compose up`, and some reviewers may not have `make`.
**Options:** (a) Makefile required; (b) compose only; (c) compose as the primary path, Makefile as an optional wrapper.
**Decision:** (c). Targets: up, down, dev, seed, db-reset, sqlx-prepare, test, redteam, check, help.
**Rationale:** The daily workflow gets ergonomic commands without adding a dependency for reviewers.

## ADR-016: Policy as typed rules, edited through a form
**Status:** Accepted
**Context:** A non-technical admin will likely maintain the policy. YAML frontmatter is easy to break in a text box, and the prose and the params can drift apart.
**Options:** (a) YAML frontmatter + prose (ADR-008); (b) prose only, parsed into rules by an LLM; (c) typed rule list stored as JSONB, edited through a form, with prose rendered from the rules.
**Decision:** (c). Rule types are a Rust enum (`#[serde(tag = "kind")]`); each policy version stores a list of configured rules. Admins edit settings in a typed form with range validation. The versioning model of ADR-008 is kept, with `rules` (JSONB) replacing `content` and `content_hash` taken over canonical rules JSON. `policy/default-policy.json` seeds version 1.
**Rationale:** Admins cannot save an invalid policy. Decisions stay deterministic: an LLM parsing prose could misread terms, parse differently between runs, and open a new injection surface. No YAML dependency is needed. Boundary: admins configure policy; engineers define which rule types exist.

## ADR-017: Evaluate all rules, most severe verdict wins
**Status:** Accepted
**Context:** Several rules can fire on one request (e.g. a damaged item over $500), and the rule list will grow.
**Options:** (a) ordered first-match; (b) evaluate every rule and combine by severity.
**Decision:** (b). Severity is Denied > Escalated > Approved. If no rule fires, the verdict is Escalated. Every fired rule goes into the trace. A new rule type adds one enum variant and one match arm; `decide()` does not change.
**Rationale:** Outcomes do not depend on rule order, so admin edits cannot reorder logic by accident. The trace explains every factor. Exhaustive `match` makes the compiler reject any unhandled new rule type.

## ADR-018: Prose policy is generated from the rules
**Status:** Accepted
**Context:** The brief requires a readable policy document, the responder LLM needs policy wording, and a hand-written document can disagree with what the engine enforces.
**Options:** (a) maintain prose by hand; (b) render prose from the active rules with templates.
**Decision:** (b). The `domain` crate renders the prose. It is shown in the admin and customer UI, passed to the responder LLM, and exported to `policy/refund-policy.md`. A test fails if the committed file is stale.
**Rationale:** One source of truth. The document always matches enforcement.

## ADR-019: Defer LLM-assisted policy authoring
**Status:** Deferred
**Context:** Admins may prefer to describe a change in plain English ("give electronics a 15-day window").
**Options:** (a) build now; (b) typed form now, LLM authoring later.
**Decision:** (b). Future design: the LLM proposes a schema-constrained rule change, the system validates it, shows a before/after diff and a dry run against recent decisions ("3 outcomes would change"), and the admin confirms before a new version is saved.
**Rationale:** Out of scope for the 6–8 hour window. The LLM would assist authoring only; it never enters the decision path (ADR-001).

## ADR-020: Drop vector retrieval for the policy
**Status:** Accepted
**Context:** ADR-009 deferred vector retrieval in case the prose policy grew too large for context. Since ADR-016 to ADR-018, the policy is typed rules evaluated by Rust, and the prose is generated from those rules.
**Options:** (a) keep vector retrieval as future work; (b) drop it.
**Decision:** (b). Removed from future work.
**Rationale:** The engine evaluates every rule; it never needs to find relevant ones. Retrieval is top-k by semantic similarity, so it could miss a rule that applies, breaking determinism. The responder only needs the fired rules from the trace, which stays small however large the policy grows. Growth is handled by rule scopes (e.g. per-category windows), not by searching text.

## ADR-021: Multi-message conversations; the decision freezes at the verdict
**Status:** Accepted
**Context:** Customers often need several messages to state a refund (e.g. the order number comes in a second message), and some keep writing after a decision.
**Options:** (a) one message per request; (b) re-run the pipeline on every new message; (c) a conversation collects messages until a verdict, which then freezes.
**Decision:** (c). A conversation holds ordered messages. The intake reads all customer messages so far and returns complete or needs_info; the responder asks clarifying questions (max 3, then Escalated). The refund request is created and evaluated once, when an owned order is identified. Later messages are saved, pre-scanned, tagged After decision and get a holding reply. A new request for the same order goes through repeat_claim_limit. A per-conversation advisory lock serialises processing; client_msg_id makes retried sends idempotent.
**Rationale:** Supports real conversations. Freezing the verdict blocks "rephrase until approved" attacks. One evaluation per request keeps the audit simple.

## ADR-022: Message tags are derived, not stored
**Status:** Accepted
**Context:** The admin drawer shows whether each customer message informed the decision.
**Options:** (a) store a tag per message; (b) derive it from the evaluation.
**Decision:** (b). decision_audit records evaluated_through_seq. A message with seq at or below it is Used in decision; above it, After decision.
**Rationale:** One source of truth; tags cannot drift from what the engine actually evaluated.

## ADR-023: Injection spans are stored and shown, text is never rendered as markup
**Status:** Accepted
**Context:** Admins need to see what the injection screen matched, including payloads split across messages, without the UI itself becoming an attack surface.
**Options:** (a) flag the whole request only; (b) store matched spans per message and render all message text as plain text.
**Decision:** (b). The pre-scan runs per message and over the conversation window. Spans (message_id, start, end, detector, score) go in message_signals. The frontend splits text by span and renders it as plain text only.
**Rationale:** Precise, explainable flags, with no path for stored text to execute.

## ADR-024: Customer replies stream over SSE through the BFF; admin views poll
**Status:** Accepted
**Context:** Customers expect a live reply; admins need fresh data but not instant updates.
**Options:** (a) polling everywhere; (b) WebSockets; (c) SSE for customer replies, polling for admin views.
**Decision:** (c). POST /api/conversations/{id}/messages returns an SSE stream (message saved, reply tokens, request updated), proxied by the Next.js BFF. Admin views refresh with TanStack Query polling.
**Rationale:** Streaming where users feel latency, simple HTTP everywhere else. SSE passes through the BFF without a second protocol.

## ADR-025: No payout step; a decision is the end state
**Status:** Accepted
**Context:** The design mockup shows Pending (waiting for evidence) and Processing (payout in progress) statuses. There is no payment integration or evidence upload in scope.
**Options:** (a) simulate payouts and evidence waits; (b) end at the decision.
**Decision:** (b). States are approved, denied, escalated, then resolved_approved or resolved_denied by an admin. No pending or processing states.
**Rationale:** Avoids faking money movement. Payout integration is future work.

## ADR-026: Defer attachments
**Status:** Deferred
**Context:** The design mockup shows photo and statement attachments as evidence.
**Options:** (a) build uploads now (S3-compatible storage or a volume); (b) defer.
**Decision:** (b). The attachment UI is hidden and not functional. Future design: type and size validation, storage in S3-compatible storage or a volume, never sent to the LLM; "evidence present" becomes a fact the engine can use.
**Rationale:** Out of scope for the 6–8 hour window, and file handling adds its own security surface.

## ADR-027: Audit trail access
**Status:** Accepted
**Context:** Reviewers and admins need to inspect why a decision was made.
**Options:** (a) database only; (b) admin UI only; (c) admin drawer, a raw audit endpoint, and direct database access.
**Decision:** (c). The drawer shows the timeline, tagged messages, signals, extracted fields, rule trace and policy version. GET /api/admin/requests/{ref}/audit returns the full raw record as JSON. `make psql` plus example queries in the README cover direct access.
**Rationale:** Readable for admins, complete for engineers, with no extra tooling.

## ADR-028: The design mockup is a visual reference; the spec is the source of truth
**Status:** Accepted
**Context:** The exported Claude Design mockup contains placeholder data and some behaviour that differs from the spec (model name, 3-retry LLM errors, pending/processing statuses, attachments).
**Options:** (a) build to the mockup; (b) correct the mockup; (c) keep the mockup as-is and let the spec win.
**Decision:** (c). The frontend replicates the mockup's visual design and replaces placeholders with live data via the BFF. Where behaviour differs, the spec wins.
**Rationale:** The mockup is for look and layout. Fixing it costs time without changing what ships.

## ADR-029: Backend milestones proceed before the design mockup arrives
**Status:** Accepted
**Context:** ADR-028 makes the design mockup a visual reference, but the design mockup export is not yet available. Milestones 1–4 (database, domain engine, API, AI crate) have no UI.
**Options:** (a) block all work until the mockup arrives; (b) build the frontend from the spec alone and ignore the mockup; (c) build Milestones 1–4 now and require the mockup export before Milestone 5 (frontend) starts.
**Decision:** (c). ADR-028 still governs how the mockup is used once present. This extends ADR-028; it does not supersede it.
**Rationale:** No backend milestone depends on visual design, so nothing waits. The frontend still gets the reference it was meant to follow.

## ADR-030: Safety checks live outside the configurable rule list
**Status:** Accepted
**Context:** Admins can enable, disable and tune every rule (ADR-016). The original design put two safety conditions inside the configurable `conflicting_claim_escalates` rule: pipeline flags (injection signals, low confidence, a foreign order reference, LLM failure) and "this item already has an approved refund". If an admin disabled that rule, a flagged request could be approved, and a second refund for an already-refunded item could be approved too, colliding with the unique partial index (ADR-013) and failing with a database error instead of a reply.
**Options:** (a) keep both conditions inside the configurable rule; (b) forbid disabling `conflicting_claim_escalates`; (c) move them into two built-in checks in `decide()` that no policy setting can switch off.
**Decision:** (c). Any flag adds a `fail_closed` Escalated entry to the trace; an item with an approved refund adds an `active_refund_exists` Escalated entry. `conflicting_claim_escalates` keeps only claim-versus-records conflicts: not received but delivered, a claimed amount above the paid amount, and contradictory statements. Both built-in entries appear in the trace like rules, with generic customer reasons ("This request needs a closer look from our team." and "This item already has a refund on record…") that never reveal what screening detected.
**Rationale:** ADR-003 and ADR-013 are invariants, not policy preferences, so no admin edit can break them. Option (b) would need special-casing in the form and its validation. The trace still explains every escalation.

## ADR-031: Backend runs on `OfflineAssistant` until OpenRouter is wired
**Status:** Superseded by ADR-033
**Context:** The build order puts the HTTP API, auth, conversations and the message pipeline (milestone 3) before the OpenRouter integration (milestone 4). The pipeline and its integration tests run on `FakeAssistant`, but the running binary needs some assistant, and `docker-compose.yml` does not yet pass `OPENROUTER_API_KEY` to the backend.
**Options:** (a) refuse to start the binary without `OPENROUTER_API_KEY` from milestone 3 on; (b) run the binary with a stub `RefundAssistant` until milestone 4 adds the real provider.
**Decision:** (b). `ai::OfflineAssistant`, defined in `backend/crates/ai/src/lib.rs` and wired in `backend/crates/api/src/main.rs`, implements `RefundAssistant`; every stage call (intake, respond, review) returns `AiError::Transport("no LLM provider is configured")`.
**Rationale:** Option (a) would require the key before any code reads it, and `docker compose up` would fail for a reviewer without one. With (b), the live stack fails closed: the pipeline treats the failed intake as `llm_failure`, so every request becomes Escalated, the customer gets the Rust template reply (`domain::responder::fallback_reply`, ADR-003), and an admin resolves it manually. No request is approved or denied without a model. The background review job's single attempt also fails, marking escalation reviews failed instead of drafted. `docker compose up` still works without an API key during this period, and startup logs a warning that every refund request will be escalated. Milestone 4 replaces `OfflineAssistant` with the OpenRouter assistant plus a fail-fast key check and should supersede this ADR.

## ADR-032: A responder failure is a flag the engine applies
**Status:** Accepted
**Context:** The responder LLM only words a verdict the engine already made; its reply is validated (`domain::responder::validate_reply`), and a reply that names another outcome counts as a failed call. After the primary and the fallback model both fail, the pipeline must still answer the customer and persist a verdict. The original design set the verdict to Escalated directly. ADR-030 has since made every flag go through the engine as a built-in `fail_closed` Escalated entry, with the most severe verdict winning (ADR-017).
**Options:** (a) overwrite the verdict with Escalated outside the engine; (b) add `responder_failure` to the facts' flags and run `decide` again, then send the Rust template reply (`fallback_reply`) for the resulting verdict.
**Decision:** (b). Implemented in `backend/crates/api/src/pipeline.rs` (`decide_and_reply`).
**Rationale:** One precedence rule covers every flag: the stored rule trace always contains the entry that produced the verdict (`fail_closed`), so the audit explains itself; with (a) the trace would show an approval rule next to an Escalated verdict. An Approved decision becomes Escalated, so money is never approved without a validated reply. A Denied decision stays Denied (Denied outranks Escalated) and is worded by the template, since the denial came from deterministic rules (e.g. final sale, expired window), not from the failed model, so escalating it would only add admin work. The customer always gets a reply, and the template is proven to pass validation. This refines ADR-003 for this one failure type, consistent with ADR-030.

## ADR-033: The backend only serves with a real OpenRouter key
**Status:** Accepted
**Context:** ADR-031's stub let the backend run with no LLM provider by escalating every request, so `docker compose up` never failed for a reviewer without a key. OpenRouter is now wired: `ai::OpenRouterAssistant` implements `RefundAssistant`, and a misconfigured or missing key should be caught before the pipeline turns it into silent escalations.
**Options:** (a) keep failing closed without a key (ADR-031's stub); (b) `${OPENROUTER_API_KEY:-}` in compose and let the backend exit at runtime; (c) require the key at the boundary: compose refuses to start the container, and the binary also refuses to serve.
**Decision:** (c). `ai::OfflineAssistant` is removed; `backend/crates/api/src/main.rs` builds `ai::OpenRouterAssistant`. `api::config::Config::openrouter_api_key()` refuses a missing or blank `OPENROUTER_API_KEY`, and refuses the `.env.example` placeholder `sk-or-v1-replace-me`, with a message naming the variable; `main.rs` checks it before connecting to Postgres, so a missing key fails fast. `refund-api seed` does not read the key, so `make seed` and `make db-reset`, which run the binary natively without `.env`, keep working. `docker-compose.yml` passes `OPENROUTER_API_KEY: ${OPENROUTER_API_KEY:?...}` on the backend service plus the seven `AI_*` overrides (empty default falls back to `backend/crates/ai/models.toml`). Compose interpolates every service's variables even for `docker compose up postgres`, so the Makefile's postgres-only targets (test, check, seed, db-reset, sqlx-prepare, psql, down, dev) go through a `DB_COMPOSE` variable that supplies a stand-in key only when none is set in the environment. Only `make up` and `docker compose up` need the real key.
**Rationale:** Option (a) hides a misconfiguration inside the admin queue, where a reviewer only discovers it after every request lands as Escalated. Option (b) still only surfaces the failure in container logs after a build. Option (c) fails at the earliest possible point — `docker compose up` stops immediately with a named variable, and the binary itself never serves without a valid key even outside compose. Tests never call a model, so `make check` passes without a key.

## ADR-034: OpenRouter calls go through Rig's low-level completion request, not its Agent API
**Status:** Accepted
**Context:** The build plan sketched Rig's Agent API (`client.agent(model)...prompt().extended_details()`). But `api::pipeline::call_with_fallback` and `api::review_job` already own the per-attempt timeout, the single fallback attempt and the audit record (ADR-003, ADR-032), and the pipeline owns reply validation (ADR-032). An agent loop would duplicate control flow the project already owns and would need tool support the pipeline never uses.
**Options:** (a) Rig's Agent API per stage; (b) one HTTP request per stage call through Rig's low-level completion request, built and sent by the project.
**Decision:** (b), implemented in `backend/crates/ai/src/openrouter.rs` and `backend/crates/ai/src/prompts.rs`. Each stage call uses `client.completion_model(model).completion_request(user).preamble(system).max_tokens(n).additional_params(json).send()` from Rig 0.42, with the `rig` facade's default features off so only `reqwest` and `rustls` compile in (the agent runtime is not built). Every request's `additional_params` carries `reasoning.effort` (from the stage config, ADR-011) and `provider.require_parameters: true`, so OpenRouter only routes to upstreams that honour every parameter, and a strict schema or effort is never silently dropped. Intake and review add a strict `json_schema` `response_format` built by the project, not by Rig's `output_schema`: `ai::prompts::strict_schema` rewrites schemars output (inlines `$ref`s, closes every object, requires every property, turns `anyOf [T, null]` into `type: [T, "null"]`, and keeps only the `uuid` format), checked by a unit test that walks the schema. The responder stage is plain text with no schema. Output caps are intake 4096, responder 1024, review 8192 tokens (reasoning tokens count toward the cap). Errors map to `AiError`: a non-2xx response or a 2xx error envelope becomes `Http{status, body}` with the body truncated to 500 chars (it lands in `decision_audit.stages`); an empty turn becomes `Empty`; a network failure becomes `Transport`; a serde failure decoding the intake or review JSON, including an unknown field such as a stray `verdict` key (the domain types use `deny_unknown_fields`), becomes `InvalidJson`. Token usage fills `StageRecord.prompt_tokens` / `completion_tokens`. Customer text is only ever placed inside `<message id=".." seq="..">` tags in the prompt; any `<message` or `</message` the customer typed, in any case, is escaped to `<\message` / `<\/message` so a message body cannot close its own tag or open a fake one. The responder stage never receives customer text at all. Model slugs `openai/gpt-6-luna`, `openai/gpt-6-luna-pro` and `openai/gpt-5.6-luna` (ADR-011) were confirmed on OpenRouter's public models list on 2026-09-25, each supporting `reasoning`, `response_format` and `structured_outputs`.
**Rationale:** One HTTP request per call, with no tools and one turn, is fewer moving parts than an agent loop, and the request body stays fully controlled and assertable: wiremock tests (`backend/crates/ai/tests/reasoning_effort.rs`) assert on the exact JSON sent. Timeouts and fallback attempts stay in one place (`call_with_fallback`), consistent with ADR-003. The escaping scheme keeps customer-controlled text unable to forge a message boundary, extending the injection defences of ADR-001 and ADR-023 into the prompt itself.

## Future work
- LLM-assisted policy authoring with dry-run impact preview (ADR-019).
- Fraud-scoring stage added to the pipeline (ADR-002).
- Separate policy-editor permission, distinct from support admins.
- Attachments via S3-compatible storage or a volume (ADR-026).
- Payout integration after approval (ADR-025).

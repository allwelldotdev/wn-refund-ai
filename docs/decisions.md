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
**Status:** Accepted; superseded by ADR-041 (low-confidence timing only)
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
**Status:** Accepted; superseded by ADR-042 (a second request for the same item), ADR-043 (messages after an answered request)
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

## ADR-035: Frontend routes follow the design mockup
**Status:** Accepted
**Context:** The design mockup specifies a route structure organised around orders and admin queues, differing from the build plan's earlier `/chat`, `/chat/[id]` and single `/admin` page. Client-side navigation carries no per-tab session id (ADR-010), so route guards run client-side while the Rust API remains the enforcement point.
**Options:** (a) keep the plan's `/chat`, `/chat/[id]` and single `/admin` page; (b) follow the mockup's route structure, with the request case file as a drawer and chat as a widget, both driven by URL state.
**Decision:** (b). Routes: `/login` (sign-in plus demo accounts), `/support` (the customer's "My orders" page with the help chat as a widget), `/admin/overview`, `/admin/requests`, `/admin/escalations`, `/admin/policy`. `/` redirects to `/login`, `/admin` redirects to `/admin/overview`. The request case file opens as a drawer via `?ref=RR-…` on any admin page. The chat widget's state lives in the URL (`?chat=<conversationId|new>&order=&view=requests`), so a reload restores it. One responsive implementation serves desktop, tablet and mobile. Route guards are client-side.
**Rationale:** The mockup organises the customer view around orders, with help attached to each order, so routes and URL state should match that structure rather than the plan's chat-first layout. Client-side guards are a UX convenience only; ADR-010 already puts enforcement in the Rust API, so nothing weakens security.

## ADR-036: Demo data reseeded as a co-working catalogue
**Status:** Accepted
**Context:** ADR-014 seeded a general e-commerce catalogue. The design mockup shows a co-working business (day passes, room bookings, memberships, office deposits, accessories, subscriptions).
**Options:** (a) keep the ADR-014 seed and genericise the UI to match; (b) reseed to match the mockup's catalogue.
**Decision:** (b). 15 customers use the mockup's names (`first.last@example.com`); admins are Ngozi Adeyemi and Sam Whitfield (`@worknoon.example`); the demo password is `demo-2026`; order refs are `ORD-1xxxx`. The same 15 scenario keys and expected verdicts from ADR-014 carry over, except `clean_damaged_home` is renamed `clean_damaged_subscription`. The default policy keeps a 14-day refund window for everything, plus 7 days for Accessories; human review above $500; repeat claims 2 in 30 days. Request refs stay `RR-` (a schema default), although the mockup shows `REQ-`.
**Rationale:** Matching the mockup's business domain makes the demo read as a coherent product rather than a generic store, at no cost to the scenario coverage ADR-014 established. Changing the request-ref prefix would touch the schema for a cosmetic difference the spec doesn't require (ADR-028).

## ADR-037: Backend additions the design needed
**Status:** Accepted; superseded by ADR-046 (customer notice on admin resolve)
**Context:** Building the admin and customer views to the mockup surfaced gaps in the existing API: an overview page needs aggregate counts, the admin queue needs richer filtering, sign-in has no throttling, and demo accounts need scenario metadata for reviewers. This extends ADR-027 (audit access) and ADR-024 (streaming vs. polling).
**Options:** (a) build only what earlier milestones specified; (b) add the endpoints and behaviour the frontend milestone requires.
**Decision:** (b). `GET /api/admin/stats?since=` returns the overview's "today" counts and the oldest open escalation. The admin queue gains a `since` filter and a repeatable `flag` filter, where each value is an any-of group and all groups must match, covering date-range, escalation-reason and "flagged for injection" filters. Sign-in throttling pauses sign-in for one email for 5 minutes after 5 consecutive failures (429 with Retry-After); a 401 carries `attempts_left`; unknown emails are counted the same way so nothing reveals which accounts exist; the limiter is in memory, like the message limiter. Demo accounts carry each scenario's title, description, expected verdict and order ref, sourced from the seed matrix. Deferred: telling the customer in chat when an admin resolves an escalation, though the mockup shows it; the resolve note stays an audit note, and the customer instead sees the new status ("Approved/Denied after review") in their requests list.
**Rationale:** These additions extend ADR-027's audit access and ADR-024's polling model rather than replacing them: `stats` and `flag` filtering are read paths built the same way. Sign-in throttling closes an enumeration and brute-force gap the design surfaced. An in-chat resolution notice would need a new customer-facing push channel beyond ADR-024's SSE-for-replies model, so it is deferred rather than built ad hoc.

## ADR-038: Where the mockup and the spec differ, the spec wins — the concrete list
**Status:** Accepted; superseded by ADR-044 (appeal / dispute button)
**Context:** ADR-028 established that the spec wins where the mockup and spec differ, citing model name, retry count, statuses and attachments as examples. Building the frontend surfaced a longer, concrete list of such differences. This extends ADR-028.
**Options:** (a) resolve each difference ad hoc as it is found; (b) enumerate the full list once, as a single reference extending ADR-028.
**Decision:** (b). No payout, pending or processing states, and no payment-method or delivery-time promises (ADR-025). No attachments or photo requests (ADR-026). No appeal button; the verdict freezes (ADR-021). One refund request per conversation: after a verdict, another order starts a new conversation. Customers never see rule names or policy internals; a denied card links to the generated policy text (ADR-018). One admin role; no "policy editor" tier. LLM-error details show the real single fallback (ADR-011), and the low-confidence threshold shown is 0.6. Light theme only, as the design system specifies.
**Rationale:** A single enumerated list prevents each difference from being re-litigated per page, and keeps ADR-028's rule traceable to concrete outcomes rather than left as a general principle.

## ADR-039: Policy semantics unchanged by the mockup
**Status:** Accepted
**Context:** The mockup implies two semantic changes: that a category refund window replaces the general one (e.g. "Office deposits: 30 days" alongside "All: 14 days"), and that damaged items are refundable "even if final sale". ADR-017 stands.
**Options:** (a) change the engine to match the mockup's implied semantics; (b) keep ADR-017's evaluate-all-rules, strictest-wins model, and treat the mockup's wording as illustrative only.
**Decision:** (b). Every applicable rule is evaluated and the strictest outcome wins. A category window can only shorten the general window, never lengthen it, and final sale still denies a damaged item.
**Rationale:** Order-independent, deterministic decisions (ADR-017) matter more than matching an illustrative mockup literally. The UI shows the backend-generated policy text (ADR-018), so what customers read always matches what gets enforced, regardless of how the mockup phrased it.

## ADR-040: Verdict first without changing the stream
**Status:** Accepted
**Context:** The mockup shows the verdict badge and reference before the explanation text. ADR-024's SSE stream commits the reply before streaming it and sends `request_updated` after the text, so token order does not naturally match the mockup's visual order.
**Options:** (a) reorder SSE events so the verdict arrives before reply tokens; (b) keep ADR-024's event order, and on `done` re-read the stored conversation and animate the stored reply client-side, badge and reference first, then the text typed out.
**Decision:** (b).
**Rationale:** Reordering SSE events would complicate the stream contract for a purely visual sequencing need. Tokens arrive in a burst anyway, so re-reading the stored conversation and animating it client-side loses nothing perceptible while keeping ADR-024's stream unchanged.

## ADR-041: Low confidence only escalates once the request is complete or clarifying runs out
**Status:** Accepted
**Context:** ADR-003 made low intake confidence one of the fail-closed triggers. In practice a vague first message ("Something I ordered arrived broken.") came back from intake as needs_info with confidence below 0.6, so the low-confidence flag escalated it before the clarifying step (ADR-021, up to 3 questions) could run. Low confidence on an incomplete request only means the request is not understood yet.
**Options:** (a) keep escalating on any low confidence; (b) apply low confidence only when the extraction is complete, and otherwise let the clarifying questions run, adding the flag if they run out; (c) drop the low-confidence flag.
**Decision:** (b), implemented in `backend/crates/api/src/pipeline.rs` (`screen_intake`, `process`) and `backend/crates/domain/src/intake.rs`. The flag is raised only when nothing is missing (order, item, reason all known) or when the clarify limit is reached with confidence still low (then both low_confidence and clarification_limit are recorded). Safety flags (pre-scan hits, intake injection signals, foreign order references) still escalate immediately. The engine is not called during clarify turns, so no decision is made until the request is complete or the questions run out.
**Rationale:** Questions resolve most vague openings; fail-closed still applies to every request the engine actually decides and to anything suspicious; the audit still records low confidence when it mattered.

## ADR-042: The assistant stays on refund requests and reports earlier requests instead of re-deciding them
**Status:** Accepted; tone superseded by ADR-051
**Context:** Customers asked the assistant about things other than refunds, and asked again about items that already had a request. Before, every message went towards a decision: an off-topic message became a clarifying question or an escalation, and a second request for an item was decided again (repeat_claim_limit, or escalated by the built-in active_refund_exists check, ADR-030). The product owner asked that the assistant stay strictly on refund requests (no other topics, no browsing), speak empathetically, and, when an item already has a request in any state, tell the customer its status and ask if there is anything else, filing nothing.
**Options:** (a) handle it all in the responder prompt only; (b) let intake classify the latest message's intent (refund_request | out_of_scope | finished) and have Rust choose a no-request reply mode; (c) keyword filters in Rust.
**Decision:** (b). Intake returns `intent`; the item's newest earlier request (ref, state, decided_at) is given to intake as trusted data. After screening — only when no safety flag is raised, so anything suspicious still escalates — and before clarifying, the pipeline picks: finished → closing reply; out_of_scope → redirect reply; target item already has a request (any state) → existing_request reply naming that request and its status and asking if there is anything else. Nothing is filed and the engine is not called. These are new assistant kinds, do not count as clarify turns (ADR-021), and are validated: existing_request replies must name the earlier ref, ask a question and use only that request's real outcome word; closing and redirect may name no outcome. They fall back to Rust templates if the model fails twice rather than escalating, since nothing is being decided. The rest of the conversation continues normally, so the customer can go on to another order. All three prompts (intake, responder, review) state the refund-only scope and that the models have no tools or internet access; the responder gets a tone rule (warm, polite, plain, one sincere acknowledgement per reply).
**Rationale:** Intent classification is already part of the model's job and stays untrusted-but-harmless (a wrong intent only changes the wording, never a verdict). A second request for the same item was noise for admins. Templates keep replies safe when the model fails. The "Already refunded" demo scenario now gets the status of RR-0903 in chat rather than a new escalated request; the engine's active_refund_exists check (ADR-030) stays as a safety net, e.g. for requests that race across two conversations.

## ADR-043: An answered request closes to new customer messages, enforced in the API
**Status:** Accepted
**Context:** ADR-021 saves messages sent after the verdict, tags them After decision, and gives a holding reply, but the conversation stays open indefinitely. The product owner asked that an answered request (approved or denied, automatically or after an admin's review) close: the chat's message box is replaced by a "Start new request" button, and the chat stays readable in Your requests. Escalated requests are still being worked on, so the customer should still be able to add details for the specialist. Items that already have a request should also not be pickable in the chat's order picker (ADR-042 already stops the pipeline from filing a second request).
**Options:** (a) hide the composer in the UI only; (b) close in the UI and enforce in the API; (c) leave conversations open and rely on holding replies.
**Decision:** (b). `POST /api/conversations/{id}/messages` (`backend/crates/api/src/conversations.rs`, `post_message`) returns 409 `request_closed` when the conversation's request is approved, denied, resolved_approved or resolved_denied; escalated conversations still accept messages, which get the holding reply and the After decision tag (ADR-021 still describes that part). The check runs after the client_msg_id duplicate check, so a retried send is still acknowledged. The frontend (`frontend/lib/customer.ts`, `frontend/components/chat/*`) replaces the composer with a footer and a "Start new request" button, shows requests read-only in Your requests, blocks every item that already has a request in any state in the order picker (marked Approved / Approved after review / Denied / Denied after review / Under review), and shows the earlier request inline when an order with a partly-requested item is picked.
**Rationale:** The API is the enforcement point (roles and rules live there, ADR-010), so a stale tab or a direct call cannot reopen a decided chat; escalated chats stay open because the admin has not decided yet.

## ADR-044: Customers can dispute an automatic denial once
**Status:** Accepted
**Context:** ADR-038 dropped the design's appeal button because the verdict freezes (ADR-021). The product owner now wants customers to be able to ask a person to review a request the assistant denied automatically, as an admin-controlled option; decisions made by an admin stay final.
**Options:** (a) no disputes; (b) let the customer reopen the chat and re-run the pipeline; (c) a one-time dispute that hands the automatic denial to a person without re-deciding it.
**Decision:** (c). `POST /api/conversations/{id}/dispute {reason?}` is allowed only when the request's state is `denied` (automatic), it has not been disputed, and the setting is on (409 `not_disputable` / `already_disputed` / `disputes_off`). In one transaction with the request row locked: the optional reason is stored and pre-scanned like any customer message, a system note ("You disputed this decision on …") is added, the request becomes `escalated` with `disputed_at`, a `disputed` event (actor customer) and a pending review are recorded. The decision audit row is not changed, so the automatic verdict and its rule trace stay on record; the engine is not re-run. The review job sees the reason and `disputed_at`. Admins see a Disputed flag, a "Customer dispute" filter and a timeline step, and resolve it like any escalation; the result (resolved_approved/denied) is final.
**Rationale:** Keeps the frozen verdict auditable, puts a person in the loop for the one case customers most often contest, and cannot loop (one dispute; admin decisions are final).

## ADR-045: Admin settings live outside the policy versions
**Status:** Accepted
**Context:** The dispute switch (ADR-044) is an operational setting, not a refund rule; policy versions are append-only, hashed and cited by each decision (ADR-008, ADR-016).
**Options:** (a) make it a rule in the policy JSON; (b) a separate single-row `app_settings` table read at dispute time.
**Decision:** (b). `app_settings` (allow_disputes default on, updated_by_admin_id, updated_at), `GET/PUT /api/admin/settings` for admins; a save records who changed it only when the value changes; the value is read when a customer disputes, and turning it off does not cancel disputes already sent. Customers get a derived `can_dispute` on their requests.
**Rationale:** The engine and the policy hash stay about refund eligibility; toggling disputes doesn't create a policy version or change how any decision is explained.

## ADR-046: A drafted, validated notice tells the customer why an admin resolved their escalation
**Status:** Accepted; tone superseded by ADR-051
**Context:** ADR-037 kept the admin's resolve note audit-only; the customer only saw the new status in Your requests. The product owner now wants the customer told in the chat, in a warm "Dear {first name}" message explaining how and why, written from the admin's note, plus a short summary line. If the model fails, the product owner chose that the review is not completed (no template fallback); the admin's note is kept.
**Options:** (a) the admin writes the customer message directly; (b) a model rewrites the admin's note, the admin previews and confirms before it is sent; (c) two separate fields (audit note + customer message).
**Decision:** (b). A fourth LLM stage, `notice`, gets its own model, effort and timeout in `backend/crates/ai/models.toml` (`[notice]`), its own `AI_NOTICE_*` env overrides, and one fallback model, matching the other stages (ADR-011). `backend/crates/ai/src/prompts.rs` (`NOTICE_SYSTEM`) drafts the message from the admin's note and the resolution. `POST /api/admin/requests/{ref}/resolve/draft` (`backend/crates/api/src/admin.rs`, `draft_notice`) returns `{message, summary}`; output must pass `domain::notice::validate_notice` (greets the customer by first name, states the resolution's actual outcome word and never the other one or "escalated", states the approved amount, and includes a one-line summary). If both the primary and fallback attempts fail validation, the endpoint returns 503 `assistant_unavailable`, nothing changes, and the request stays escalated with the admin's note intact. `POST /api/admin/requests/{ref}/resolve` requires the previewed message and summary, re-validates them, and in the same transaction as the state change posts the message as an `admin` chat message and the summary as a `system` note, using the message roles from ADR-044's migration. The admin's original note is stored unchanged in the audit. The frontend resolve dialog shows the draft for the admin to preview before confirming.
**Rationale:** Fails closed like the rest of the pipeline (ADR-003): a template fallback would either leak the admin's internal wording or send generic text the admin never reviewed, so resolution stays blocked until a valid draft exists. The preview step keeps a person responsible for what the customer reads. Validation stops the model from contradicting the decision it is only meant to word.

## ADR-047: Test orders use a server-owned catalogue and a real-engine preview
**Status:** Accepted
**Context:** Orders were seed-only, so trying the chat meant using the scripted demo accounts. The product owner asked for an "Add order" button in My orders that simulates a customer order — a sequential read-only order number, order date, items from a catalogue covering the co-working offers and products, a total, and a delivery status (Delivered with a date, Used on the order day, Confirmed with a start date, or Active for a running plan) — for live demos.
**Options:** (a) let the client send item names, prices and categories; (b) a server-owned catalogue where the client sends catalogue ids and quantities; (c) an admin-only seeding tool.
**Decision:** (b). `GET /api/catalog` lists the catalogue; `POST /api/orders` (customer-only) validates catalogue ids server-side and sets names, prices, totals, categories and dates itself, marking the order `is_test`. `POST /api/orders/preview` runs the real policy engine (ADR-017) on the order as it would be stored, per item, assuming a damage report today and using the customer's real prior claims, without storing anything, so the "likely decision" shown never comes from frontend rules. Migration `0004_test_orders.sql` adds `orders.fulfilment` (delivered/used/confirmed/active), `starts_at`, `ends_at` and `is_test`; the seed fills `fulfilment` for seeded orders. Order numbers continue the `ORD-` series (max + 1 under a transaction-scoped advisory lock). Catalogue categories reuse the seed's six (ADR-036), so category-scoped windows apply, and two final-sale items exercise that rule. Used and Active orders count as delivered on the order date for the refund window; Confirmed has no delivery yet.
**Rationale:** Prices and categories drive refund decisions, so they must not come from the browser. A dry run of the real engine keeps the policy in one place, since the original design's own preview rules differed from the engine.
**Consequence:** Test orders live in the same tables as seeded ones and are only told apart by `is_test`; `make db-reset` removes them.

## ADR-048: Customer views poll while a request is with a specialist
**Status:** Accepted
**Context:** ADR-024 gave customer replies SSE and admin views polling; nothing on the customer side re-read after the reply stream ended. ADR-046 added an in-chat notice when an admin resolves an escalation, but the customer's status tag still showed Escalated until a page reload.
**Options:** (a) keep as is, requiring a reload; (b) a server push channel to the customer (SSE subscription or WebSocket) for request updates; (c) polling on the customer side, only while something is under review.
**Decision:** (c). The customer's conversation list re-reads every 3 seconds while any of their requests is escalated; a thread (live chat or request detail) re-reads every 3 seconds while its own request is escalated; both stop once nothing is under review. The live chat pauses its poll while a send is in flight, since the send itself re-reads the thread after the stored reply. The request detail's status comes from its polled thread. No backend change.
**Rationale:** Reuses the existing BFF and query setup, with no new streaming endpoint or long-lived connection. Load stays bounded because polling only runs while a request is under review, which is rare and short. A 3-second interval is close enough to live for a human decision. This extends ADR-024 rather than replacing it: customer views now also poll, but only in this one state; ADR-024 stays Accepted.

## ADR-049: The assistant answers greetings and questions about the customer's orders
**Status:** Accepted
**Context:** ADR-042's intents were refund_request | out_of_scope | finished. Customers testing the chat said "Hello" or "Hi, what happened to order 10416?" and got the refunds-only redirect, which read as rigid; asking to see their orders did nothing useful. This extends ADR-042, which stays Accepted.
**Options:** (a) widen the responder prompt only; (b) two new intake intents with Rust choosing the reply and the facts; (c) keyword matching in Rust.
**Decision:** (b). Intake adds `greeting` and `order_inquiry` (asks what happened to an order or earlier request, to see their orders, or whether something can be refunded without describing a problem). The intake prompt maps a bare number ("10416" → ORD-10416) or an item name to the customer's order and narrows out_of_scope to topics that are not about their orders or refunds; each turn is judged on its latest message. Replies (none files a request, none is a clarify turn, only when no safety flag is raised): greeting → a fixed sentence with no model call; order_inquiry with an order → responder mode `order_status` given trusted facts only (order ref, placed/delivered dates, each item's amount and its newest request with status, or none), stating the record and asking whether they want a refund for items without a request, validated to name the order and every earlier request ref, use an outcome word only where a request has it, and ask a question when an item is open, with a Rust template fallback; order_inquiry without an order → a fixed reply plus the chat showing the order chips. Rust resolves the order from intake's order id, else from its item id, else from the order the customer picked with that message, always within the customer's own orders. New message kinds greeting, order_status, order_list (migration 0005).
**Rationale:** Intent stays untrusted-but-harmless, as in ADR-042: a wrong guess changes wording, never a verdict. Order facts come from the database, not the model. Fixed sentences need no model call where nothing depends on records. The engine is never called, so the LLM still decides no refund (ADR-001).

## ADR-050: One final question before every decision
**Status:** Accepted
**Context:** The first message that made a request complete (order, item and reason known) went straight to a verdict, and a verdict freezes the request (ADR-021). Customers had no chance to add a detail they thought mattered.
**Options:** (a) keep deciding at once; (b) ask one final question per conversation before deciding; (c) ask for confirmation of the extracted facts (order, item, reason) with yes/no buttons.
**Decision:** (b). When the request is complete and no safety flag is raised, the pipeline replies with a new kind `final_check` (responder mode worded by the model, validated to ask a question and name no outcome; Rust template "Got it. Anything else I should know before I check this?" on failure) and stops. It is asked once per conversation, known from the messages the pipeline already loads. The next customer message goes to the decision with every message in view, so any detail reaches intake and the engine. While the last reply is the final question, a finished, out_of_scope or greeting intent does not short-circuit to a no-request reply ("No, that's all" means go ahead); an order question or an item that already has a request still gets its usual reply. Flagged requests (pre-scan signals, injection signals, foreign order reference, low confidence on a complete request) escalate at once without the question. Migration 0006 adds the kind. The final question is not a clarify turn (ADR-041's count is unchanged).
**Rationale:** One extra turn for every decided request is a small cost for letting the customer add context before the verdict freezes. A question worded from trusted data cannot leak or change an outcome. Safety cases skip it so a manipulation attempt gets no extra turn. The engine still makes every decision.

## ADR-051: Replies are brief and professional, without apologies
**Status:** Accepted
**Context:** After live testing, the product owner found replies long and over-apologetic. ADR-042's mandatory acknowledgement sentence produced lines like "I'm sorry for any uncertainty about your order" on every reply, including neutral ones such as an order status.
**Options:** (a) keep the empathetic tone; (b) professional, courteous and brief, no apologies or filler; (c) enforce "no sorry" in reply validation.
**Decision:** (b). Responder: professional, courteous and brief, never apologise or say sorry, no filler or sympathy lines, at most 60 words (order_status may add a short clause per item); the denial opener drops "Unfortunately". Notice: "Dear {first_name}," then one or two sentences, at most 50 words, faithful to the admin's note, no sympathy line; the notice validator's message cap drops from 1200 to 600 characters. All four prompts were streamlined without dropping any rule. Fallback templates shortened. Not (c): rejecting a reply over a word would turn an approval into an escalation via the responder-failure path (ADR-032), too costly for a tone slip.
**Rationale:** Shorter, neutral replies read as more professional and are faster to scan; the facts and outcome words, which validation protects, are unchanged; prompt-level guidance was enough in live checks (no "sorry" across greeting, clarify, redirect, order status, final question and all three verdicts).

## ADR-052: Admins message customers on escalated requests; messages are stored as written, with author and read markers
**Status:** Accepted
**Context:** A specialist deciding an escalation could only approve or deny; to ask for evidence they had no channel, and the customer's added details got the same bot holding reply every time. The updated design adds admin-customer chat on escalated requests with unread badges, a banner and toasts; the design's demo syncs tabs with a browser BroadcastChannel.
**Options:** (a) route admin messages through the notice model like the resolution message (ADR-046) vs (b) store the admin's message verbatim; for delivery, (c) a push channel vs (d) the existing polling (ADR-048 on the customer side, 5 s polling on admin views, ADR-024); for unread state, (e) client-side "seen" state vs (f) server-side read markers.
**Decision:** (b), (d), (f). `POST /api/admin/requests/{ref}/messages` stores the message exactly as written (1-4000 characters) with `messages.author_admin_id`, only while the request is escalated (409 `not_escalated` after; a row lock on the request orders it against a resolve). Only the Approve/Deny message is still worded by the model (ADR-046), and it now records its author too. Read markers `conversations.customer_read_seq` / `admin_read_seq` (one shared marker for all admins) advance through `POST /api/conversations/{id}/read` and `POST /api/admin/requests/{ref}/read` with the last seen seq, never backwards; existing conversations start as read. Conversation summaries carry `unread_count`, `last_reply_at`, `last_reply_by`; admin queue rows carry `unread_from_customer` (customer messages after the decision and the admin marker) and `customer_replied_at` (set while the customer's latest post-decision message has no admin reply). On an escalated request the assistant sends at most one holding reply and none once an admin has written. The decision message is the last admin message of a resolved request (the chat closes on resolve, ADR-043), so no extra column marks it. Where the design and spec differ the spec wins (ADR-028): no BroadcastChannel; refs stay RR- (ADR-036).
**Rationale:** An admin's own words need no model and can't be distorted by one; the LLM still decides nothing (ADR-001). Polling already runs exactly while a request is escalated, which is exactly when chat is possible. Server-side markers keep counts right across tabs, devices and admins. A quiet bot keeps the thread a conversation with the person.

## Future work
- LLM-assisted policy authoring with dry-run impact preview (ADR-019).
- Fraud-scoring stage added to the pipeline (ADR-002).
- Separate policy-editor permission, distinct from support admins.
- Attachments via S3-compatible storage or a volume (ADR-026).
- Payout integration after approval (ADR-025).
- A rule that refuses refunds for already-used bookings based on evidence the customer uploads (needs file storage, ADR-026).
- Assistant memory through agentic retrieval over the customer's orders and past messages, to cut repeat questions and shorten conversations, personalise replies, and improve interactions.

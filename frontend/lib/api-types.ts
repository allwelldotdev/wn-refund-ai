/**
 * Shapes returned by the Rust API through the BFF. Every Option on the Rust
 * side is sent as `null`, never omitted. Money is integer cents; timestamps
 * are RFC 3339 strings.
 */

export type UUID = string;
export type ISODate = string;
export type Cents = number;

export type Role = "customer" | "admin";
export type Verdict = "approved" | "denied" | "escalated";
export type RequestState = Verdict | "resolved_approved" | "resolved_denied";
export type ReasonCategory =
  | "damaged"
  | "wrong_item"
  | "not_received"
  | "changed_mind"
  | "not_as_described"
  | "other";
export type Flag =
  | "prescan_signal"
  | "intake_injection_signal"
  | "low_confidence"
  | "foreign_order_reference"
  | "llm_failure"
  | "responder_failure"
  | "clarification_limit"
  | "no_rule_fired";
export type ReviewStatus = "pending" | "drafted" | "failed";
export type AssistantKind =
  | "clarify"
  | "verdict"
  | "holding"
  | "existing_request"
  | "closing"
  | "redirect"
  | "greeting"
  | "order_status"
  | "order_list";
export type OrderStatus = "processing" | "shipped" | "delivered";
export type Fulfilment = "delivered" | "used" | "confirmed" | "active";

export const REQUEST_STATES: RequestState[] = [
  "approved",
  "denied",
  "escalated",
  "resolved_approved",
  "resolved_denied",
];

export interface ApiErrorBody {
  error: {
    code: string;
    message: string;
    fields?: { path: string; message: string }[];
    latest?: PolicyVersionView;
    attempts_left?: number;
  };
}

// Auth

export interface Principal {
  kind: Role;
  id: UUID;
  name: string;
  email: string;
}

export interface DemoAccount {
  name: string;
  email: string;
  role: Role;
  scenario: string | null;
  title: string;
  description: string;
  expected_verdict: Verdict | null;
  order_ref: string | null;
}

// Customer

export interface PublicPolicy {
  version: number;
  prose: string;
}

export interface OrderItem {
  id: UUID;
  name: string;
  category: string;
  quantity: number;
  amount_cents: Cents;
  final_sale: boolean;
  active_refund: boolean;
}

export interface Order {
  id: UUID;
  ref: string;
  placed_at: ISODate;
  delivered_at: ISODate | null;
  status: OrderStatus;
  total_cents: Cents;
  fulfilment: Fulfilment | null;
  /** A confirmed booking's start, or when an active plan began. */
  starts_at: ISODate | null;
  /** When an active plan runs out, if it has an end. */
  ends_at: ISODate | null;
  /** Added by the customer from My orders to try the chat (demo). */
  is_test: boolean;
  items: OrderItem[];
}

export type CatalogKind = "booking" | "plan" | "deposit" | "service" | "product";

export interface CatalogItem {
  id: string;
  group: "workspace" | "services" | "products";
  name: string;
  kind: CatalogKind;
  unit_cents: Cents;
  unit: string;
  hourly: boolean;
  category: string;
  final_sale: boolean;
}

export interface Catalog {
  /** A preview only: the number is assigned when the order is added. */
  next_order_ref: string;
  groups: { key: CatalogItem["group"]; label: string; items: CatalogItem[] }[];
}

export interface NewOrderBody {
  placed_on: string;
  items: { item: string; quantity: number }[];
  fulfilment: Fulfilment | null;
  delivered_on: string | null;
  starts_on: string | null;
  runs_for_days: number | null;
}

export interface OrderPreview {
  assumption: string;
  items: { name: string; verdict: Verdict; reason: string }[];
}

export interface RequestSummary {
  id: UUID;
  ref: string;
  state: RequestState;
  order_ref: string | null;
  item_name: string | null;
  amount_cents: Cents | null;
  created_at: ISODate;
  resolved_at: ISODate | null;
  disputed_at: ISODate | null;
  /** An automatic denial, not yet disputed, while disputes are allowed. */
  can_dispute: boolean;
}

export interface ConversationSummary {
  id: UUID;
  created_at: ISODate;
  updated_at: ISODate;
  last_seq: number;
  preview: string | null;
  request: RequestSummary | null;
}

/** `admin`: an admin's decision sent to the customer; `system`: a note such as a dispute. */
export type MessageRole = "customer" | "assistant" | "admin" | "system";

export interface Message {
  id: UUID;
  seq: number;
  role: MessageRole;
  assistant_kind: AssistantKind | null;
  body: string;
  order_id: UUID | null;
  created_at: ISODate;
}

export interface ConversationDetail {
  conversation: { id: UUID; last_seq: number; created_at: ISODate; updated_at: ISODate };
  messages: Message[];
  request: RequestSummary | null;
}

export type SseEvent =
  | { event: "message_saved"; data: { message_id: UUID; seq: number; duplicate: boolean } }
  | { event: "reply_start"; data: { kind: AssistantKind } }
  | { event: "reply_token"; data: { text: string } }
  | { event: "reply_done"; data: { message_id: UUID; seq: number; body: string } }
  | { event: "request_updated"; data: RequestSummary }
  | { event: "error"; data: { code: string; message: string } }
  | { event: "done"; data: Record<string, never> };

// Admin

export interface AdminListItem {
  ref: string;
  state: RequestState;
  customer_name: string;
  customer_email: string;
  order_ref: string | null;
  item_name: string | null;
  amount_cents: Cents | null;
  reason_category: ReasonCategory | null;
  flags: Flag[];
  review_status: ReviewStatus | null;
  created_at: ISODate;
  resolved_at: ISODate | null;
  disputed_at: ISODate | null;
}

export interface AdminList {
  items: AdminListItem[];
  total: number;
}

export interface AdminStats {
  since: ISODate;
  created: {
    total: number;
    approved: number;
    denied: number;
    escalated: number;
    resolved_approved: number;
    resolved_denied: number;
  };
  open_escalations: number;
  oldest_open_escalation_at: ISODate | null;
}

export type TimelineKind = "decided" | "review_drafted" | "review_failed" | "resolved" | "disputed";

export interface TimelineEvent {
  kind: TimelineKind;
  actor_kind: "system" | "admin" | "customer";
  actor_name: string | null;
  payload: Record<string, unknown>;
  created_at: ISODate;
}

export type Detector =
  | "role_marker"
  | "instruction_override"
  | "encoded_payload"
  | "unusual_unicode"
  | "abnormal_length";

export interface SignalView {
  scope: "message" | "window";
  detector: Detector;
  start: number;
  end: number;
  score: number;
}

export interface DetailMessage {
  id: UUID;
  seq: number;
  role: MessageRole;
  assistant_kind: AssistantKind | null;
  body: string;
  created_at: ISODate;
  tag: "used_in_decision" | "after_decision" | null;
  signals: SignalView[];
}

export type RuleKind =
  | "final_sale_not_refundable"
  | "refund_window"
  | "human_review_above"
  | "damaged_or_incorrect_eligible"
  | "repeat_claim_limit"
  | "conflicting_claim_escalates";

export interface FiredRule {
  kind: RuleKind | "fail_closed" | "active_refund_exists";
  verdict: Verdict;
  explanation: string;
  customer_reason: string;
  detail: Record<string, unknown>;
}

export interface IntakeOutput {
  status: "complete" | "needs_info";
  missing: ("order" | "item" | "reason")[];
  order_id: UUID | null;
  order_item_id: UUID | null;
  mentioned_order_refs: string[];
  reason_category: ReasonCategory | null;
  claimed_amount_cents: Cents | null;
  contradictory_statements: boolean;
  injection_signals: { message_id: UUID; kind: string; excerpt: string }[];
  confidence: number;
}

export interface StageRecord {
  model: string;
  effort: "none" | "low" | "medium" | "high";
  latency_ms: number;
  prompt_tokens: number | null;
  completion_tokens: number | null;
  attempt: number;
  fallback: boolean;
}

export interface StageLog {
  record: StageRecord | null;
  failures: { model: string; error: string }[];
}

export interface AuditInfo {
  verdict: Verdict;
  flags: Flag[];
  rule_trace: FiredRule[];
  extracted: IntakeOutput | null;
  facts: {
    now: ISODate;
    order: {
      order_ref: string;
      placed_at: ISODate;
      delivered_at: ISODate | null;
      item: { name: string; category: string; amount_cents: Cents; final_sale: boolean };
    } | null;
    flags: Flag[];
  };
  stages: { intake: StageLog | null; responder: StageLog | null };
  evaluated_through_seq: number;
  policy_version: { id: UUID; version: number; content_hash: string };
}

export interface ReviewOutput {
  summary: string;
  suggested_resolution: "approve" | "deny";
  rationale: string;
  risk_notes: string[];
  questions_for_customer: string[];
}

export interface ReviewInfo {
  status: ReviewStatus;
  draft: ReviewOutput | null;
  error: string | null;
  model: string | null;
  latency_ms: number | null;
  resolution: "approved" | "denied" | null;
  resolution_note: string | null;
  resolved_by: string | null;
  resolved_at: ISODate | null;
}

export interface RequestDetail {
  request: {
    ref: string;
    state: RequestState;
    reason_category: ReasonCategory | null;
    amount_cents: Cents | null;
    created_at: ISODate;
    resolved_at: ISODate | null;
    disputed_at: ISODate | null;
  };
  customer: { id: UUID; name: string; email: string; scenario: string };
  order: {
    ref: string;
    placed_at: ISODate;
    delivered_at: ISODate | null;
    item: { name: string; category: string; amount_cents: Cents; final_sale: boolean };
  } | null;
  timeline: TimelineEvent[];
  messages: DetailMessage[];
  audit: AuditInfo | null;
  review: ReviewInfo | null;
}

export interface ResolveResponse {
  ref: string;
  state: "resolved_approved" | "resolved_denied";
  resolved_at: ISODate;
}

export interface AppSettings {
  allow_disputes: boolean;
  updated_by: string | null;
  updated_at: ISODate | null;
}

// Policy

export type WindowScope = { kind: "all" } | { kind: "category"; category: string };

export type Rule =
  | { kind: "final_sale_not_refundable"; enabled: boolean }
  | { kind: "refund_window"; enabled: boolean; days: number; scope: WindowScope }
  | { kind: "human_review_above"; enabled: boolean; amount_cents: number }
  | { kind: "damaged_or_incorrect_eligible"; enabled: boolean }
  | { kind: "repeat_claim_limit"; enabled: boolean; max_claims: number; lookback_days: number }
  | { kind: "conflicting_claim_escalates"; enabled: boolean };

/** The API wraps the rule list: `rules` in every policy body is `{ rules: Rule[] }`. */
export interface Policy {
  rules: Rule[];
}

export interface PolicyVersion {
  id: UUID;
  version: number;
  content_hash: string;
  author_kind: "system" | "admin";
  author_name: string | null;
  change_note: string | null;
  reverted_from_version_id: UUID | null;
  reverted_from_version: number | null;
  created_at: ISODate;
}

export interface PolicyVersionView {
  version: PolicyVersion;
  rules: Policy;
  prose: string;
}

import { describe, expect, it } from "vitest";

import {
  errorCount,
  fromDraft,
  newWindow,
  sameRules,
  serverErrors,
  toDraft,
  validateDraft,
} from "@/lib/policy-draft";

import { designPolicyV7 } from "./fixtures/design";

describe("policy draft", () => {
  it("round-trips the design's version 7 unchanged", () => {
    const draft = toDraft(designPolicyV7);
    expect(validateDraft(draft)).toEqual({});
    expect(sameRules(fromDraft(draft)!, designPolicyV7)).toBe(true);
  });

  it("treats a reorder as a change, because the content hash depends on order", () => {
    const reordered = [...designPolicyV7].reverse();
    expect(sameRules(reordered, designPolicyV7)).toBe(false);
  });

  it("reproduces the design's 'validation errors' board", () => {
    const draft = toDraft(designPolicyV7);
    const amount = draft.find((r) => r.kind === "human_review_above")!;
    amount.amount = "0";
    const deposits = draft.find((r) => r.kind === "refund_window" && r.category === "Office deposits")!;
    deposits.days = "400";
    const duplicate = { ...newWindow(draft), scopeKind: "category" as const, category: "office deposits" };
    draft.push(duplicate);

    const errors = validateDraft(draft);
    expect(errors[amount.uid]).toEqual({ amount: "Amount must be greater than 0" });
    expect(errors[deposits.uid]).toEqual({ days: "Days must be between 1 and 365" });
    expect(errors[duplicate.uid]).toEqual({ category: "A refund window for this scope already exists" });
    expect(errorCount(errors)).toBe(3);
    expect(fromDraft(draft)).toBeNull();
  });

  it("uses the API's ranges, not the mockup's", () => {
    const draft = toDraft(designPolicyV7);
    const repeat = draft.find((r) => r.kind === "repeat_claim_limit")!;
    repeat.maxClaims = "50";
    repeat.lookback = "3650";
    expect(validateDraft(draft)).toEqual({});
    repeat.maxClaims = "101";
    expect(validateDraft(draft)[repeat.uid]).toEqual({ maxClaims: "Max claims must be a whole number from 1 to 100" });
  });

  it("converts dollars to cents", () => {
    const draft = toDraft(designPolicyV7);
    draft.find((r) => r.kind === "human_review_above")!.amount = "600.5";
    const rules = fromDraft(draft)!;
    expect(rules.find((r) => r.kind === "human_review_above")).toMatchObject({ amount_cents: 60050 });
  });

  it("maps the API's 422 paths onto the matching cards", () => {
    const draft = toDraft(designPolicyV7);
    const { byRule, other } = serverErrors(draft, [
      { path: "rules[5].days", message: "must be between 1 and 365" },
      { path: "rules[6].scope.category", message: "category must not be empty" },
      { path: "rules", message: "at least one rule is required" },
    ]);
    expect(byRule[draft[5].uid]).toEqual({ days: "must be between 1 and 365" });
    expect(byRule[draft[6].uid]).toEqual({ category: "category must not be empty" });
    expect(other).toEqual(["rules: at least one rule is required"]);
  });
});

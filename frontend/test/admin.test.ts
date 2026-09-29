import { describe, expect, it } from "vitest";

import type { FiredRule } from "@/lib/api-types";
import { traceResult } from "@/lib/admin";

const fired = (kind: FiredRule["kind"], verdict: FiredRule["verdict"]): FiredRule => ({
  kind,
  verdict,
  explanation: "",
  customer_reason: "",
  detail: {},
});

describe("traceResult", () => {
  it("names a flag holding a denial for a person", () => {
    const trace = [fired("final_sale_not_refundable", "denied"), fired("fail_closed", "escalated")];
    expect(traceResult("escalated", trace)).toBe(
      "A rule denies it, but a check flagged the request, so a person decides.",
    );
  });

  it("keeps the other results", () => {
    expect(traceResult("escalated", [])).toBe("No rule applied, so a person decides.");
    expect(traceResult("escalated", [fired("human_review_above", "escalated")])).toBe("One or more checks need a person.");
    expect(traceResult("denied", [fired("refund_window", "denied")])).toBe("A rule denies it; a denial outranks every other rule.");
    expect(traceResult("approved", [fired("damaged_or_incorrect_eligible", "approved")])).toBe("Every rule that applied allows it.");
  });
});

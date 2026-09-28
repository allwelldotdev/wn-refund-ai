import { describe, expect, it } from "vitest";

import {
  alreadyStored,
  customerRequests,
  decisionMessageId,
  fulfilment,
  initials,
  isClosed,
  itemMarkers,
  orderAvailability,
  requestsByActivity,
} from "@/lib/customer";
import { formatShortDate } from "@/lib/format";

import { designConversations, designOrders } from "./fixtures/design";

const byRef = (ref: string) => designOrders.find((o) => o.ref === ref)!;

describe("customer order markers", () => {
  it("lists requests newest first and skips conversations without one", () => {
    expect(customerRequests(designConversations).map((r) => r.request.ref)).toEqual(["REQ-5817", "REQ-5391", "REQ-5790"]);
  });

  it("blocks an order whose only item is waiting for a person (ORD-10421)", () => {
    const a = orderAvailability(byRef("ORD-10421"), designConversations);
    expect(a.blocked).toBe(true);
    expect(a.markers).toMatchObject([{ kind: "review", label: "Under review", request: { ref: "REQ-5817" } }]);
  });

  it("keeps the coffee pickable when only the mug was refunded (ORD-10416)", () => {
    const a = orderAvailability(byRef("ORD-10416"), designConversations);
    expect(a.blocked).toBe(false);
    expect(a.free.map((i) => i.name)).toEqual(["Coffee Subscription (September)"]);
    expect(a.freeCents).toBe(2800);
    expect(itemMarkers(byRef("ORD-10416"), designConversations)).toMatchObject([
      { kind: "approved", label: "Approved after review", item: { name: "Worknoon Mug" }, request: { ref: "REQ-5790" } },
    ]);
  });

  it("blocks a denied item but keeps the rest of its order pickable (ORD-10430)", () => {
    const a = orderAvailability(byRef("ORD-10430"), designConversations);
    expect(a.blocked).toBe(false);
    expect(a.markers).toMatchObject([{ kind: "denied", label: "Denied", item: { name: "Meeting Room (4 hrs)" } }]);
    expect(a.free.map((i) => i.name)).toEqual(["Coffee add-on"]);
  });

  it("closes answered requests but not escalated ones", () => {
    const [escalated, denied, reviewed] = customerRequests(designConversations).map((r) => r.request);
    expect([isClosed(escalated), isClosed(denied), isClosed(reviewed), isClosed(null)]).toEqual([false, true, true, false]);
  });

  it("words a stored fulfilment", () => {
    const base = byRef("ORD-10437");
    const at = (d: string) => `2026-09-${d}T12:00:00Z`;
    expect(fulfilment({ ...base, fulfilment: "confirmed", delivered_at: null, starts_at: at("30") }, formatShortDate).label).toBe("Confirmed, starts Sep 30");
    expect(fulfilment({ ...base, fulfilment: "active", starts_at: at("19"), ends_at: at("26") }, formatShortDate).label).toBe("Active since Sep 19, until Sep 26");
    expect(fulfilment({ ...base, fulfilment: "used", delivered_at: at("19") }, formatShortDate).label).toBe("Used Sep 19");
  });

  it("words fulfilment by what was bought", () => {
    expect(fulfilment(byRef("ORD-10437"), formatShortDate).label).toBe("Delivered Sep 23");
    expect(fulfilment(byRef("ORD-10430"), formatShortDate).label).toBe("Used Sep 20");
    expect(fulfilment(byRef("ORD-10426"), formatShortDate).label).toBe("Confirmed, not started");
    expect(fulfilment(byRef("ORD-10421"), formatShortDate).label).toBe("Active since Sep 17");
  });

  it("drops the in-flight copy only once the thread holds the stored message", () => {
    const messages = [{ id: "m-1" }, { id: "m-2" }];
    expect(alreadyStored(undefined, messages)).toBe(false);
    expect(alreadyStored("m-3", messages)).toBe(false);
    expect(alreadyStored("m-2", messages)).toBe(true);
    expect(alreadyStored("m-2", [])).toBe(false);
  });

  it("lists requests with a specialist's reply first, latest reply first", () => {
    const [first, second, third] = designConversations;
    const replied = [
      first,
      { ...second, last_reply_at: "2026-09-22T09:00:00Z", last_reply_by: "Ngozi Adeyemi" },
      { ...third, last_reply_at: "2026-09-23T09:00:00Z", last_reply_by: "Sam Whitfield" },
    ];
    const refs = (cs: typeof replied) => requestsByActivity(cs).map((r) => r.request.ref);
    expect(refs(designConversations)).toEqual(customerRequests(designConversations).map((r) => r.request.ref));
    const withReplies = refs(replied);
    expect(withReplies.slice(0, 2)).toEqual([third, second].map((c) => c.request!.ref));
  });

  it("marks the last admin message of a resolved request as its decision", () => {
    const messages = [
      { id: "m1", role: "customer" as const },
      { id: "m2", role: "admin" as const },
      { id: "m3", role: "admin" as const },
      { id: "m4", role: "system" as const },
    ];
    const request = customerRequests(designConversations)[0].request;
    expect(decisionMessageId(messages, { ...request, state: "escalated" })).toBeNull();
    expect(decisionMessageId(messages, { ...request, state: "resolved_denied" })).toBe("m3");
    expect(decisionMessageId(messages, null)).toBeNull();
    expect(initials("Ngozi Adeyemi")).toBe("NA");
  });
});

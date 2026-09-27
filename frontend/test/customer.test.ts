import { describe, expect, it } from "vitest";

import { customerRequests, fulfilment, isClosed, itemMarkers, orderAvailability } from "@/lib/customer";
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

  it("words fulfilment by what was bought", () => {
    expect(fulfilment(byRef("ORD-10437"), formatShortDate).label).toBe("Delivered Sep 23");
    expect(fulfilment(byRef("ORD-10430"), formatShortDate).label).toBe("Used Sep 20");
    expect(fulfilment(byRef("ORD-10426"), formatShortDate).label).toBe("Confirmed, not started");
    expect(fulfilment(byRef("ORD-10421"), formatShortDate).label).toBe("Active since Sep 17");
  });
});

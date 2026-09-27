# Changelog

This file records what changed in Worknoon Support for the customers who request refunds and the admins who review them; versions follow semantic versioning.

## [0.1.1] — 2026-09-27
### Improved
- Everything you can click now shows a hand cursor on hover, and unavailable controls show a blocked cursor.

## [0.1.0] — 2026-09-27
### New
- Customers can sign in with any of the seeded demo accounts; repeated failed sign-in attempts pause further tries on that account for a few minutes.
- Customers can see their own orders, newest first, in My orders.
- In the help chat, a customer can pick one of their orders, describe the problem, and get an immediate approved or denied decision with the reason, or have the request handed to a person to decide.
- Your requests lists a customer's past requests, each labelled Approved, Denied, Escalated, or, once a person has decided it, Approved after review or Denied after review.
- Customers can read the refund policy from inside the help chat.
- Reloading the page returns a customer to the conversation they had open, instead of starting over.
- The admin overview shows how many requests came in today, split into approved, denied and escalated, plus how many escalations are still open.
- Admins can browse the full request list, filtered by status, date range or escalation reason, and searched by reference, customer, email or order.
- Opening a request shows its full status timeline, the customer's messages, what was extracted from them, the rule-by-rule reasoning behind the decision, any flags raised, the reply sent to the customer, and the underlying audit record.
- The escalations queue lists open requests oldest first, each with a suggested decision and summary drafted for the admin.
- Admins can approve or deny an escalated request, with a required note explaining the decision.
- Admins can edit the refund policy's rules, with a live preview of the generated policy text before saving.
- Admins can see the policy's full version history and revert to an earlier version.

## Planned
- Approved requests will show Processing and Processed labels once a payment processor is connected; the demo has none today, so labels stay limited to what the system can verify.
- A rule will refuse refunds for items that have already been used, based on evidence the customer uploads, once file storage for photos or documents is added.
- Orders past their refund window will be hidden from the chat's order picker; they stay visible today so the policy is applied in one place only.

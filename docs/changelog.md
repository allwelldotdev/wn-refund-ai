# Changelog

This file records what changed in Worknoon Support for the customers who request refunds and the admins who review them; versions follow semantic versioning.

## [0.13.0] — 2026-09-29
### Fixed
- A request the assistant was unsure about or found suspicious, for example an attempt to instruct the assistant, a mention of someone else's order, an unclear request or clarifying questions that ran out, is no longer denied automatically when a final-sale or expired-refund-window rule applies; it goes to the support team, and the customer is told it needs a closer look from the team. Ordinary requests that a rule denies are still denied automatically.
- Admins now see these requests in Escalations; the request details panel still lists the denial rule and says "A rule denies it, but a check flagged the request, so a person decides.", while a plain denial reads "A rule denies it; a denial outranks every other rule."

## [0.12.1] — 2026-09-29
### Improved
- When a customer asks to refund another customer's order, tries to instruct the assistant, impersonates staff or claims a policy change, or sends instruction-like text, the suggested review in the request details panel now starts its first risk note with "Possible misuse:" and says what happened, for example "Possible misuse: claim on another customer's order."
- When the only order involved belongs to someone else, the suggested review now recommends denying and no longer asks the customer to confirm the order number.

## [0.12.0] — 2026-09-28
### New
- When you ask the assistant again about a request it just reported, it now points you to that request's conversation instead of repeating the status, with an "Open … in Your requests" button that takes you there. A request still with the support team gets a short note that you can follow and message the team in Your requests; a decided request gets its outcome and date, followed by a note that the full conversation is in Your requests. Asking about an order for the first time still gets the normal order status.

## [0.11.0] — 2026-09-28
### Fixed
- The assistant now answers in light of what it just asked. After it reports an order and asks whether you'd like to request a refund on an item, replying "yes", "yes please" or "sure" starts that refund request instead of repeating the order status, and "no thanks" closes the chat politely. Saying "yes" to "Is there anything else I can help with?" brings up your order list.

### Changed
- The order buttons in the chat now appear only when you ask to see your orders, and at the start of a new chat; they no longer appear under the assistant's clarifying questions.

## [0.10.4] — 2026-09-28
### Changed
- The message a customer gets after a specialist decides their escalated request now speaks in that specialist's own voice, for example "Dear Grace, I reviewed your request RR-1001 and denied it because you have not supplied evidence to support your refund claim."; before, it spoke of "a support specialist" in the third person. It still appears under the specialist's name with the Admin tag, with no sign-off. The short chat note that follows it is unchanged.

## [0.10.3] — 2026-09-28
### Fixed
- The message a customer gets after a specialist decides their escalated request now reads as one sentence after the greeting, for example "Dear Amara, a support specialist reviewed request RR-1001 and denied it because …"; before, the word after the greeting was often capitalised as if it started a new sentence. Names, references and "Worknoon" keep their capitals. The admin's preview matches what the customer receives.

## [0.10.2] — 2026-09-28
### Fixed
- On the admin Escalations page, "Message customer" now puts the cursor straight into the message box; before, it landed on the panel's close button. Closing the panel with Escape now returns focus to the "Message customer" button.

## [0.10.1] — 2026-09-28
### Improved
- The automatic reply a customer gets after messaging a request that's already with the support team now says a support agent will respond, not just see the message.

## [0.10.0] — 2026-09-28
### New
- While a request is escalated, admins can message the customer from the escalation card or the request details panel; the message appears in the customer's chat exactly as written, with the admin's name.
- Escalation cards show when the customer has replied and is waiting, with a count of new messages, and move those cards to the top; admins get a notification for a new customer message or a new escalation.
- Customers see a specialist's message in the chat tagged with the admin's name and an Admin tag.
- The decision message in the chat now carries an "Approved after review" or "Denied after review" tag.
- When a specialist replies about a request the customer isn't looking at, a banner with View and Dismiss appears in the chat, and "N new" shows on the Your requests tab and on the closed help button.
- An escalated request's details now include a box for customers to reply to the specialist.

### Improved
- The request details panel now shows the whole conversation, including admin messages, with a Collapse / Show all button.
- Your requests now lists requests with a reply first and marks new ones.

### Changed
- Once a specialist has written on a request, the assistant no longer answers the customer's messages on it; before that, it answered only once.

## [0.9.1] — 2026-09-28
### Improved
- The assistant's chat replies are now shorter and more to the point, professional and courteous, without apologies or filler.
- The message customers receive after an admin decides their escalated request is now one or two short sentences explaining the decision, without a sympathy line.

## [0.9.0] — 2026-09-28
### Changed
- Once the assistant has the order, item and problem it needs, it now asks once whether there is anything else to add before checking the request; the decision follows whatever you answer, including "no, that's all", and anything you add is taken into account. Requests that look like manipulation attempts still go straight to a person without this question.

## [0.8.0] — 2026-09-28
### New
- Saying hello to the assistant now gets a short welcome instead of being told it can only help with refunds.
- Customers can ask the assistant what happened to an order or an earlier request, by order number or by item name; it reports when the order was placed and delivered, each item's amount, and any refund request and its status, and offers to file one for items that don't have one yet.
- Asking to see your orders brings up the order list in the chat so you can pick one to ask about.

## [0.7.2] — 2026-09-28
### Fixed
- Once an admin decides an escalated request, the customer now sees it within a few seconds without reloading: the status tag in Your requests and the request details changes to "Approved after review" or "Denied after review", the admin's message appears in the chat, and the message box is replaced by the "Start new request" button. Before, the tag stayed "Escalated" until the page was reloaded.

## [0.7.1] — 2026-09-28
### Fixed
- A new chat's first message no longer briefly appears twice while the assistant is working on it.
- Switching from Chat to Your requests and back no longer loses a new conversation; the chat keeps its messages, a reply still arriving, the picked order and the scroll position. "Start new request" still starts an empty chat.

## [0.7.0] — 2026-09-27
### New
- Customers can add a test order from My orders, picking a date, items from a catalogue, and a delivery status, to try the chat.
- Before adding a test order, the form shows what the assistant would likely decide for each item.
- Added test orders are tagged "Test" and appear first in My orders.

### Improved
- My orders now shows a start date for a confirmed order and an end date for an active one.

## [0.6.1] — 2026-09-27
### Improved
- Suggested reviews on escalations are now short and to the point, easier for admins to scan.

### Changed
- The request details panel's "Policy & model" section is now "Policy & processing" and no longer shows AI names.

## [0.6.0] — 2026-09-27
### New
- Once an admin decides an escalated request, the customer sees a chat message explaining why.
- The request details also show a short summary note of the admin's decision.
- Admins write a note, preview the customer's message, then send it to resolve the request.
- If the assistant can't draft the message, the review stays open and the admin's note is kept.

## [0.5.0] — 2026-09-27
### New
- Customers can dispute an automatic denial once from Your requests, with an optional reason.
- Admins can turn customer disputes on or off in a new Settings page.

### Improved
- Disputed requests are tagged, filterable and shown in the request timeline.

### Changed
- Decisions made by an admin can't be disputed.

## [0.4.0] — 2026-09-27
### New
- Once a request is approved or denied, the chat replaces the message box with a "Start new request" button; a request still with a specialist stays open so you can add details.
- Your requests now opens each request as a read-only chat that shows its status and says when a decision is final.
- In My orders, "View refund request" now opens that request directly.

### Fixed
- A message typed in a new chat no longer reappears as a draft in the next new chat after it was sent.

### Improved
- Items that already have a request can no longer be picked again in the chat, and show their status (Approved, Denied, Under review, or "… after review").
- The chat shows five orders at first with a "Show more" option, and picking an order shows any earlier request filed on it.
- My orders now shows ten orders per page.

## [0.3.0] — 2026-09-27
### New
- Asking about an item that already has a request now shows that request's status instead of opening a second one.

### Changed
- The chat now politely declines anything that isn't a refund request, including requests to look things up online.
- Replies are warmer and briefly acknowledge the customer's situation.
- Saying you need nothing else ends the chat with a short thank-you, without filing a request.

## [0.2.0] — 2026-09-27
### Changed
- The chat now asks a clarifying question for a vague message instead of handing it to a person right away, unless it looks suspicious.

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
- The assistant will remember a customer's orders and past conversations so it asks fewer repeat questions, conversations are shorter and replies are more personal.
- Approved requests will show Processing and Processed labels once a payment processor is connected; the demo has none today, so labels stay limited to what the system can verify.
- A rule will refuse refunds for items that have already been used, based on evidence the customer uploads, once file storage for photos or documents is added.
- Orders past their refund window will be hidden from the chat's order picker; they stay visible today so the policy is applied in one place only.

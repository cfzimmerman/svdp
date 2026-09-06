---
name: recording-svdp-deliveries
description: Records St. Vincent de Paul food and grocery gift card deliveries in ServWare. Use when someone says they made deliveries, delivered to families, or needs to mark requests complete or log assistance.
metadata:
  conference: Nativity Catholic Church, Menlo Park
---

# Recording SVdP deliveries

You are helping a St. Vincent de Paul volunteer record a delivery day in ServWare.

**Who you are talking to.** Volunteers, often elderly, who are not computer users. They have
just spent a morning packing food and driving around. Use short sentences and plain words.
Never show them a request id unless they ask, never mention JSON, tools, or errors by their
technical name, and never ask them to retype something you can already see.

**What is at stake.** These entries are the county's record of assistance given to families in
need, and they involve real money. ServWare has no way to delete an assistance item once it is
added, so a mistake has to be fixed by hand in the website. **Confirmation before writing is
the only safety net there is.** Never skip it, and never assume.

## Before you start

Run `servware_health`. If it reports a problem, stop and tell the volunteer in plain words
that the system needs attention, then say what it reported. Do not continue.

If it says the username and password are not set up, follow
`references/first-time-setup.md`.

## The conversation, step by step

### 1. Find out what happened today

Run `list_open_requests` and show the families as a simple numbered list — name, household
size, how long they have been waiting, and the gift card amount that household size calls for.

Then ask: **"Which of these did you deliver to today?"**

Let them answer however is natural — names, numbers, "the first four", "everyone except the
Garcias". Read it back as a list and ask if you have it right. Families they did not reach
are simply left out; do not ask them to explain why.

### 2. Check the dollar amounts

Show the families they delivered to with the calculated gift card amount for each, and ask:

> "Are these amounts right? If any were different, just tell me which ones."

Most weeks nothing changes. When they name an exception, change only that one and read the
corrected list back. Amounts come from household size, but **the volunteer's number always
wins** — they know what card they actually handed over.

Food is logged at the standard **$70** per delivery. Say that amount out loud once, as part of
the summary, so it is never a surprise — for example *"and $70 of food for each, as usual"*. If
the volunteer says a delivery's food was worth something different, use their number. This
almost never happens, so do not ask about it for each family.

### 3. Split the deliveries by volunteer

Ask: **"Who drove today?"** Usually one to three groups.

For each group, ask which families that group delivered to. Match the names they give against
`list_volunteers` and confirm the match out loud — *"I have Pat Nguyen, is that right?"* — before
using it. If a name is ambiguous, ask; never guess between two volunteers.

Every delivery must end up in exactly one group.

### 4. Show the whole plan and get a clear yes

Show a summary per group: the volunteer, the families, each gift card amount, the food amount,
and the total dollars. Then ask for confirmation in a way that cannot be answered by accident:

> "I'm about to record all of this in ServWare. Once it's in, it can't be undone from here.
> Should I go ahead?"

Wait for a clear yes. Anything hesitant means stop and ask what to change.

### 5. Record it

Submit the plan. For each family, three things are recorded: the food value, the gift card
value, and marking the request complete and credited to the volunteer.

While it runs, say what is happening in plain terms. When it finishes:

- **All good** — say how many families were recorded and the total logged.
- **Something did not go through** — say plainly which families are affected and what it says.
  Do not retry silently. Nothing is lost; recording can be picked up again.
- **Someone else changed a request** — say so and ask what they want to do. Never overwrite.

## Rules that do not bend

- **Never write anything before an explicit yes** at step 4.
- **Never invent a family, an amount, or a volunteer.** Every one comes from a tool result or
  from something the volunteer told you.
- **Never re-run a recording to "make sure it went through."** Recording is safe to resume: it
  checks what is already in ServWare first. Ask for the current state instead of guessing.
- **Never show a raw error.** Say what happened and what it means for them.
- **If you are unsure, ask.** A confused question costs a moment. A wrong entry costs a family's
  record and someone's afternoon fixing it.

## If something goes wrong partway

Recording is resumable and knows what it already did, so nothing gets recorded twice. Tell the
volunteer which families still need recording and offer to continue. If they would rather stop,
say clearly what was recorded and what was not, so they can note it.

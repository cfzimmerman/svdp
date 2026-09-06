---
name: pulling-svdp-data
description: Pulls St. Vincent de Paul neighbour information from ServWare into spreadsheets. Use when someone wants a list of families, household ages, or to analyse who the conference serves.
metadata:
  conference: Nativity Catholic Church, Menlo Park
---

# Pulling SVdP data into spreadsheets

You are helping a St. Vincent de Paul volunteer get information out of ServWare so they can
plan a project — a Christmas Adopt-a-Family list, a mailing, a summary for the conference.

**Who you are talking to.** Volunteers, often elderly, who are not computer users and do not
think in spreadsheets. Several have said they get by on "basic Excel". Never mention JSON,
joins, tool names, or client ids unless they ask. Talk about families, not records.

**What this skill is for.** Getting the data out. The thinking — filtering, counting, sorting,
working out who qualifies — happens here in the conversation, on the files you pull. If a
question can be answered from a spreadsheet you already have, do not pull again.

**What you must not do.** Nothing here writes to ServWare. If a volunteer starts talking about
recording a delivery, that is a different job with different safeguards; say so and stop.

## Before you start

Run `servware_health`. If it reports a problem, say so in plain words and stop. If it says the
username and password are not set up, follow `references/first-time-setup.md`.

## The three spreadsheets

Everything is pulled into CSV files on the volunteer's **Desktop**. Each one answers a different
kind of question, and they line up with each other by household.

| Tool | File | One row per | Speed |
|---|---|---|---|
| `export_neighbors` | `svdp-neighbors-<date>.csv` | household | fast |
| `export_requests` | `svdp-requests-<date>.csv` | request for help | fast |
| `export_household_members` | `svdp-household-members-<date>.csv` | **person, with their age** | slow |

Every file has a `client_id` column. That is the same household in all three, which is how you
put them together.

**Pull only what the question needs.** Most questions need one or two files. Ages are the only
reason to run the slow one.

## Reading them back

The export tools save the files and tell you the row count; they do not hand you the contents.
When you actually need to work with the data, call `read_export` with the file name. With no
file name it lists what is already on the Desktop — check there before pulling anything again.

**Do arithmetic with code, not by eye.** Counting families, finding the youngest child, adding
up dollars — write it out and compute it. A miscount here puts a real family on or off a list.

## Things about this data that will trip you up

These are verified against the live system. Do not assume otherwise.

- **The household members file does not include the neighbour themselves.** Every row is
  somebody *else* in the house — Son, Daughter, Spouse, Mother, Cousin and so on. There is no
  "Self" row. So a household's size is **the number of rows plus one**.
- **Not every household has members recorded.** ServWare lets a conference enter either a head
  count *or* the individual people, not both. Households that only have a head count produce **no
  rows at all** in the members file, so their children's ages are simply not in the system. In a
  recent three-month window that was 19 households out of 158.
  **Never let those families silently vanish from an answer.** Report them as a separate short
  list — "these N families have children but no ages recorded, you would need to check them by
  hand" — which is exactly the work this tool is meant to save, so it is worth saying out loud.
- **A few people have no age recorded** even when their household is listed. Treat a blank age as
  unknown, never as zero.
- **`calculated_child_count`** on the requests file counts children without saying how old they
  are. It is useful for spotting the households above, not for filtering by age.

## Talking about a slow pull

`export_household_members` opens one page per household, so it takes a minute or two and Claude
will ask the volunteer to approve it. Before calling it:

1. Ask for a date range if they have not given one. "Which months should count as recent?"
2. Say roughly how long it will take and why: *"I have to look up each family one at a time, so
   give me a minute or two."*
3. If it comes back saying there are too many households, do not raise the limit on your own —
   offer a shorter date range first.

## Finishing

Tell them plainly: which files are on their Desktop, what is in them, and the answer to what
they actually asked. Offer to save the answer as its own spreadsheet they can send on.

Say once, without lecturing, that the files hold real family information and should stay on
their own computer.

## Recipes

- `references/adopt-a-family.md` — the Christmas Adopt-a-Family list: active households with a
  child aged 13 or under, with name, address, phone, and the two youngest children's ages.

# Adopt-a-Family list

**The ask.** Households that are active with the conference and have at least one child aged
**13 or younger**, with name, address and phone numbers — plus the ages of the youngest and
second-youngest child, which is the part nobody could get before.

This used to be done by hand: export ServWare's Neighbor Assistance Summary Report, wrestle it
into Excel, then open each family's record one at a time and type the two ages into two columns.
That is the work this replaces. Everything else about the spreadsheet stayed the same, so keep
the output recognisable.

## Ask first

1. **What date range counts as "active"?** Last year it was three months. Offer that as a
   starting point — *"shall we count families who asked for help in the last three months?"* —
   but use whatever they say.
2. **Is 13 still the cut-off?** Programs change their age bands. Confirm rather than assume.

## Pull

Two files, in this order:

1. `export_requests` with their date range — this is who counts as active.
2. `export_household_members` with the same date range — this is the ages. Warn them it takes a
   minute (see the main skill).

`export_requests` writes two files — one per request, one per item of help given. You do not need
`export_neighbors` unless they want the head of household's age, email, or the language a family
speaks; the requests file already carries name, address and both phone numbers.

If they ask how much a family has received, total `monetary_value` in the **assistance** file by
`date_provided`. That is how the conference's own summary report counts it, and it will not match
a total taken from request dates.

## Work out the answer

Read both files back and compute — do not eyeball this.

1. Group the member rows by `client_id`. Ignore rows with a blank age.
2. For each household, sort the ages. The **youngest** is the smallest; the
   **second-youngest** is the next one, or blank if there is only one person listed.
   These two columns are a convention carried over from her old spreadsheet, not a limit on the
   data. **If a household has more than two children, say so and give all their ages** — the
   previous sheet had room for four and larger families lost the rest.
3. Keep households whose youngest is **13 or under**.
4. Attach name, address and phones from the requests file, and count how many requests each
   household made in the window.

Remember: **there is no "Self" row**, so every age you are looking at is somebody other than the
head of household. You do not need to exclude anyone. In practice a household's youngest listed
person is a child, but if an age looks wrong for its relationship — a "Mother" aged 9 — say so
rather than quietly using it.

## Report both lists

**The candidates.** One row per household: last name, first name, address, city, home phone,
mobile phone, number in family, number of children, requests in the window, youngest child's
age, second-youngest child's age. Offer to save it as a spreadsheet.

This mirrors the spreadsheet she built by hand, with two differences worth mentioning: her old
sheet had room for four children per household and some families have more, and it had no column
that ties a row back to ServWare, which is why matching families across sheets used to be manual.
Every file here carries `client_id` for exactly that.

**The families you could not answer for.** Households active in the window that have children
according to `calculated_child_count` but produced **no rows** in the members file, because
ServWare has only a head count for them. List these separately, by name, and say plainly:
*"these families have children but nobody's ages are recorded in ServWare, so they would need
checking by hand."* In a recent three-month window this was 8 households.

Do not merge the two lists and do not leave the second one out. A family left off a Christmas
list because a spreadsheet was quietly incomplete is the worst outcome this tool can produce.

## Do not put a price on anyone

This list says who to shop for. It says nothing about how much to spend, and neither should you.
The weekly delivery amounts do not carry over to Christmas; that scale is set by whoever runs the
program. If they ask for help budgeting, work from numbers they give you.

## Sanity check

Before handing it over, check a couple of rows against ServWare in the browser. This is the
first time anyone has trusted these ages without typing them, and it is worth one minute of
confirmation the first time it is run each year.

# Decision log

Why this project is shaped the way it is. Entries record the reasoning and the **rejected
alternatives**, because a decision without its rationale gets re-litigated and a discovery
without its evidence gets re-derived.

Append new entries as work proceeds. Note explicitly when a finding overturns an earlier
claim — the correction is the valuable part.

---

## D1. Target the Claude Desktop **Chat tab** via a `.mcpb` extension, not Cowork

*September 2026*

The goal is a tool elderly volunteers can use without a terminal. Cowork looked like the
obvious home — it is the agentic, file-oriented surface and takes skills as a simple zip
upload. Three findings ruled it out:

1. **Its VM is per-session.** Nothing installed survives; only explicitly-connected host
   folders persist. A "setup skill installs Rust and builds from source" approach would pay a
   293-crate cold build *every session*, over a VirtioFS mount.
2. **It cannot reach the OS keychain.** The VM is a guest Linux environment with no host
   process access, so credentials would live as plaintext in a folder handed to Claude.
3. **Decisive: sandboxed code has no general network egress.** Egress is an account/org
   setting defaulting to a fixed package-manager allowlist (npmjs, PyPI, crates.io, github).
   Reaching `servware.org` requires a custom domain allowlist that appears to be admin-only on
   Team/Enterprise. A skill cannot request it. Cowork simply could not talk to ServWare.

An MCP server delivered as a `.mcpb` runs on the **host**, so it has ordinary network access
and the egress question never arises. Installation is one click, and the volunteer's password
goes through a proper form field rather than the chat.

Cowork is a tab *inside* Claude Desktop, not a separate product — worth stating because the
two were initially conflated.

**Consequence:** skills supply the *procedures*; the MCP server supplies the *capabilities*.
Skills work in both Chat and Cowork (they share one list), so the suite is unaffected.

## D2. Stay in Rust rather than porting to TypeScript

*September 2026*

Anthropic's docs and examples centre on Node, and Claude Desktop bundles a Node runtime, so a
JS server would be one file that works on every platform with no compiled binary. That was the
only real argument for switching, and it was a *packaging* argument, not a language one.

Against it:

- **Dependency churn.** The stated concern was JS bitrot. A TS port needs the MCP SDK, zod, a
  cookie jar (Node `fetch` will not persist `Set-Cookie` across the login 302 on its own), and
  an HTML parser — cheerio alone pulls dozens of fast-moving transitive packages. A pinned
  `Cargo.lock` over a small crate set ages far better for a tool that runs twice a week for
  years.
- **Correctness.** The core is a form-overlay engine and a per-delivery state machine guarding
  real money. Exhaustive matching over write outcomes and error kinds is what makes "did this
  write land?" checkable rather than hopeful.
- **Maintainer fluency.** Rust is the maintainer's native language; that is a first-order
  maintainability property.

The packaging objection then dissolved entirely — see **D8**.

## D3. The MCP server is the primary UI; the CLI is the maintainer's tool

*September 2026*

Previously the CLI was the product. It now has two front ends over one logic layer, which
preserves the logic/UI separation that was explicitly worth keeping. The CLI stays useful for
debugging, fixture recording, and as a fallback during the first live delivery night.

## D4. Credentials via `user_config` with `sensitive: true`

*September 2026*

Rejected: pasting into chat (enters the transcript), a `.env` file the volunteer hand-creates
(too fiddly for the audience, and plaintext), and reading the OS keychain directly (impossible
from Cowork; unnecessary here).

The manifest declares the field; Claude Desktop collects it at install, stores it encrypted,
and injects it into the server process as an environment variable. **The model never sees it.**
Verified empirically — see **D9**.

## D5. Extract whole forms and overlay; never enumerate fields

*September 2026*

The live request edit form renders **50 named controls**. The hand-written builder in
`update_request.rs` sent 39 and silently cleared the other 11 on every `mark-complete`:
`otherVisitCnt`, `eldercareVisitCnt`, `hospitalVisitCnt`, `prisonVisitCnt`, `phoneVisitCnt`,
`churchPantryVisitCnt`, `referralOrganizationId{,2,3,4}`, `referralConference`. Six of those
are visit counters, so past runs have been zeroing the conference's visit statistics in the
county's records.

`src/servware/form.rs` parses the form, reproduces the browser's successful-controls rules, and
applies a *checked* overlay. Overlaying a control the form does not render is a hard error —
the canary for ServWare renaming a field. It is also **less code** than the builder it replaces.

Guards: a diff assertion proves an update changes only what it names, and the same diff is what
a human is shown before submission.

## D6. Server-side session state, not conversational state

*September 2026*

A delivery plan is persisted as a file; only `session_id` and `revision` cross the conversation.

Reasons, in priority order: the confirm gate must bind to **persisted bytes** rather than to
what the model claims it displayed; state survives a Claude Desktop restart or the user closing
the window mid-flow; and re-emitting twenty rows of dollar amounts through the model every turn
is a transcription-error surface with real money behind it.

Related: one **whole-plan mutator** rather than incremental edit tools. Models emit a complete
structured object from a conversation reliably and maintain long sequences of incremental edits
badly, and whole-object replace is naturally idempotent under retry.

## D7. Idempotency: tag `notes`, read the detail page, reverse the write order

*September 2026*

`add_assistance` was not idempotent — each run appended two more items, double-logging $70 plus
the gift card. Fixing it needs a key for "already submitted", and **no natural key works**:
`(request, type)` blocks legitimate re-delivery next month; `dateProvided` is a date, so a retry
across midnight duplicates; adding `monetaryValue` turns a $70→$80 correction into a double-log.

So a machine tag goes into the otherwise-empty `notes` field. It is exact, survives all three
cases, and is greppable in ServWare.

Two supporting changes. Reads come from the **detail page**, which is status-agnostic — the list
API filters to `Open`, so it goes blind precisely when recovery is needed. And the write order
reverses to food → gift card → mark-complete, making completion the commit marker.

Governing invariant: **ServWare is the source of truth; the local receipt is an optimization.
Every write is preceded by a fresh read-check, even on the first attempt.**

This matters more than it looks: `api.md` documents no delete or edit endpoint for assistance
items, so a mistaken entry is permanent from this tool's perspective. Preview-and-confirm is
therefore the only safety mechanism in the system, not a UX nicety.

## D8. Gatekeeper does not block a Rust binary in a `.mcpb` — verified

*September 2026*

This was the main risk to D2 and was carried unresolved through the whole design conversation.

Method: built the arm64 binary on a real Mac, packaged it, made two byte-identical bundles, and
stamped one with Safari's `com.apple.quarantine` attribute to simulate a browser download.

- Direct `ditto` extraction **does** propagate quarantine, and a quarantined ad-hoc-signed
  binary is killed on exec (`spctl` verdict: *rejected*). Rust's default Apple Silicon signature
  is `adhoc, linker-signed` with no TeamIdentifier.
- But **Claude Desktop's own extractor does not propagate it.** Both bundles installed and ran.

**Therefore no Apple Developer ID and no notarization are required**, and distribution by
GitHub release download works. Re-verify if Claude Desktop changes how it unpacks bundles.

An earlier A/B on this was contaminated: once the first exec triggered Gatekeeper evaluation
with no GUI session to answer it, later runs against the same inode stayed poisoned. Two
pristine extractions were needed to get a clean result.

## D9. Empirical findings that contradict documentation or intuition

*September 2026*

- **`api.md` is partly abridged.** Its assistance-item section lists 13 form fields; the real
  browser POST sends **30**, in exactly the order `update_assistance.rs` already used. An
  earlier claim that the code diverged from the capture was **wrong** — the code was right and
  the doc was stale. Where a capture and `api.md` disagree, the capture wins.
- **`id` and `name` diverge** in the form: `id="homeVisitAssignedFirst"` carries
  `name="visitAssignedToMemberId"`. Always key on `name`. The existing member scrape works only
  because one control happens to have matching values.
- **No CSRF token.** All 14 hidden fields are accounted for.
- **Assistance items are server-rendered** in the detail page; the only XHRs it fires are
  `files/list` and `approval/list`. This was the open question that could have invalidated D7,
  and it holds.
- **Unchecked checkboxes are rendered but not submitted.** This broke the first cut of the
  overlay: the "field removed" canary fired on every unchecked box, so `visitCompleted` could
  never be turned on. Fixed by keeping the full control inventory separate from the submitted
  subset.
- **Extraction must be form-scoped.** `sendToRoleId` and `clientMapBoundaryScope` belong to a
  send-email modal elsewhere on the page.
- **An installed extension starts disabled** and must be toggled on before tools appear.
- **`user_config` requires `description` on every entry**, and `manifest_version` must be
  `0.3`/`0.4`. Omitting `description` fails install with an opaque "Required, Required".

## D10. Test strategy: strict deserialization and a live check, not more fixtures

*September 2026*

The regression most worth defending against is "ServWare changed", which already happened once.
No offline test catches it — snapshots, canned responses, and hand-written fakes are all frozen
copies of the old world.

Two things do:

1. **Strict deserialization of the ~10 depended-upon fields.** Blanket `#[serde(default)]` plus
   null-stripping turns a renamed `calculatedHouseholdCount` into `0`, which the gift-card
   ladder maps to **$50 for every family**. Making those fields required converts a silent money
   bug into a loud parse failure, and it *removes* code.
2. **A live read-only `doctor` / `servware_health` check** run as step 0 of every skill
   invocation, including that assistance type IDs still resolve to their expected names.

Offline tests still earn their place for the money bugs — form round-trip identity, idempotency
and resume, required-field strictness, the gift-card ladder, pagination completeness, and
write verification. Ranked by value per unit of maintenance.

**Rejected:** `proptest` (the gift-card ladder is a six-arm `match`; property-testing it is
theatre), `wiremock` (redundant with an `axum` fake and cannot do stateful, which is what
resumption tests need), and live-ServWare CI. Learning surface is maintenance cost.

## D11. Typed errors at the library boundary

*September 2026*

This **reverses** an earlier rule in `CLAUDE.md` that said `anyhow` everywhere and no custom
error types. That was right for a pure CLI, where every error ends up as text for a human.

The MCP layer must *match* on error kind to choose retry / skip-as-already-done /
report-conflict / tell-the-human-the-page-changed, and `anyhow::Error` cannot be pattern-matched.
`thiserror` types at the boundary, `anyhow` in binaries.

## D12. Fixtures are synthetic by construction, never scrubbed

*September 2026*

The repo is public and the data concerns vulnerable families; the detail page carries names,
addresses, phones, **SSN last-4**, driver's licence, and full request history. A leak is
permanent.

Sanitizing a real capture and committing the output was rejected as not conservative enough — a
scrubber fails open on every field nobody thought of. Instead `scripts/gen-fixtures.py`
generates fixtures from invented values. Field *names* and control types are protocol facts and
carry no personal data; every *value* is fabricated.

Real captures are still used, through an **opt-in local test** reading a path from
`SVDP_LOCAL_DETAIL_HTML` that asserts structure and prints no values. That is how D5 and the
unchecked-checkbox bug in D9 were validated.

`.gitignore` matches by glob (`*.csv`, `*.har`). The previous exact-name list missed the
`group1.csv` files the documented workflow instructed users to create — a live leak vector.

## D13. First live write, verified end to end

*September 2026*

One real delivery recorded against production ServWare (request 4311091, credited to volunteer
37629), chosen by the maintainer rather than picked from the list by the tool. Sequence: food
$70 → gift card $60 → mark complete, each confirmed by re-reading the record.

What it established, beyond "it works":

- **Idempotency holds against the real system.** Re-running the identical food write returned
  `AlreadyDone` and left the item count at two. The double-charge defect that motivated the
  rewrite is fixed in production, not only against an in-memory fake.
- **Completion preserves what it does not touch.** The diff showed exactly 9 fields changed and
  **42 preserved**. The old builder would have blanked 11 of those 42 on this very write.
- **A volunteer's amount overrides the ladder.** The household of three computes to $70; $60 was
  recorded because that is what the volunteer wrote down by hand. The computed figure is a
  suggestion, never an authority.

Caveat carried forward: `mark_complete` writes `requestAssignedToMemberId`, overwriting the
county's intake assignment. It was empty on this request so nothing was lost, but the behaviour
should be revisited — preserving a non-empty value is probably correct.

## D14. `clientId` exists only in the list API, which forces the write order

*September 2026*

Adding an assistance item needs `clientId`. It appears **nowhere on the request detail page** —
not in the edit form, not in any modal, not as a stray hidden input. Its only source is the list
endpoint's `client.id`.

The list endpoint is filtered by status, so a completed request cannot be found there. Two
consequences, both structural:

1. **Assistance must be written before completion**, independently of the idempotency argument in
   D7. Completing first makes `clientId` unreachable and the assistance write impossible. This was
   discovered the hard way: a post-completion retry failed with "could not determine the client
   id".
2. **The session plan must capture `client_id` at plan time**, while the request is still open,
   and carry it through submission. It does, which is why the flow is unaffected — but a future
   refactor that tries to re-derive `client_id` at submit time would break for exactly the
   requests that most need a retry.

The CLI gained an explicit `--client-id` override, since a maintainer re-running against an
already-closed request has no other way to supply it.

## D15. One bundle per platform, built in CI

*September 2026*

A `.mcpb` carries a compiled binary, so it is a platform-specific artifact — one bundle per
target, not a universal one. Targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-unknown-linux-gnu`.

Built by GitHub Actions rather than by hand. Hand-building meant the artifact depended on the
maintainer's laptop being awake and on whatever was in his working tree — during development a
stale bundle was once deployed over a failed build without anyone noticing. CI also gates the
things that are easy to lose: `cargo test`, `clippy -D warnings`, and a check that no CSV or HAR
is tracked and no fixture carries a phone number outside the reserved `555-01xx` range.

The Intel Mac target matters more than it looks. The first bundle shipped was arm64-only; on an
Intel Mac the server would simply fail to start, and the failure surface inside Claude Desktop is
an unexplained "server failed to start". Volunteers on older Macs are exactly the audience here.

No signing step, per D8.

## D16. The conference policy is compiled in

*September 2026*

`conference.toml` is embedded with `include_str!` and parsed into the default config.

An MCP server is spawned by Claude Desktop with an undefined working directory, so the previous
"read `conference.toml` from the current directory" silently found nothing and fell back to
hard-coded values. The config was effectively inert — worse than not having one, because it
looked configurable.

Embedding means a shipped binary always has a valid, reviewed policy, and a malformed edit fails
at build time rather than at a volunteer's keyboard. A test asserts the embedded file parses and
still matches conference practice, since it now decides real money.

An external file still overrides it — `SVDP_CONFERENCE_CONFIG`, else `conference.toml` in the
working directory — which is what another conference would use. A malformed override is ignored
with a warning rather than half-applied.

## D17. The completion write preserves the county's intake assignment

*September 2026*

`mark_complete` used to set `requestAssignedToMemberId` unconditionally. That field records who
at the county took the request at intake; the delivery volunteer belongs in
`visitAssignedToMemberId`.

It is now claimed only when empty. On the first live write it happened to be empty so nothing was
lost, but on a county-assigned request the old behaviour would have destroyed their record — and
unlike the assistance items, nobody would have noticed.

## D18. How a volunteer starts the workflow each week

*September 2026*

Three entry points, in order of how little the volunteer has to do.

**A desktop shortcut, via a deep link.** `claude://claude.ai/new?q=<url-encoded prompt>` is a
documented scheme that opens Claude Desktop into a new chat with the prompt already filled in
(limit ~14,000 characters). `scripts/make-shortcut.sh` writes a `.webloc` on macOS or a
`.desktop` on Linux. The volunteer double-clicks an icon and presses Enter — no typing, nothing
to remember, no menu to find. This is what the setup guide should lead with.

**The `/` menu.** Typing `/` in the chat box lists available skills to pick from. Fine as a
fallback, but it means typing and scanning a list.

**Natural phrasing.** Skills are model-invoked by matching the `description` against what the
user says. The description was rewritten to the documented best practice: third person, concrete
keywords, and an explicit "use when" clause naming phrases a volunteer actually uses
("made deliveries", "delivered to families", "mark requests complete"). Reliable, but least
predictable for someone who phrases things unexpectedly.

**Rejected:** Projects (documented as a Cowork feature, with no documented way to attach a skill
or extension, and Cowork is ruled out by D1); saved or pinned prompt templates (no such feature
is documented for the consumer Chat surface).

**MCP prompts are implemented but not relied upon.** The server advertises three
(`record_deliveries`, `who_is_waiting`, `finish_saving`) and they list and resolve correctly.
Whether Claude Desktop surfaces MCP prompts in its UI is **not documented** — the capability is
described for the Agent SDK, not the consumer Chat client. They cost little, and if the client
does surface them they are the shortest path of all; until that is confirmed, the desktop
shortcut is what the guide should say.

## D19. Household ages come from the request detail page, not from a report

*September 2026*

A volunteer organising the Christmas Adopt-a-Family program needed active households with a
child aged 13 or under. She could already export ServWare's Neighbor Assistance Summary Report,
but it does not carry the children's ages, so last year she opened each household's record and
typed the youngest and second-youngest child's age in by hand.

**The ages were already in a page this tool downloads on every delivery.** The request detail
page is a six-tab Bootstrap layout and every pane is server-rendered inline in the same ~200 KB
response. `div#tabs-familymembers` holds a table of `First Name · Last Name · Relationship · Age
· Phone · SSN (Last 4) · Drivers License/ID · Disabled · Notes`. `Age` is an integer ServWare
computes server-side; there is no per-member birthdate on the page at all.

**Rejected: the report routes.** The detail page's nav menu leaks about two dozen of them,
including `clientrequestsummariesrpt` — her report. Every one is an unknown quantity: unknown
parameters, unknown response format, unknown whether any exports. The same table is
reconstructable from `/app/clients/list` plus `/app/assistancerequests/list`, both already
understood, with columns we choose. They are recorded in `api.md` §12 as existing and
deliberately unexplored.

**Rejected: the neighbour detail page.** `/app/clients/{id}` looked like the natural home for a
household roster, and would have avoided joining through a request. Spiked with one live GET:
its `tabs-family` pane renders only the two head-count inputs plus an empty
`<div id="familymember-list"></div>` filled by XHR. It would cost two requests per household and
an endpoint never captured. The request detail page serves the same data in one page we already
parse, so households are reached through their most recent request.

**Two facts verified against 139 real households, both of which change how the data must be
read:**

* **The members table excludes the neighbour themselves.** No row ever carries a `Relationship`
  of "Self" — observed values are Son, Daughter, Spouse, Domestic Partner, Mother, Father,
  Brother, Sister, Cousin, Grandson, Granddaughter, Niece, Nephew, Stepson, Uncle, Grandparent,
  Other. `calculatedHouseholdCount` equalled the row count **plus one** in all 139 cases. A
  household size derived from these rows must add one, and there is no "Self" row to filter out.
* **A household may have no roster at all.** ServWare's own tooltip says "Enter household adult
  and child counts or enter specific household member details. Only one or the other is
  allowed." Households recorded by head count produce no rows, so their children's ages do not
  exist in the system. In a 06/01–08/31 window that was 19 of 158 households, 8 of which have
  children. This is not a parser bug and it must never be silently dropped from an answer — the
  skill requires reporting those families as a separate list for manual checking.

## D20. The extension extracts; ordinary Claude analyses

*September 2026*

Asked to support projects beyond deliveries, the instinct is to build the project in. That is
rejected. The extension pulls a slice of the neighbour population into CSVs and stops.

The reasoning is that the hard part is extraction, not analysis. ServWare is behind a login form
with no public API, so nothing but this code can get the data out. Once the data is a CSV, plain
Claude is already good at the rest — filtering, grouping, joining, and emitting a new spreadsheet
are things it does without any help from us.

So there is **no query language, no filter DSL, and no report-specific logic in the Rust.** The
export layer produces the three natural grains of the data — household, request, person — each
carrying `client_id` so they can be joined. Adopt-a-Family is then a paragraph in a skill, and
next year's different question needs no code change.

**Where overfitting belongs is markdown.** `skills/pulling-svdp-data/references/adopt-a-family.md`
is deliberately specific to one program, down to the age cut-off and the output columns, because
next December it gets edited in a text editor rather than recompiled.

**Consequence for the tool surface:** exports return a path and a row count, never rows. A
separate `read_export` hands the CSV text over when someone actually asks for analysis, so
neighbour data enters a conversation only on purpose. It takes a bare filename — separators,
`..`, and absolute paths are refused rather than normalised — so it can only read files this tool
wrote.

## D21. Export columns are an allowlist, and ages are emitted where dates of birth are not

*September 2026*

`/app/clients/list` returns the entire Client record regardless of which columns are requested —
about sixty fields, including `ssnLastFour`, `driversLicenseId`, `identificationType`,
`idExpirationDate`, `caseNumber`, `notes` and `alertNote`. The household members table sits
beside the ages in the same DOM.

Each export's header set is therefore a `const` in `domain::export`, **pinned by a test** that
asserts the emitted header equals it exactly and that no column name matches an identity or
free-text pattern. A new ServWare field cannot add a column by accident; adding one is a
deliberate edit to a visible list.

Never emitted, from any table: SSN, driver's licence, other identity documents, case notes,
alert notes, and **dates of birth**. The last is the substantive choice. Ages answer every
question these projects ask, and a spreadsheet that gets emailed between volunteers then never
carries the name + address + date-of-birth triple. This is easy here only because ServWare
renders ages rather than birthdates on the page we parse.

For the members table the identity columns are not merely dropped after parsing — **they are
never read out of the DOM.** Only the four wanted columns are addressed by index, and
`tests/exports.rs` plants sentinel values in the SSN and licence cells and asserts they appear in
neither a parsed member nor a rendered CSV.

Files are written `0600` and land on the Desktop, discovered via `UserDirs` rather than
configured, so the install flow volunteers already have does not grow a third box.

## D22. Reads that fan out are approval-gated, even though they are reads

*September 2026*

`read_only_hint` exists so a client can auto-approve reads and the conversation stays smooth —
six dialogs before anything happens loses this audience (D1). `export_neighbors`,
`export_requests` and `read_export` carry it: each is a handful of JSON pages or a local file.

`export_household_members` deliberately does not. It opens one 200 KB page per household against
a live county system under a named account — 158 households in the verification window, taking
just under two minutes. The approval dialog is the honest encoding of "this makes 160 requests,
is that what you meant?", which is the same discipline the write tools follow. Alongside it:
fetches are sequential with a 150 ms gap and never concurrent; the default ceiling is 200
households with a hard maximum of 400; going over **refuses and names the count** rather than
truncating, because half a list is how a family gets left off a Christmas program; and every
export result states how many requests it made.

## D23. Validated against a volunteer's hand-built spreadsheet

*September 2026*

The Adopt-a-Family organiser's 2025 workbook (ServWare's Neighbor Assistance Summary Report for
07/01–09/30/2025, plus her hand-typed child ages) was used as a reference to check whether this
tooling captures a superset of what she had. It does, and the exercise turned up four things.

**Ages reconcile at 97%.** Of 182 child-age cells she typed by hand across 79 households that
overlap a current pull, 176 match an age this tool read, 153 of them at exactly +1 year — which
is what thirteen months of elapsed birthdays looks like. Head-of-household ages reconcile at
99% (142 of 151 at +1, 8 at +0). The extraction reproduces her manual work.

**Her method structurally truncates.** The spreadsheet has columns `C1`–`C4`, so a household is
capped at four children; the largest household in a current pull has eight. Nine of the
overlapping households have more under-18s than she had room to record.

**The report keys on `dateProvided`, not `dateRequested` — and this export now carries both.**
Reconciling "Total Assistance" against requests filtered by *request* date agreed for 88 of 153
households and overshot in 15. Filtering by *assistance* date agreed for 101 and overshot in
**zero**. "When a family asked" and "when help reached them" are different questions and a
request may carry items given on different days, so a fourth table was added — `assistance`, one
row per item, with `date_provided`, `assistance_type`, `monetary_value` and `pending`. Without
it no export could answer the question her report answers.

**The residual difference is staleness in her copy, not missing data here.** For the 52
households where her totals are lower, the shortfall is always $120, $130, $140, $150, $160 or
$170 — exactly `$70 food + one gift card from the $50–$100 ladder`, i.e. precisely one delivery
each. Her export was taken before those deliveries were entered. Nothing in her file exceeds what
this tool finds, on any column. This is an argument for pulling fresh at the moment of use rather
than working from a months-old export.

**Two corrections landed as a result:**

* **Reaching an old window used to fail.** `fetch_window` walks newest-first, so a window a year
  back sat behind ~1,200 newer requests and the page budget gave up having kept nothing — and
  "pull everything" would have failed too, against 4,907 total requests. `seek_window_start` now
  binary-searches the offset where the window begins using single-row probes (13 tiny requests
  instead of a dozen discarded hundred-record pages), and the budget is one shared constant sized
  past the full history.
* **The head of household had no age anywhere.** The members table excludes the neighbour (D19),
  so their age was the one thing her report had that this did not. `birth_date` is now read but
  kept private, and a derived `age` column is emitted — which is exactly the shape D21 asks for.

**Known gaps, deliberately left open.** Her report also carries `Ethnicity` and `Gender`. Both
are available in the roster response and neither is currently exported; adding them is a one-line
edit to `NEIGHBORS_HEADER` plus its pinned test, and it is a judgement call about what belongs in
a spreadsheet that gets emailed between volunteers rather than a technical limitation.

## D24. Exports carry recorded money only, and never inherit a human layout's limits

*September 2026*

Two corrections to the export design, both raised after the reference-file comparison (D23).

**No export may carry a policy-derived amount.** The gift-card ladder and the $70 food figure are
a *weekly delivery* policy. What a Christmas program spends per family is a separate decision
belonging to the volunteers running it, and one that changes year to year. Encoding a suggested
amount in raw CSV data would quietly present a delivery figure as a Christmas recommendation.

So the only money in an export is money ServWare says was **actually given** — `monetary_value`
per item and `assistance_total_dollars` per request, both historical fact. `domain::export` and
`domain::pull` do not import `domain::policy` at all, and a test asserts both that no export
column name looks policy-derived and that neither module's source mentions conference policy.
That source-level canary was verified to fail when a policy import is injected; the mistake it
guards against is a later change wiring the ladder into a project export.

**Extraction must not saturate where a human layout did.** The reference spreadsheet has columns
`C1`–`C4`, capping a household at four children, and larger families silently lost some. Real
data has households of ten, three with more than four children — one with eight.

Nothing in the parse or the CSV imposes a limit: one row per person, no cap. A fixture with a
ten-member household pins this. Real data gives independent evidence there is no cap upstream
either — `calculatedHouseholdCount` equalled the member row count plus one for **all 139**
households including the largest, and a truncating table would break that identity exactly at the
cap. The skill states the rule in words as well, because the model is the other place a family
could get trimmed.

## D25. The bundle carries its own correctness guidance

*September 2026*

The `.mcpb` ships tools and prompts; **skills are not part of it** and are installed separately.
So a volunteer who installs only the extension gets no skill, and every fact that decides whether
an answer is right was written down only in `skills/pulling-svdp-data/SKILL.md`. Without it a
model would compute household size off by one and silently drop the households ServWare holds
only a head count for — the exact failure the skill exists to prevent.

Those facts now live in three places that travel with the binary: the server's `instructions`
(sent on every initialize), the tool descriptions, and — for the two that matter most — the
**result text of `export_household_members` itself**, which is where the mistake would be made.
The skill keeps the conversational guidance; the bundle keeps the invariants.

**Related fix: `ServWareError::TooBroad`.** Asking for more households than the ceiling allows was
raised as `Malformed`, whose `user_message` is "ServWare sent back something this tool did not
understand. Nothing was written." A legitimate request with a clear next step was being reported
as a system fault, and the guidance was discarded. `TooBroad` passes its message through verbatim;
genuine faults still hide their detail. Caught by exercising the tools over stdio rather than
through the CLI, which is an argument for testing the surface volunteers actually use.

## D26. Onboarding lives in the bundle, because a bootstrap skill cannot exist

*September 2026*

The question was whether a single "bootstrap" skill could install the other skills and the
extension, or at least walk a volunteer through it. Checked against the MCPB manifest schema and
the Claude Desktop docs:

* **The manifest has no `skills` field.** Confirmed against the schema in
  `modelcontextprotocol/mcpb`: the top-level fields are `manifest_version`, `name`, `version`,
  `description`, `author`, `server` (required) plus `display_name`, `long_description`, `icon`,
  `icons`, `repository`, `homepage`, `documentation`, `support`, `screenshots`, `tools`,
  `tools_generated`, `prompts`, `prompts_generated`, `keywords`, `license`, `privacy_policies`,
  `compatibility`, `user_config`, `localization`, `_meta`. A `.mcpb` cannot carry a skill.
* **Skills install one at a time**, per user and per surface: Settings → Capabilities → Skills.
  Skills on claude.ai do not sync to Claude Desktop.
* **No skill can install anything.** Skills are instructions plus files; there is no installer
  primitive. Claude Desktop has no equivalent of Claude Code's `/plugin install org/repo`.
* **Whether Claude Desktop surfaces MCP prompts in its UI remains undocumented** (still true since
  D18).

So a bootstrap *skill* is not merely unsupported, it is the wrong shape: it has the same
installation problem it would exist to solve. A volunteer who can install it could have installed
the real skills.

**The dependency inverts instead.** The extension is the one artefact that must be installed, it
installs by dragging one file, and once it is in it can explain everything else. Onboarding
therefore lives in the bundle:

* **`getting_started`** — a read-only tool, and the only one that works with no credentials. It
  says what the extension does in two paragraphs of plain words and reports whether set-up is
  finished. A matching prompt exists for clients that surface prompts.
* **The server no longer exits when credentials are missing.** This was the worst failure in the
  product: with the two boxes empty the process exited at startup and its explanation went to
  stderr, i.e. a Claude Desktop log file no volunteer will ever open. The extension simply
  appeared dead. It now starts regardless, and `ServWareError::NotConfigured` — raised at
  `login()`, the single point every ServWare call funnels through — returns numbered instructions
  naming the actual menu items. Distinct from `LoginFailed`, where something *was* rejected.
* **Manifest metadata is now filled in**: `long_description` covering both jobs, plus `homepage`,
  `documentation` (pointing at `docs/for-volunteers.md`, written for volunteers rather than the
  developer README) and `support`. `license` was removed rather than claim one with no LICENSE
  file in the repo.
* **Three desktop shortcuts** instead of one — Start Here, Record Deliveries, Get a List of
  Families — since the deep link is the shortest path this audience has.

**The irreducible manual step is the first one:** somebody has to hand the volunteer the `.mcpb`
file. Nothing in any Claude surface can bootstrap that, so it stays a person-to-person handoff.
Everything after it is now self-explaining.

**The skills remain optional polish.** Correctness invariants live in the bundle (D25); the skills
add conversational guidance. A volunteer who installs only the extension gets right answers with a
less-guided conversation, which is the correct failure mode.

## D27. Skills are a build artifact, one zip per skill

*September 2026*

The first `svdp-skills.zip` was created by a `zip` command typed once by hand during a deploy. It
existed on one Desktop, in no script and no workflow, and was cited in setup instructions as
though it were a real artifact. It was also **wrong**: it held both skill folders, and Claude
Desktop's upload takes a single skill, so it would probably not have installed at all.

`scripts/build-skills.sh` now emits **one zip per skill** (`svdp-skill-<name>.zip`), each
containing that skill's directory and nothing else. It runs in the release workflow as its own
job — skills are plain markdown and platform-independent, so they build once rather than once per
target — and the zips are attached to the release alongside the three bundles. `deploy-mac.sh`
builds them too, so a local deploy and a release produce the same set.

The script **validates frontmatter before zipping**: keys restricted to the six claude.ai accepts
(`name`, `description`, `license`, `compatibility`, `metadata`, `allowed-tools`), `name` matching
the directory, and `description` under 200 characters. Verified to fail on an injected
`when_to_use` key. An upload dialog is a bad place to discover a frontmatter problem, and CI
additionally asserts each zip has exactly one top-level entry and a `SKILL.md`.

Note `recording-svdp-deliveries` sits at 199 characters, one under the limit — a future edit will
trip the check, which is the point.

The general lesson is the one worth keeping: **an artifact referenced in user-facing instructions
must come from a script.** Anything assembled by hand for a demo is not a deliverable, and citing
it as one is how instructions rot on contact.

### D27 amendment: one zip for all skills, not one per skill

*September 2026*

Per-skill zips were the wrong end of the trade. A volunteer would have to find and download N
files and keep track of which was which.

`build-skills.sh` now produces a single **`svdp-skills.zip`** containing one top-level
`svdp-skills/` directory, one folder per skill inside it, and a `HOW-TO-ADD-THESE.txt`. This works
because Claude Desktop's skill picker accepts a **folder**: the volunteer unzips once and adds
each folder in turn. The single top-level directory keeps an unzip from scattering folders across
Downloads.

The one-skill-per-upload constraint has not gone away — it is now handled by unzipping first
rather than by shipping separate archives. The instructions travel inside the zip, since this
audience does not read repositories (D26).

CI asserts the zip has exactly one top-level entry named `svdp-skills`, that the instruction file
is present, and that **every** skill directory in the repo appears with its `SKILL.md` — so adding
a skill and forgetting to ship it fails the release. That check was run locally against a
deliberately incomplete zip to confirm it fails rather than merely existing.

## D28. One archive per platform is what a volunteer receives

*September 2026*

Supersedes the D27 amendment, which was built on a wrong assumption of mine: I believed Claude
Desktop's skill picker accepted a *folder*, and shipped a zip of unzipped skill folders on that
basis. **Claude Desktop accepts a zip for a skill.** Corrected by Cory, whose actual concern was
never the number of downloads but that volunteers would not know how to unzip anything.

The shape is now one file per platform:

```
svdp-servware<suffix>.zip
└── svdp-servware/
    ├── START-HERE.txt
    ├── svdp-servware.mcpb
    └── skills/
        ├── pulling-svdp-data.zip
        └── recording-svdp-deliveries.zip
```

**One unzip, then two kinds of install:** drag the `.mcpb` into Settings → Extensions, then add
each skill zip under Settings → Capabilities → Skills, left zipped. The single top-level directory
means unzipping produces one folder rather than scattering files.

`scripts/build-release.sh` assembles it, calling `build-mcpb.sh` and `build-skills.sh`. Inside the
archive the bundle is named without a platform suffix, so `START-HERE.txt` reads identically on
every machine.

**Consequence: the archive is per-platform, so skills no longer build as their own CI job.** The
`.mcpb` carries a platform-specific binary, so each matrix target assembles its own archive;
skills are markdown and cost nothing to rebuild per runner. The release publishes three zips and
no bare `.mcpb` — a volunteer choosing between a `.zip` and a `.mcpb` is a choice they should not
have to make.

CI verifies, per platform: exactly one top-level entry named `svdp-servware`; `START-HERE.txt` and
`svdp-servware.mcpb` present; a zip present for **every** skill directory in the repo; the inner
`.mcpb` containing `bin/svdp-mcp` with a manifest whose platform matches the matrix; and each
skill zip being a readable archive rooted at its own skill directory. Every one of those checks
was executed locally against deliberately broken archives, including one with the bundle deleted
and one with a skill withheld, to confirm they fail rather than merely run.

**Instructions ride inside the archive.** `START-HERE.txt` is the primary setup document, not
`docs/for-volunteers.md` and not the release notes: this audience is assumed to read nothing
outside what they were handed.

## D29. The monthly interval is 28 days, and both lists are always shown

*September 2026*

Volunteer feedback: deliveries go out once a month per family, so the working list should not
offer a household served three weeks ago.

**The threshold is 28 days, not 30, and the difference is not cosmetic.** Measured against a year
of real assistance items — 1,114 repeat deliveries across 177 households — the gap between
consecutive deliveries clusters hard at exactly four weeks, because deliveries run on fixed
weekdays:

| gap | count |
|---|---|
| 1–27 days | 55 |
| **28–31 days** | **388** |
| 32–45 days | 462 |
| 46+ days | 209 |

A 28-day threshold would have held back 55 of those 1,114 (5%). A 30-day threshold would have
held back **280 (25%)** — a quarter of legitimate monthly deliveries suppressed. The cliff sits
precisely between 28 and 29. Live output confirms it: five families currently sit at exactly 30
days since their last delivery and are correctly shown as due. Configurable as
`delivery_interval_days`.

**Recency reads `date_provided`, not `date_requested`** (D23). `client.lastRequestDate` is
useless here: a household with an open request last *asked* today by definition.

**The history lookback is 120 days.** The list endpoint can only filter on `date_requested`, but
recency is about when help arrived, and the lag between the two is median 4 days, p99.9 39 days,
observed maximum 134. A 28-day window would miss deliveries against older requests entirely.
Costs about five extra JSON pages, paced.

**A partly recorded delivery must not hide its own request.** An interrupted recording leaves
items on a still-open request; if those counted toward recency, the request would disappear from
the list of work left to do. `last_delivery` therefore excludes items belonging to the request
being judged.

**Both lists are always shown** — every open request first, then the shorter due list, with a
"Due now" column in the full table making the filter auditable. Cory's reasoning: if the
filtering does something strange, the volunteer can still see everything. This replaced an
earlier design where the tool returned only the filtered list plus an `include_recent` override;
showing both makes that parameter redundant, and one way to do a thing beats two.

**The filter is advice, not enforcement.** If a volunteer says they delivered to a household that
was not due, that is recorded. They were there. The skill states this explicitly, because the
failure mode to avoid is a tool arguing with the person holding the clipboard.

**Placement.** The interval lives in `conference.toml` and `ConferenceConfig::served_recently`
(conference policy, like the gift-card ladder); `domain::recency` turns request history into
last-delivery dates; the MCP tool applies it and reports; the skill owns the wording. The ServWare
protocol layer is untouched.

## D30. One universal Mac build, and no Intel runner

*September 2026*

An Intel Mac release job sat queued indefinitely. The cause: **`macos-13` was retired in December
2025**, so a job requesting that label never gets a runner. It had been pinned since the release
workflow was first written and nothing had exercised it.

The obvious repair was to swap in a live Intel label — `macos-26-intel` (a standard x64 runner,
announced with macOS 26's general availability in February 2026), `macos-15-intel` (the last
x86_64 image, available until August 2027), or the billed `macos-14-large`. All three were
rejected, because **GitHub drops x86_64 macOS entirely after August 2027** and any of them just
schedules this same conversation for then.

Instead there is now **one Mac job producing a universal binary**: both `aarch64-apple-darwin` and
`x86_64-apple-darwin` are built on an arm64 runner (`macos-26`, pinned rather than
`macos-latest`) and joined with `lipo`. This outlives the x86_64 runner deadline, since
cross-compiling to Intel does not require an Intel machine.

**The volunteer-facing win is the actual point.** The release went from three downloads to two, and
the setup instructions no longer ask an elderly volunteer to open the Apple menu, read "About This
Mac", and decide whether their processor says M1 or Intel. There is one Mac file and it works on
every Mac. For this audience, removing a decision is worth more than saving a build.

`SVDP_UNIVERSAL=1` selects the fat build in `build-mcpb.sh`, which then **asserts both slices are
present** via `lipo -archs` — a bundle silently carrying one architecture would install cleanly and
then fail for half the conference. CI re-checks the same property on the bundle inside the shipped
archive. `deploy-mac.sh` also builds universal, so the artifact tested on the maintainer's Mac is
the one volunteers receive.

**Unverified:** the x86_64 slice has never been compiled. The Mac on the LAN was asleep, and this
machine is Linux, so nothing here could cross-compile a Darwin target. The risk sits with
`aws-lc-rs` (pulled in by `reqwest`'s rustls stack), which builds C and assembly through cmake;
macOS x86_64 is a first-class target for it and GitHub's runners carry cmake, so this is expected
to work rather than known to. `workflow_dispatch` is enabled on the release workflow, so it can be
proven without cutting a tag.

Also worth recording, since it came up: **there is no macOS 16.** Apple renumbered from 15 to 26.

## D31. Documentation moved out of the README, split by audience

*September 2026*

The README had accumulated four audiences in one file: volunteer setup, volunteer usage,
maintainer build-and-release notes, and project background. Cory asked for it to become a stub so
he could write a note to the next human maintainer there.

Documentation now lives in `docs/`, split by who reads it:

| Document | Audience |
|---|---|
| `docs/for-volunteers.md` | volunteers — plain words, no repository paths |
| `docs/maintaining.md` | whoever looks after the code |
| `docs/README.md` | an index, so `docs/` is navigable when the root README is a stub |
| `DECISIONS.md`, `api.md`, `CLAUDE.md` | unchanged, and left at the root where everything links to them |

`docs/for-volunteers.md` kept its path deliberately: the manifest's `documentation` field points
at it, and moving it would break the link shown in Claude Desktop's extension settings.

**The primary setup document is still `START-HERE.txt` inside the release archive**, not anything
in `docs/`. A volunteer reads what they were handed; `docs/for-volunteers.md` is the longer version
for anyone who goes looking. `CLAUDE.md` now says so, along with a note not to move documentation
back into the README.

`docs/maintaining.md` carries a **Known gaps** section — what has never been run, and what is
expected to work rather than known to. That belongs in the maintainer's guide rather than only in
a conversation, because it is the first thing a new maintainer needs and the last thing anyone
thinks to write down.

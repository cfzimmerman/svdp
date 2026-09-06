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

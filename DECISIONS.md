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

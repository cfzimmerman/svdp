# CLAUDE.md

Guidance for Claude Code working in this repository.

**Keep this file updated** with project purpose, decisions, and user feedback so context
carries across sessions.

## Log decisions and discoveries

`DECISIONS.md` is the trail of intent: why the project is shaped as it is, including the
**rejected alternatives** and the reason they lost. Read it before proposing a change to the
architecture — most of the obvious-looking alternatives have already been considered and ruled
out for reasons that are not obvious.

Append to it as part of the work, not as a cleanup pass afterward:

- A **decision** gets an entry with its rationale and what was rejected. A decision without its
  rationale gets re-litigated.
- A **discovery** — something established by running it and observing the result, especially
  where it contradicts documentation or intuition — gets an entry with how it was verified, so a
  later session can re-verify rather than trust blindly.
- When a finding **overturns an earlier claim, including one of your own**, say so explicitly.
  The correction is the valuable part.

## Git

**Do not make git writes.** No `commit`, `push`, `branch`, or `reset`, and do not offer to —
Cory commits his own work. Read-only git (`status`, `log`, `diff`, `show`) is fine and useful.
At a natural checkpoint, just say what changed.

## Project

**Repository**: svdp (github.com/cfzimmerman/svdp) — **public**.

SVdP (St. Vincent de Paul) at Nativity Catholic Church in Menlo Park delivers food and
grocery gift cards to families in need. The county records requests in **ServWare**
(servware.org), a Spring MVC + jQuery/Bootstrap app with no public API.

Twice a week volunteers deliver in one to three groups, then must mark requests Complete
and log dollar values for food and gift cards. This tool automates that last step.

### Goal

Make the workflow usable by **elderly, non-technical volunteers** — no terminal, no
spreadsheets — via an MCP server installed into the Claude Desktop **Chat tab**.

## PII: the hard rule

Neighbour data is sensitive information about vulnerable families, and this repo is public.

- **Never commit CSVs, HAR captures, or anything derived from a real ServWare response.**
  `.gitignore` matches by glob (`*.csv`, `*.har`), not by filename — the old exact-name list
  missed the `group1.csv` files the documented workflow told users to create.
- **Fixtures are synthetic by construction** (`scripts/gen-fixtures.py`), not scrubbed from
  real data. Field *names* are protocol facts and safe; every *value* is invented.
- Real captures are validated against by an **opt-in local test** that reads a path from
  `SVDP_LOCAL_DETAIL_HTML` and prints structure only, never values.
- Do not print neighbour or volunteer PII into transcripts, terminal output, or logs. The
  request detail page carries names, addresses, phones, **SSN last-4**, driver's licence, and
  the family's full request history.

## Architecture

- **Language**: Rust (edition 2024). Chosen over a TypeScript port for dependency stability
  and because typed errors make "did this write land?" checkable.
- **Two UIs over one logic layer**: `src/bin/mcp.rs` (volunteers, via Claude Desktop) and the
  CLI (maintainer). Protocol and domain logic stay independent of both.
- **Errors**: `thiserror` types at the library boundary — the MCP layer must *match* on error
  kind to choose retry / skip / escalate. `anyhow` in binaries only. (This reverses an earlier
  rule in this file that said "no custom error types"; that was right for a pure CLI.)
- **HTTP**: `reqwest` with a cookie jar. Base URL is **injected**, not a const, so tests point
  at a local server and exercise real cookies, redirects, and encoding.

### Form handling — the load-bearing piece

`src/servware/form.rs` extracts *every* named control from a form and applies a checked
overlay. Never enumerate fields by hand.

Why: the live request edit form renders **50 named controls**; the old hand-written builder
sent 39 and silently cleared the other 11 (`otherVisitCnt`, `eldercareVisitCnt`,
`hospitalVisitCnt`, `prisonVisitCnt`, `phoneVisitCnt`, `churchPantryVisitCnt`,
`referralOrganizationId{,2,3,4}`, `referralConference`) on every mark-complete.

Rules encoded there, verified against a real capture:
- Extraction is **form-scoped** — the page has other forms (a send-email modal) whose controls
  must not leak in.
- `id` and `name` diverge (`id="homeVisitAssignedFirst"` is `name="visitAssignedToMemberId"`).
  **Always key on `name`.**
- Unchecked checkboxes are **rendered but not submitted**. The full control inventory is kept
  so an overlay can tell "ServWare removed this field" (an error) from "this box is off"
  (normal, and what marking a visit complete flips).
- Spring MVC checkbox convention: `name=value` only when checked, `_name=on` always.
- Overlaying a control the form does not render is a **hard error** — the canary for ServWare
  renaming something.

## Verified facts (spike, Sep 2026)

- **`rmcp` 3.2.0** works: stdio server, tool schemas derived from serde structs via `schemars`,
  `annotations(read_only_hint = true)` for client auto-approval. Requires Rust ≥ 1.88.
- **`.mcpb` packaging works with a Rust binary.** `server.type = "binary"`,
  `command: "${__dirname}/bin/svdp-mcp"`. Build with `scripts/build-mcpb.sh`; it validates the
  manifest before zipping. `manifest_version` must be `0.3`/`0.4`, and **every `user_config`
  entry requires `description`** (omitting it fails install with "Required, Required").
- **Gatekeeper is a non-issue.** Claude Desktop does not propagate the quarantine attribute
  when unpacking a bundle; a downloaded `.mcpb` installs and runs. No Developer ID or
  notarization needed.
- **`sensitive: true` values are encrypted at rest** in
  `~/Library/Application Support/Claude/Claude Extensions Settings/<ext>.json`, keyed by the
  `Claude Safe Storage` keychain item, and injected as env vars at server spawn. The model
  never sees the value.
- An installed extension starts **disabled**; it must be toggled on before tools appear.
- No CSRF token in the request form — all 14 hidden fields are accounted for.
- ServWare sessions expire after 3600s; long delivery-night chats will cross that, so
  transparent re-auth is a functional requirement.

## Known defects in the legacy `src/api` + `src/nativity.rs` path

Being replaced; do not extend. Writes accept any 2xx/3xx as success while Spring re-renders a
rejected form as 200 (so failures report as successes); blanket `#[serde(default)]` turns a
renamed `calculatedHouseholdCount` into `0`, which the gift-card ladder maps to $50 for every
family; `add_assistance` is not idempotent and double-logs money on re-run; `?` inside write
loops strands batches with no recovery path; the list call caps at 100 with no pagination.

## Build and run

```bash
cargo test                      # must pass with no network
cargo build --bin mcp           # the MCP server
./scripts/build-mcpb.sh         # -> dist/svdp-servware.mcpb (run on the target platform)
python3 scripts/gen-fixtures.py # regenerate synthetic fixtures

# Validate against a real local capture (never committed):
SVDP_LOCAL_DETAIL_HTML=/path/detail.html cargo test --test local_capture -- --ignored --nocapture
```

Credentials come from `SERVWARE_USER` / `SERVWARE_PASS` (env, or `.env`). The `.mcpb` supplies
them from its `user_config`.

## Reference

`api.md` is the reverse-engineered ServWare API reference. It is **partly abridged** — its
assistance-item section lists 13 form fields where the real browser POST sends 30. Where
`api.md` and a capture disagree, the capture wins, and `api.md` should be corrected.

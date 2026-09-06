# ServWare tools for St. Vincent de Paul

Records SVdP food and gift card deliveries in [ServWare](https://www.servware.org), so
volunteers do not have to type them in one at a time.

Built for the SVdP conference at Nativity Catholic Church in Menlo Park, CA. Another conference
could use it, but the assistance type IDs are conference-specific — see
[For another conference](#for-another-conference).

## For volunteers

You need a ServWare username and password from your conference. Nothing else — no terminal, no
spreadsheets.

### 1. Install Claude Desktop

Download it from [claude.ai/download](https://claude.ai/download) and sign in.

### 2. Add the SVdP extension

Download the file for your computer from the
[latest release](https://github.com/cfzimmerman/svdp/releases/latest):

| Your computer | File |
|---|---|
| Mac, Apple silicon (M1 or later) | `svdp-servware-macos-arm64.mcpb` |
| Mac, Intel | `svdp-servware-macos-x86_64.mcpb` |
| Linux | `svdp-servware-linux-x86_64.mcpb` |

Not sure which Mac you have? Apple menu → About This Mac. If it says "Apple M1", "M2", "M3" or
similar, use the Apple silicon one; if it says "Intel", use the Intel one.

Then in Claude Desktop: **Settings → Extensions**, and drag the downloaded file in.

### 3. Enter your ServWare details and switch it on

Open the extension's settings and type your ServWare username and password into the two boxes.
They are stored securely on your own computer and are never part of the conversation.

**A newly installed extension starts switched off.** Make sure the toggle is on, or nothing will
happen.

### 4. Use it

In the Chat tab, say what you did:

> I did deliveries today and need to record them.

Claude will show the families waiting, ask which ones you delivered to, check the gift card
amounts with you, ask who drove, show you everything before saving, and then record it.

It always shows you the full list and asks before saving anything.

**If something looks wrong, say so.** Nothing is written to ServWare until you say yes.

## Notes for whoever looks after this

### Building

```bash
cargo test                 # whole suite, no network needed
./scripts/build-mcpb.sh    # -> dist/svdp-servware.mcpb, for this machine
```

Releases are built by GitHub Actions for all three targets — push a `v*` tag. A `.mcpb` carries
a compiled binary, so each platform needs its own bundle. Don't hand-build for volunteers; let
CI do it, so what they install is reproducible.

### The command-line tool

Same logic layer as the extension, for debugging and as a fallback:

```bash
svdp health                     # sign-in, request list, and form check
svdp requests                   # open requests with suggested gift card amounts
svdp volunteers                 # volunteer names and ids
svdp request 4311091            # one request and what is logged against it
svdp form 4311091               # the edit form, to see what a write would send
svdp snapshot                   # save one page locally for offline work
```

Writes are dry-run unless you pass `--yes`:

```bash
svdp add-item --request 4311091 --slot food --dollars 70 --session TEST
svdp complete --request 4311091 --volunteer 37629
```

Credentials come from `SERVWARE_USER` and `SERVWARE_PASS`, in the environment or a `.env` file.

**ServWare is production and requests run as you.** Reads are cheap but not free; writes are
permanent — ServWare has no way to delete an assistance item once added.

### For another conference

Assistance type IDs differ per conference. `conference.toml` is compiled into the binary as the
default; override it with an external `conference.toml` or `SVDP_CONFERENCE_CONFIG=/path`.

### Working on it

- `CLAUDE.md` — how the code is organised, and the rules that matter.
- `DECISIONS.md` — why it is built this way, including alternatives that were rejected.
- `api.md` — the reverse-engineered ServWare API.

**Never commit CSVs, HAR captures, or anything derived from a real ServWare response.** This
repository is public and the data concerns families receiving assistance. Test fixtures are
synthetic by construction; CI fails if a real-data file is tracked.

## Background

Community members submit requests for food or other assistance to a county office, which records
them in ServWare. Twice a week, volunteers call the people who have been waiting longest, pack
food, allocate grocery gift cards by household size, drive the deliveries, and then record what
was given.

That last step is what this automates. It used to mean editing spreadsheets and running terminal
commands; now it is a conversation.

### ServWare

There is no official ServWare API — this was reverse-engineered from browser traffic. ServWare is
a good tool and its maintainers deserve the credit for it.

If you work on ServWare and want to talk, please reach out. There is no commercial interest here.
If you are at another SVdP conference with similar needs, likewise — it would be good if this
were useful beyond one parish.

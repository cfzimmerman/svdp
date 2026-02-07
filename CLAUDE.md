# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Every Claude session must keep CLAUDE.md updated** with project purpose, user feedback, and decisions so context carries across sessions.

## Project

**Repository**: svdp (github.com/cfzimmerman/svdp)

SVDP (St. Vincent de Paul) at Nativity Catholic Church in Menlo Park delivers food to community members in need. The county takes requests via a web app called **ServWare** (servware.org), a jQuery/Bootstrap server-rendered app built for SVDP chapters. There are no public API docs.

### Goals

1. Query open requests from ServWare and export to CSV
2. Read/sort CSV to find oldest open requests
3. Apply updates back to ServWare

### Current Phase

**Phase 1**: HTTP client foundation — authenticate with ServWare and make requests. This requires reverse-engineering the API with the user's help via browser dev tools network captures.

### What we know about ServWare

Full API reference: **`servware/api.md`** (reverse-engineered from HAR captures)

Key endpoints:
- `POST /security/login` — form fields: `username`, `password` → 302 to `/app/home?continue`
- `GET /app/assistancerequests/list` — DataTables SSP JSON API, returns `{sEcho, iTotalRecords, iTotalDisplayRecords, aaData}`
- `POST /app/assistancerequests/{id}` — update request (mark complete), 30 form fields, Spring MVC checkbox convention
- `POST /app/assistancerequests/{id}/assistanceitems/new` — add assistance item

Session: HttpOnly cookies (not visible in HAR exports), 1-hour timeout, extendable via `GET /security/extendSession`

## Architecture

- **Language**: Rust (edition 2024)
- **Error handling**: `anyhow::Result` everywhere, `.context()` for meaning, `bail!`/`ensure!` for conditions. No custom error types.
- **HTTP client**: `reqwest` with `cookie_store(true)` for automatic session management
- **Credentials**: env vars (`SVDP_USERNAME`/`SVDP_PASSWORD`) for scripting, `rpassword` interactive prompt for manual use. No config file.
- **Password safety**: `secrecy::SecretString` wraps password to prevent accidental logging
- **CLI**: `clap` derive-based subcommands

## Build and Run

```bash
cargo build                                    # Build
cargo run -- login                             # Test login
cargo run -- ping                              # Test session keepalive
RUST_LOG=svdp=debug cargo run -- login         # Debug logging
```

## Dev Environment

Docker-based dev environment. Use `./run-dev.sh` to start.

# devmail — Claude Context

## What this project is

`devmail` is a Rust CLI tool that runs two servers simultaneously:
- **SMTP server** on `127.0.0.1:1025` — accepts all incoming email, stores it
- **HTTP server** on `127.0.0.1:8085` — serves a webmail UI and JSON API

It's a development tool for testing apps that send email. Nothing is ever delivered.

---

## Project structure

```
src/
  main.rs          — entry point: parses CLI args, creates shared store, spawns both servers
  config.rs        — clap CLI struct (Config) — --store, --path, --smtp-addr, --http-addr, --pass
  model.rs         — Email, Attachment, EmailSummary (incl. to/cc), EmailDetail types
  store.rs         — EmailStore (Arc<RwLock<>>) with in-memory + optional mbox disk write/reload
  mime.rs          — parses raw SMTP DATA bytes into typed Email via mail-parser
  smtp/
    mod.rs         — re-exports run()
    server.rs      — TCP listener, spawns one SmtpSession per connection
    session.rs     — SMTP state machine (Connected→Greeted→InTransaction→Data)
    parser.rs      — parses SMTP command lines into SmtpCommand enum
  http/
    mod.rs         — re-exports run()
    server.rs      — creates axum router, binds listener
    api.rs         — AppState, auth middleware, all HTTP handlers, login/logout, serves index.html
assets/
  index.html       — entire webmail UI (vanilla HTML/CSS/JS, no build step)
                     embedded at compile time via include_str!()
screenshot.png     — UI screenshot used in README.md
```

---

## How to build and run

**Windows (native, in VSCode terminal):**
```
cargo build
cargo run
cargo run -- --store                      # enable mbox disk storage
cargo run -- --store --path C:\tmp\mail
cargo run -- --pass mysecret              # password-protect the webmail UI
DEVMAIL_PASS=mysecret cargo run           # same, via env var
```

**Linux binary (Docker Desktop required):**
```
./build.sh build-linux         # produces dist/devmail (Linux x86_64)
```

**Test in Docker container:**
```
./build.sh test-container      # builds and runs in Ubuntu 24.04
```

---

## Key design decisions

- **SharedStore** = `Arc<RwLock<EmailStore>>` — cloned into both server tasks
- **No async trait** — `EmailStore` is a concrete struct, not a trait
- **IndexMap** preserves insertion order for email list (newest-first via `.rev()`)
- **assets/index.html** embedded via `include_str!("../../assets/index.html")` in `src/http/api.rs`
- **mbox format** — hand-written append; `X-DevMail-ID` header stores the UUID for stable reload
- **mbox rewrite on delete** — deleting any email atomically rewrites `devmail.mbox` via tmp+rename
- **devmail_state.json** sidecar tracks read UUIDs so read status survives restart
- **mail-parser 0.9** (Apache 2.0) — handles full MIME including multipart, base64/QP decoding
- **Polling** — webmail JS polls `/api/emails` every 3 seconds (no WebSocket needed)
- **iframe sandbox** — HTML emails rendered in `<iframe sandbox="allow-same-origin" srcdoc="...">` to prevent script execution
- **Auth** — `--pass` / `DEVMAIL_PASS` sets a password; a random session token is generated on startup; stored in an `HttpOnly; SameSite=Strict` cookie; login/logout via `/login` and `/logout`; all routes protected by axum `route_layer` middleware except `/login` and `/logout`
- **Search** — client-side, filters `EmailSummary` list by all words matching subject/from/to/cc; `EmailSummary` includes `to` and `cc` for this purpose; matched tokens highlighted with `<mark class="hl">` in yellow
- **`__AUTH_ENABLED__`** placeholder in `index.html` — replaced at serve time by `serve_index` handler to inject `const DEVMAIL_AUTH = true/false` for the Sign out button
- **Received header** — prepended to the raw message in `SmtpSession` at DATA acceptance time (RFC 5321 §4.4); format: `from <ehlo-id> ([<peer-ip>])\r\n\tby devmail with ESMTP; <date>`

---

## All dependencies are MIT or Apache 2.0 (no copyleft)

Do not add any GPL, LGPL, or AGPL dependencies.

---

## HTTP API routes

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Serve embedded index.html (auth-gated if --pass set) |
| GET | `/login` | Serve login page |
| POST | `/login` | Validate password, set session cookie, redirect to `/` |
| POST | `/logout` | Clear session cookie, redirect to `/` |
| GET | `/api/emails` | List emails (Vec<EmailSummary> with to/cc, newest first) |
| GET | `/api/emails/:id` | Get EmailDetail |
| POST | `/api/emails/:id/read` | Mark as read (204) |
| DELETE | `/api/emails/:id` | Delete email (204); rewrites mbox if disk enabled |
| GET | `/api/emails/:id/raw` | Raw RFC 5322 message text |
| GET | `/api/emails/:id/attachments/:filename` | Download attachment |

---

## SMTP protocol flow

```
S: 220 devmail ESMTP ready
C: EHLO client.name
S: 250-devmail
S: 250 8BITMIME
C: MAIL FROM:<sender@example.com>
S: 250 OK
C: RCPT TO:<recipient@example.com>
S: 250 OK
C: DATA
S: 354 End data with <CR><LF>.<CR><LF>
C: <email content>
C: .
S: 250 OK: message accepted
C: QUIT
S: 221 Bye
```

---

## Disk storage details

- **`devmail.mbox`** — standard mbox format; each entry has an `X-DevMail-ID: <uuid>` line between the `From ` separator and the RFC 5322 headers, enabling stable UUID-based identity across restarts
- **`devmail_state.json`** — `{ "read_ids": [...] }`; written on every `mark_read` and `delete`; loaded on startup to restore read status
- On delete: the mbox is rewritten (not just filtered via a sidecar) using a tmp-file + rename for atomicity; `devmail_state.json` is also updated

---

## Release packaging

- Windows: `dist/devmail-v<version>-windows-x86_64.zip` → subfolder `devmail-v<version>-windows-x86_64/` containing `devmail.exe`, `LICENSE.md`, `README.md`
- Linux: `dist/devmail-v<version>-linux-x86_64.tar.gz` → subfolder `devmail-v<version>-linux-x86_64/` containing `devmail`, `LICENSE.md`, `README.md`
- The `dist/` folder is gitignored

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A server that replaces the original Dragon City (Flash) back end, plus a web client that runs the unmodified SWF in the browser with Ruffle. Rust (axum + tokio) + PostgreSQL (accounts/sessions/friendships) + MongoDB (player saves), packaged with Docker Compose. Docs for humans are in `README.md` and `docs/` (deployment, configuration, architecture, API, game protocol, development, SECURITY.md); don't duplicate them here.

The server was rewritten from Python (Flask) to Rust; the behaviour, the wire formats, the database schemas and the environment variables are meant to be unchanged. The Python tree is in git history (`git log -- src/app.py`) if you need to compare.

## Commands

```bash
cargo build                                    # debug build
cargo test                                     # whole suite, needs no services
cargo test --test game a_goal_pays_out_once    # one integration test
cargo run -- <serve|migrate|create-admin|seed|healthcheck>   # CLI (serve listens on BIND_ADDR, :8080 by default)
cargo run --release --example bench -- 127.0.0.1:8080        # load generator (use a throw-away server)

cd frontend && npm install
npm run dev        # Vite on :5173, proxies /api /static /flash /assets /deploy /stream to :8080
npm run build      # tsc -b && vite build -> frontend/dist (served by the Rust server)
npm run lint       # oxlint

docker compose up -d --build                   # needs .env (copy .env.example)
```

If rust-analyzer is running in the editor, terminal builds can fail with "file in use": use a separate `CARGO_TARGET_DIR`.

Tests use the in-memory stores (`src/db/memory.rs`) through `tests/common/mod.rs`: `TestApp::player()`, `client.packet(&[("cmd", json!([args]))])`, `app.edit_save()`/`app.give()` to set up a scenario. The web client has no tests. The real PostgreSQL/MongoDB code (`src/db/postgres.rs`, `src/db/mongo.rs`) is not covered by the suite: after touching it, run a short session against two throw-away containers.

Migrations: add `migrations/NNNN_name.sql` (plain SQL, idempotent with `IF NOT EXISTS`; databases created by the old Alembic setup must be adopted untouched). `serve` and `migrate` apply them.

## Architecture (the parts that span files)

- **One origin serves everything**: the built SPA, `/api/*` (JSON API for the web client/admin) and the original Flash endpoints (`/static/dragoncity/srv/*`, `/flash`, `/assets`, `/stream`). The login cookie (`dc_session`) is what authenticates the Flash client's requests, so the SPA catch-all stays the router's `fallback` (`app.rs`), and Vite's `assetsDir` is `_app` so it doesn't collide with the game's `/assets`.
- **Identity comes only from the session** (`auth/mod.rs`, `request_context`: origin/CSRF check, then user load; the `Login`/`Admin` extractors enforce access). Ids the Flash client sends (`USERID`, `id`) are ignored on purpose. Player id == `users.id`.
- **`packet.php` flow**: `routes/packet.rs` → takes `Storage::lock(player_id)` → `commands::dispatch_commands` runs each command against a snapshot of the save, restoring it on `CommandError::Rejected`/`Failed` → save written back → SSE `JUEGO_GUARDADO` sent. Unknown/rejected commands still answer `"ok"` so the client keeps running.
- **Commands**: handlers are `async fn(&mut Ctx) -> CmdResult<()>` grouped by theme in `src/commands/` and dispatched by the `match` in `commands::run` (wire names live in the `commands!` list in `enums.rs`; adding a variant without a handler does not compile). The save (`ctx.data`) is a `serde_json::Value` that keeps the Flash client's JSON shape; handlers are near-ports of the original logic.
- **Storage**: `storage.rs` is the only module that talks to the `PlayerStore` trait (MongoDB `players` collection) and `gamedata.rs` holds the read-only game config (`web/srv/get_game_config.json`, may start with `<signature>;`), indexed by item id, with its signed form cached. Per-player black-market offers are merged into the config when served. A neighbour's assist is queued in `pending_assists` and merged on the target's next load instead of rewriting their save. Postgres/Mongo/in-memory implementations sit behind the `AccountStore`/`PlayerStore` traits in `db/`.
- **Neighbours = friendships** (`friends.rs`, PostgreSQL `friendships`): `get_player_info` adds a computed `neighbors` list (never stored in the save), `/api/game/bootstrap` builds `fb_sig_friends` from it, and visiting (`user=<id>`) and `assist_neighbor` are friends-only. The client's own `neighbors` request field is ignored. Protocol details are in `docs/game-protocol.md#neighbours`; requests (`api/friends.rs`) become `accepted` friendships, and `friends::award_both` pays the "add a friend, get 5 gems" reward (first 5 friends, once per friend) straight into the Mongo save.
- **Deus** (`system_accounts.rs`): the client's built-in "Deus" card visits the fixed id `DEUS_PLAYER_ID` (100000030). A disabled system account + save is created at start-up and is the only non-friend that can be visited; without it the client hangs on a progress bar.
- **Single process on purpose**: per-player locks, SSE channels, the rate limiter and the login cache (`state.rs`) are in memory. Don't add replicas without moving those to a shared store. Inside the process the tokio runtime uses every core.
- **Signed responses** (`security.rs`, `json.rs`): the HMAC covers the exact text sent, which is serialized with `\uXXXX` escapes for non-ASCII like Python's `json.dumps`, so the bytes match the original server's.
- **Ruffle integration** (`frontend/src/components/game/ruffle.ts`): the SWF has the original hosts hardcoded, so Ruffle gets `urlRewriteRules` built from `legacyHosts` (`api/game.rs`) that send them to this server; flash variables come from `GET /api/game/bootstrap`.
- **Game version** (`src/game_versions.rs`): the Flash clients live in `flash/versions/` (the loader, `flash/DCLoader.swf`, is not one of them). `GET/PUT /api/admin/game-version` lets an admin pick which one `bootstrap` sends the client; the choice is stored in PostgreSQL (`app_settings`) so it survives restarts, falling back to `GAME_SWF` if the stored file is gone. A newer client version is not guaranteed to work: it may expect commands or a `get_game_config.json` shape this server doesn't have yet (see `docs/commands-roadmap.md`).

## Rules that must hold when touching commands

The client is untrusted (see `docs/SECURITY.md`). In a command: read arguments only through `ctx.int_arg/uid_arg/iso_arg/flag_arg/text_arg/name_arg` (not `ctx.args[i]`), validate before mutating, charge with `ctx.pay*()` (never by clamping at zero), take rewards from config/server state rather than client values, use `ctx.new_uid` for client-chosen ids, and `reject("...")` on anything wrong. Handlers must never panic (a panic would take the request down instead of rolling the command back): don't index `Value` with `v["key"] = ...` on anything that is not guaranteed to be an object, use `set_field`/`ensure_object`/`ensure_array`. Don't hold a `rand::rng()` across an `.await`. Real client behaviour matters: the tutorial replays many commands in a single packet, so server-time checks between commands of one packet are unreliable (see `commands/buildings.rs`'s one-time tutorial harvest exception).

Before adding a command that isn't in `Command` (`enums.rs`) yet, check `docs/commands-roadmap.md`: it tracks which candidate commands (mined from the client SWFs' strings) already have a confirmed cost/reward/state in the game config or the save template, versus which are still guesses. Only implement one once its mechanic is grounded there — a guessed price or reward is a duplication bug waiting to happen. The reliable way to confirm a command is real and see its actual args is `docker compose logs -f app | grep "is not implemented"` while playing; the `jpexs` MCP server (`.mcp.json`, wraps JPEXS FFDec, needs Java on `PATH`) can decompile a client SWF's ActionScript when string-mining alone isn't enough.

## Conventions specific to this repo

- Logs use `tracing` in English, never `println!`. API error messages are English too.
- The frontend follows the project's shadcn rules: add components with `npx shadcn@latest add <name>` (don't hand-write primitives), forms use `Controller` + `Field` + a zod schema (types named `{Name}FormValues`), text goes through `Typography`, body links through `@/components/ui/link`, components are `const X: FC<Props>`, and types are imported with `import type`. `cn` is implemented with `clsx` + `tailwind-merge` in `src/lib/utils.ts`.
- Game files (`flash/`, `assets/`, `domains/`) are not in the image and not under the MIT license; they are mounted read-only by `compose.yaml`. `.env` is git-ignored.
- Commit messages end with the `Co-Authored-By: Claude ...` trailer requested for this repo's commits.

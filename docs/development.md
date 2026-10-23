# Development

## Setup

Requirements: a recent stable [Rust](https://rustup.rs/) toolchain (1.80+), Node 22+ for the web
client, and Docker if you want real PostgreSQL and MongoDB.

```bash
cargo build                         # server
cd frontend && npm install          # web client
```

## Running locally

The simplest way is Docker (see [deployment.md](deployment.md)). To work on the code:

1. Start only the databases and publish their ports on your machine, for example:

   ```bash
   docker run -d --name dc-pg -e POSTGRES_USER=dragon -e POSTGRES_PASSWORD=dev      -e POSTGRES_DB=dragoncity -p 127.0.0.1:25432:5432 postgres:16-alpine
   docker run -d --name dc-mongo -p 127.0.0.1:27018:27017 mongo:7
   ```

2. Point the server at them and start it (`serve` applies the migrations itself):

   ```bash
   export POSTGRES_HOST=127.0.0.1 POSTGRES_PORT=25432 POSTGRES_PASSWORD=dev
   export MONGO_URL=mongodb://127.0.0.1:27018
   export BIND_ADDR=127.0.0.1:8080
   cargo run -- create-admin --email dev@example.com --password 'dev-password-1'
   cargo run -- serve                       # http://127.0.0.1:8080
   ```

3. In another terminal, start the web client with hot reload:

   ```bash
   cd frontend && npm run dev                    # http://localhost:5173
   ```

   Vite proxies `/api`, `/static`, `/flash`, `/assets`, `/deploy` and `/stream` to the server
   (set `VITE_BACKEND_URL` if it is not on `127.0.0.1:8080`).

> Some ports are reserved by Windows (a `bind ... forbidden` error): pick another one.
>
> If your editor runs `cargo check` in the background (rust-analyzer), builds from the terminal
> can fail with "file in use". Give your terminal its own target directory:
> `CARGO_TARGET_DIR=target/cli cargo test`.

## Tests

```bash
cargo test                                       # everything
cargo test --test game a_goal_pays_out_once      # one test
```

The suite needs no services: the application runs on the in-memory stores in `src/db/memory.rs`
(see `tests/common/mod.rs`, which also has a small HTTP client that keeps the session cookie).
What is covered:

| File | Covers |
|---|---|
| `src/**` (`#[cfg(test)]`) | Unit tests: validation, breeding, inventory, black market, signing, passwords... |
| `tests/api.rs` | Sign-up, login, logout, validation, enumeration, hashing, rate limit; CSRF and `Origin: null`; the web client and its security headers; Deus; the admin API; the game versions. |
| `tests/game.rs` | The client-injection attempts listed in [SECURITY.md](SECURITY.md), the rewards that must not repeat, training, PvP, the market, plus a normal play session. |
| `tests/social.rs` | The friend request flow (send, accept, decline, cancel, remove, limits, cooldown), the 5-gem reward, neighbours in `get_player_info`, friends-only visits and assists, the admin friend endpoints, the Allies Market, mystery eggs. |

Tests that exercise the game use `TestApp::player()` (a registered, logged-in client), the
`client.packet(&[("cmd", json!([args])), ...])` helper, and `app.edit_save()` / `app.give()` to
set up a scenario directly in the store.

The real PostgreSQL and MongoDB implementations are not covered by the suite (it has no
services). After touching `db/postgres.rs` or `db/mongo.rs`, start the two containers from
"Running locally" and play a short session against them.

The web client has no automated tests; it is type-checked and built with `npm run build`.

## Load testing

`examples/bench.rs` is a small load generator:

```bash
cargo run --release --example bench -- 127.0.0.1:8080 5     # address, seconds per scenario
```

It registers a throw-away account and hammers `packet.php`, `get_player_info` and the game
config. Only point it at a server whose data you do not care about.

## Database migrations

The PostgreSQL schema is a list of plain SQL files in `migrations/`, applied in order by
`dragon-city migrate` (and by `serve`) and recorded in `_sqlx_migrations`.

1. Add `migrations/NNNN_description.sql` with the next number. Never edit a migration that has
   already been released.
2. Update the structs in `src/db/mod.rs` and the queries in `src/db/postgres.rs`, and the
   in-memory store in `src/db/memory.rs` so the tests keep working.
3. Write the SQL so it can run on a database that already has the change (`IF NOT EXISTS`):
   databases created by the old Python version were adopted this way.

## Adding a command

A command is an `async fn(&mut Ctx) -> CmdResult<()>` in one of the files in `src/commands/`
(pick the theme that fits, or add a file):

1. Add the wire name to the `commands!` list in `enums.rs`.
2. Write the handler:

   ```rust
   pub async fn my_command(ctx: &mut Ctx<'_>) -> CmdResult<()> {
       let iso_id = ctx.iso_arg(0, "iso id")?;               // validated: a positive integer, as text
       let amount = ctx.int_arg(1, "amount", 1, 100)?;       // validated: an integer within a range
       ctx.map_item(&iso_id)?;                               // rejects when the object is not on the map

       ctx.pay_gold(amount * 10)?;                           // rejects when the player cannot afford it
       ctx.add_resource(Resource::Xp, 5);
       ctx.result = Value::from("ok");                       // optional: what the client gets back
       Ok(())
   }
   ```

3. Add it to the `match` in `commands::run` (`commands/mod.rs`). The compiler refuses to build
   until every `Command` has a handler.
4. Follow the rules the security model depends on:
   - read **every** argument through the `ctx.*_arg` helpers (never `ctx.args[i]` directly,
     except to keep a value exactly as sent after validating it);
   - validate **before** changing anything, and charge with `ctx.pay*`, never by clamping at 0;
   - values that grant resources must come from the server's config or state, not from the
     client's arguments;
   - ids chosen by the client (`ctx.new_uid`) must be checked for collisions;
   - return `reject("...")` for anything wrong: the command becomes a no-op and the reason is
     logged. A `CommandError::Failed` (the save did not have the expected shape) is rolled back
     too. Handlers must not panic: use `set_field` and the `ensure_*` helpers instead of
     indexing (`value["key"] = ...` panics on a value that is not an object).
5. Write a test in `tests/game.rs` (or a new file): the valid case, and each way a client could
   abuse it.

Rule helpers live in `game/`; put logic shared by several commands there (see `inventory.rs`,
`dragonarium.rs`, `training.rs`, `combat.rs`). Do not keep a `rand::rng()` alive across an
`.await`: it is not `Send`, and the request would not compile.

## Web client

`frontend/` is Vite + React + TypeScript + Tailwind + shadcn/ui.

- `src/lib/api.ts` is the fetch wrapper (`ApiError` carries the server's field errors);
  `src/lib/auth.tsx` the auth context.
- `src/pages/` has the pages, `src/components/game/` the Ruffle integration
  (`ruffle.ts` builds the load options and the URL rewrite rules).
- Components come from shadcn: add one with `npx shadcn@latest add <name>`; don't hand-write
  UI primitives. Forms use `Controller` with the `Field` component and a zod schema; text uses
  the `Typography` component and links the `Link` component.
- Ruffle itself is loaded by a `<script>` tag in `frontend/index.html` (from unpkg).

Build: `npm run build` (output in `frontend/dist`, which the server serves; the Docker image
builds it for you).

## Conventions

- Rust code is formatted with `cargo fmt` and compiles without warnings; short doc comments on
  non-obvious functions. Logs go through `tracing`, in English, never `println!`.
- Commit messages explain the *why*; keep commits focused.
- Never commit `.env`, game files or build output (see `.gitignore`).

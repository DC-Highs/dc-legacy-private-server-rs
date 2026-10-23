
<div align="center">
  <img width="330" height="230" alt="Dragon City" src="https://github.com/user-attachments/assets/1d1d8f52-cb5d-429e-bb72-e43ee86b1a44" />
</div>

**Dragon City Legacy Server** — a historical preservation project for the original
Adobe Flash version of Dragon City (2012–2020), kept alive by the community.

[![Status](https://img.shields.io/badge/Status-In_Development-orange)]()
[![Rust](https://img.shields.io/badge/Rust-axum%20%2B%20tokio-orange.svg)]()
[![Docker](https://img.shields.io/badge/Docker-Compose-2496ED)]()
[![Commands](https://img.shields.io/badge/Commands_Implemented-60%2F104-yellow)]()
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

---

## ⚠️ Legal notice

**This project is strictly for educational and historical preservation purposes.**
*Dragon City* and all of its visual, audio and brand assets are the intellectual property of
**SocialPoint / Take-Two Interactive**.

This server:

- is **not** for profit and does not distribute the game commercially;
- does **not** compete with the current versions of the game (mobile / Windows);
- operates under the exemptions to **Section 1201 of the DMCA** for the preservation of video
  games whose original authentication servers have been permanently shut down;
- uses protocols and assets of the **Adobe Flash** version, a platform officially discontinued
  in December 2020.

If you legally represent SocialPoint or Take-Two Interactive and have any concern, please
contact the maintainers before taking any formal action.

The game files (`flash/`, `assets/`, `domains/`) are **not** part of the container image and
are not covered by the MIT license.

---

## What it is

A server that replaces the original Dragon City back end, plus a web client that runs the
Flash game in the browser with [Ruffle](https://ruffle.rs/).

- **Play in the browser:** the game fills almost the whole screen under a small header with
  log in / log out. No Flash Player and no hosts-file tricks.
- **Accounts:** sign up with an email, a password and its confirmation (no email code). Each
  account is one player.
- **Admin panel:** administrators create and manage users from `/admin`.
- **Friends:** players send each other friend requests by player id on the web client's
  Friends page; friends become neighbours in the game (visit their island, help them), and both
  get 5 gems for each of their first 5 friends.
- **Storage:** accounts and sessions in PostgreSQL, each player's save in MongoDB.
- **Fast:** one async Rust binary (axum + tokio): no interpreter, no GIL, a few MB of RAM, and
  the hot paths (game config, sessions, the player lock) are served from memory. Numbers are in
  [docs/architecture.md](docs/architecture.md#performance).
- **Hardened:** the client is treated as untrusted; every command is validated server-side.
  See [docs/SECURITY.md](docs/SECURITY.md).
- **Docker:** `docker compose up` runs everything, on your PC or on a VPS.

### Status

| Area | Status |
|---|---|
| Breeding engine | ✅ Working |
| Game commands (`packet.php`) | ⚠️ 60 / 104 implemented |
| Accounts, login, admin panel | ✅ Working |
| Saves in MongoDB, accounts in PostgreSQL | ✅ Working |
| Docker deployment | ✅ Working |
| Market and minigame pages | ❌ No web UI (the API is still there) |
| Friends (requests, neighbours, visiting, the 5 gems) | ✅ Working |

Commands that are not implemented are answered with a generic `ok` so the client keeps
running.

---

## Screenshots

> **The game running in the browser**

![The game](https://i.ibb.co/HL9YSpqb/image.png)

> **The sign-up page**

![Sign up](https://i.ibb.co/JW8nf19c/image.png)

> **The admin panel**

![Admin panel](https://i.ibb.co/39Wx71d3/image.png)

> **The Friends page**

![The Friends page](https://i.ibb.co/LXPJrLdm/image.png)

The full list of screenshots the documentation expects is in [docs/images/README.md](docs/images/README.md).

---

## Quick start

You need Docker with Docker Compose, and the game files in `flash/`, `assets/` and `domains/`.

```bash
cp .env.example .env            # set POSTGRES_PASSWORD, MONGO_PASSWORD and ADMIN_*
docker compose up -d --build
```

Open <http://localhost:8080>, create an account and play. If you set `ADMIN_EMAIL` and
`ADMIN_PASSWORD`, that administrator is created on start-up; otherwise create one with:

```bash
docker compose exec app /app/dragon-city create-admin --email you@example.com --password '...'
```

Running it on a server? Read [docs/deployment.md](docs/deployment.md): put an HTTPS reverse
proxy in front of it and set `COOKIE_SECURE=true` and `TRUST_PROXY=true`.

---

## Documentation

| Document | What is in it |
|---|---|
| [docs/deployment.md](docs/deployment.md) | Running with Docker, on a VPS, behind a proxy, backups, updates |
| [docs/configuration.md](docs/configuration.md) | Every environment variable and the game-file volumes |
| [docs/architecture.md](docs/architecture.md) | Components, request flow, data model, design decisions |
| [docs/api.md](docs/api.md) | The JSON API used by the web client and the admin panel |
| [docs/game-protocol.md](docs/game-protocol.md) | The Flash client's endpoints and the `packet.php` commands |
| [docs/administration.md](docs/administration.md) | The admin panel and the command line |
| [docs/development.md](docs/development.md) | Local setup, tests, migrations, adding a command |
| [docs/SECURITY.md](docs/SECURITY.md) | Threat model, fixed vulnerabilities, known limits |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Tasks and ideas for the future |
| [docs/images/README.md](docs/images/README.md) | The screenshots the documentation is missing |

---

## Project layout

```
src/
├── main.rs, lib.rs     # command line (serve, migrate, create-admin, seed, healthcheck)
├── app.rs              # router, start-up tasks, in-memory application for the tests
├── config.rs           # settings read from environment variables
├── db/                 # the store traits; PostgreSQL, MongoDB and in-memory implementations
├── auth/               # Argon2id passwords, sessions, CSRF/origin checks
├── api/                # JSON API: auth, admin, friends, game bootstrap
├── commands/           # the packet.php commands (grouped by theme), dispatcher, validation
├── game/               # rules: breeding, black market, market, PvP, dragonarium...
├── routes/             # the original game's endpoints and the game files
├── storage.rs          # player saves: locks, pending assists, normalization
└── friends.rs, ...     # friendships, game versions, Deus, packet signing
migrations/             # PostgreSQL migrations (sqlx)
frontend/               # web client (Vite, React, shadcn/ui)
tests/                  # integration tests (in-memory stores, no Docker needed)
examples/bench.rs       # load generator
web/srv/                # read-only seed data: game config, player template, PvP opponents
docs/                   # documentation
```

---

## About digital heritage preservation

- [Exemption to PCCPSACT](https://www.federalregister.gov/documents/2018/10/26/2018-23241/exemption-to-prohibition-on-circumvention-of-copyright-protection-systems-for-access-control) — exemptions to Section 1201 of the DMCA.
- [EFGAMP](https://efgamp.eu/) — European Federation of Game Archives, Museums and Preservation Projects.
- [BlueMaxima's Flashpoint](https://bluemaxima.org/flashpoint/) — the definitive web-game preservation project.
- [The Internet Archive](https://archive.org/) — a digital library of cultural artifacts.
- [UNESCO PERSIST Programme](https://unescopersist.org/) — continued access to digital information.
- [Adobe Flash Player Archive](https://archive.org/download/flashplayerarchive/) — historical Flash Player archive.

---

## License

The server source code (`src/`, `frontend/`, `migrations/`, `tests/`, `docs/`) is under the
[MIT License](LICENSE).

The original game assets (`.swf`, images, audio) are **not** covered by that license: they
belong to SocialPoint / Take-Two Interactive.

Authors: [Gabriel18017](https://github.com/Gabriel18017) and [Marcuth](https://github.com/Marcuth).

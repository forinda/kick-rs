# users-api

> **Phase 2 example** for [kick-rs](../../README.md) — a minimal users
> CRUD service on Postgres via sqlx, exercising the framework's
> Container + Module + Adapter wiring end-to-end against a real DB.

This example is **not** a published crate. It lives in `examples/` and
is deliberately excluded from the workspace so its `sqlx` /
`tracing-subscriber` deps don't slow down `cargo build --workspace`
for the framework itself.

## What it demonstrates

- `bootstrap()` driving the full kick-rs lifecycle against axum
- `define_module()` registering services + routes
- `service_factory(|c| ...)` resolving dependencies from the container
- `Inject<T>` extractor in handlers
- `HttpError` / `HttpResult<T>` mapping service errors to RFC 7807 JSON
- In-process migrations via `sqlx::migrate!()` (no extra CLI step at
  startup — migrations are embedded at compile time and run on boot)

## Prerequisites

- Rust stable (1.78+)
- Docker (for the bundled Postgres)
- *Optional:* `sqlx-cli` if you want to run migrations manually:

  ```bash
  cargo install sqlx-cli --no-default-features --features postgres
  ```

## Running

```bash
# from this directory:
cp .env.example .env

docker compose up -d           # boots postgres on :5432
cargo run                      # connects, migrates, serves on :3000
```

Then:

```bash
curl -s http://localhost:3000/users
# []

curl -s -X POST http://localhost:3000/users \
  -H 'Content-Type: application/json' \
  -d '{"email":"alice@example.com","name":"Alice"}'
# {"id":"...","email":"alice@example.com","name":"Alice", ...}

curl -s http://localhost:3000/users/<id>
curl -s -X PATCH http://localhost:3000/users/<id> \
  -H 'Content-Type: application/json' \
  -d '{"name":"Alice B."}'
curl -s -X DELETE http://localhost:3000/users/<id>
```

## Migrations

The example uses **sqlx-cli's reversible migrations** (`.up.sql` +
`.down.sql` pairs). They're auto-applied at startup via
`sqlx::migrate!()`. To manage them manually:

```bash
# requires sqlx-cli + DATABASE_URL exported
export DATABASE_URL=postgres://users:users@localhost:5432/users_api

sqlx migrate add -r <name>     # create a new reversible migration
sqlx migrate info              # show applied + pending
sqlx migrate run               # apply pending up-migrations
sqlx migrate revert            # roll back the most recent one
```

### Alternatives in the Rust ecosystem

For different needs:

| Tool                  | Style                              | Up/down |
|-----------------------|------------------------------------|---------|
| `sqlx-cli` *(used here)* | `.sql` files                    | Both modes — simple OR reversible |
| `refinery`            | `.sql` or Rust-typed migrations    | Forward-only by default |
| `diesel_cli`          | Directory per migration            | `up.sql` + `down.sql` always |
| `sea-orm-migration`   | Rust code (DSL, DB-agnostic)       | `async fn up` / `async fn down` |

Adopters of kick-rs are free to swap out the migration strategy — none
of it is in the framework.

## Repo layout

```
examples/users-api/
├── Cargo.toml
├── compose.yml                      # postgres for local dev
├── .env.example                     # copy to .env
├── migrations/
│   ├── 20260522120000_create_users.up.sql
│   └── 20260522120000_create_users.down.sql
└── src/
    ├── main.rs                      # bootstrap entrypoint
    ├── config.rs                    # env loading
    └── modules/
        ├── mod.rs
        └── users/
            ├── mod.rs               # `define()` -> kick-rs::Module
            ├── model.rs             # User, CreateUser, UpdateUser
            ├── repository.rs        # sqlx CRUD
            ├── service.rs           # business logic + typed errors
            └── handlers.rs          # axum handlers
```

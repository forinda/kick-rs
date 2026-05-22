# multi-tenant-api

> **Phase 5.3 example** for [kick-rs](../../README.md) — a multi-tenant
> CRUD service using **schema-per-tenant Postgres** + per-tenant
> connection pools, wired entirely through context contributors.

This example is the canonical demo of the **tenant-DB factory** pattern:
identify the tenant from a header, look up (or lazily build) that
tenant's `PgPool`, and hand it to handlers as a typed `Ctx<TenantDb>`.
Handlers stay tenant-naive — they just run `SELECT * FROM posts` and
the right schema is selected by the connection's `search_path`.

## How it works

```
Request          ──► HeaderMap injected by HTTP middleware (framework-ambient)
                       │
                       ▼
              LoadTenant      contributor — reads `X-Tenant-Slug`
                       │      header; validates against `TenantsAllowlist`
                       ▼
              Tenant         (request-scoped value)
                       │
                       ▼
              LoadTenantDb   contributor — DI-injects `TenantPoolRegistry`,
                       │      calls `pool_for(&tenant.slug)` which lazily
                       │      builds & caches a `PgPool` with the right
                       │      `search_path = tenant_<slug>,public`
                       ▼
              TenantDb        (request-scoped value)
                       │
                       ▼
           async fn list(db: Ctx<TenantDb>) -> Json<Vec<Post>>
              ↑
              handler — no routing logic, no schema names, just plain SQL
```

The pipeline is topo-sorted at boot, so missing `Tenant` producers or
cyclic deps fail before any request runs.

## Running

```bash
cp .env.example .env
docker compose up -d
cargo run                     # listens on http://0.0.0.0:3001
```

Each tenant sees its own data:

```bash
# create a post in tenant `acme`
curl -X POST -H 'X-Tenant-Slug: acme' \
  -H 'Content-Type: application/json' \
  -d '{"title":"hello","body":"from acme"}' \
  http://localhost:3001/posts

# acme sees it
curl -H 'X-Tenant-Slug: acme' http://localhost:3001/posts
# [{"id":"...","title":"hello","body":"from acme", ...}]

# globex doesn't — isolated by schema
curl -H 'X-Tenant-Slug: globex' http://localhost:3001/posts
# []

# unknown tenant → structured error
curl -H 'X-Tenant-Slug: hooli' http://localhost:3001/posts
# 500
# {"code":"RK_A_TENANT_UNKNOWN","title":"`hooli` is not a known tenant", ...}

# missing header → structured error
curl http://localhost:3001/posts
# 500
# {"code":"RK_A_TENANT_MISSING","title":"required header `x-tenant-slug` is missing", ...}
```

The framework's `LoadTenant`/`LoadTenantDb` errors map to RFC 7807
problem-details JSON automatically via `HttpError`.

## Layout

```
examples/multi-tenant-api/
├── Cargo.toml
├── compose.yml                   # postgres on :5433 (so users-api can run too)
├── .env.example
├── migrations/
│   ├── 20260522130000_setup_tenants.up.sql        # schemas + tables for acme/globex/initech
│   └── 20260522130000_setup_tenants.down.sql
└── src/
    ├── main.rs                   # bootstrap: migrate, wire infra module, register contributors
    ├── config.rs                 # env loader (incl. TENANTS allowlist)
    ├── tenancy/
    │   ├── mod.rs
    │   ├── tenant.rs             # Tenant + LoadTenant contributor (reads X-Tenant-Slug)
    │   ├── tenant_db.rs          # TenantDb + LoadTenantDb contributor
    │   └── registry.rs           # TenantPoolRegistry — lazy per-tenant pool factory
    └── modules/
        └── posts/
            ├── mod.rs            # CRUD routes
            ├── model.rs          # Post, CreatePost
            └── handlers.rs       # plain SQL, no tenant routing
```

## What this shows

| Pattern | Where |
|---|---|
| Header-driven tenant resolution | `tenancy/tenant.rs` uses `Deps = (HeaderMap,)` — framework injects the headers into the contributor store automatically |
| Tenant allowlist via DI | `LoadTenant` calls `ctx.inject::<TenantsAllowlist>()` — added Phase 5.2 |
| Per-tenant pool factory | `TenantPoolRegistry` with double-checked locking; first request for a tenant creates the pool, subsequent ones reuse |
| Typed dep chain | `LoadTenantDb` declares `Deps = (Tenant,)` — pipeline topo-sort ensures `LoadTenant` runs first |
| Handler ergonomics | `async fn list(db: Ctx<TenantDb>) -> ...` — no `if tenant.is_acme()`-style branching anywhere |
| Errors as RFC 7807 | `KickError` → `HttpError` → problem-details JSON automatically |

## Tweaking

- **Adding a tenant**: append the slug to `TENANTS=` in `.env`, add a
  `CREATE SCHEMA tenant_<slug>; CREATE TABLE tenant_<slug>.posts (...)`
  migration. No code changes.
- **JWT-derived tenant**: swap `LoadTenant` to read the JWT from the
  `Authorization` header (still `Deps = (HeaderMap,)`).
- **Per-tenant database (not schema)**: change `TenantPoolRegistry` to
  build a fresh `DATABASE_URL` per tenant rather than setting
  `search_path`.

## Status

Same as the framework — pre-`v0.1.0`. The example tracks `main` for
illustrative purposes; the patterns shown here are stable.

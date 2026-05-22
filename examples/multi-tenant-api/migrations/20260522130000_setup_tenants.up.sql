-- Schema-per-tenant layout. Each tenant gets an isolated Postgres
-- schema with the same `posts` table shape; the app's `LoadTenantDb`
-- contributor selects the schema at request time via a per-tenant
-- connection pool configured with `search_path`.

CREATE SCHEMA IF NOT EXISTS tenant_acme;
CREATE SCHEMA IF NOT EXISTS tenant_globex;
CREATE SCHEMA IF NOT EXISTS tenant_initech;

CREATE TABLE IF NOT EXISTS tenant_acme.posts (
    id         UUID        PRIMARY KEY,
    title      TEXT        NOT NULL,
    body       TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tenant_globex.posts (
    id         UUID        PRIMARY KEY,
    title      TEXT        NOT NULL,
    body       TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tenant_initech.posts (
    id         UUID        PRIMARY KEY,
    title      TEXT        NOT NULL,
    body       TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

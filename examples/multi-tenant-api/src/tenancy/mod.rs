//! Tenancy layer — the heart of this example.
//!
//! Two contributors, wired in `Deps` order:
//!
//! 1. [`LoadTenant`] reads the `X-Tenant-Slug` header (or a future
//!    JWT-derived value) and produces a [`Tenant`]. Depends on
//!    `HeaderMap` (framework-injected ambient type).
//!
//! 2. [`LoadTenantDb`] depends on `Tenant` and DI-injects the
//!    [`TenantPoolRegistry`] to produce a [`TenantDb`] handle wrapping
//!    the right per-tenant connection pool. Pools are created lazily on
//!    first use and cached for the process lifetime.
//!
//! Handlers then take `Ctx<TenantDb>` and execute queries against the
//! tenant's schema without ever naming the schema directly — search_path
//! on the connection takes care of it.

pub mod registry;
pub mod tenant;
pub mod tenant_db;

pub use registry::TenantPoolRegistry;
pub use tenant::{LoadTenant, Tenant, TenantsAllowlist};
pub use tenant_db::{LoadTenantDb, TenantDb};

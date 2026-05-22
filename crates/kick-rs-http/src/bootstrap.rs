//! App entry-point builder. See [`SPEC.md` §4.14](../SPEC.md#414-bootstrap).
//!
//! Lifecycle implemented in Phase 1.4:
//!
//! 1. Fold every [`HttpModule`] provider into a [`Container`]
//! 2. Topo-sort adapters by `depends_on`
//! 3. Run `before_mount` for each
//! 4. Build the `axum::Router` from module routes
//! 5. Attach `Extension(Container)` so [`crate::Inject`] can find it
//! 6. Run `before_start`
//! 7. Bind a TCP listener and serve with graceful-shutdown on Ctrl-C
//! 8. Run `after_start` once bound (before the serve loop blocks)
//! 9. On shutdown, run every adapter's `shutdown()` concurrently via
//!    `tokio::join!` (well, `futures::join_all` since the set is dynamic),
//!    with a per-adapter timeout budget

use crate::module::HttpModule;
use axum::Extension;
use futures::future::join_all;
use kick_rs_core::{
    mount_sort::{topo_sort, MountItem},
    Adapter, AdapterContext, Container, KickError, KickResult, Plugin,
};
use std::sync::Arc;
use std::time::Duration;

/// Default per-adapter shutdown budget. Each adapter gets this long to
/// flush before the framework moves on.
const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Begin assembling an app.
pub fn bootstrap() -> Bootstrap {
    Bootstrap::default()
}

/// Fluent builder for an app. Terminal method is [`Bootstrap::listen`].
#[derive(Default)]
pub struct Bootstrap {
    modules: Vec<HttpModule>,
    adapters: Vec<Arc<dyn Adapter>>,
    plugins: Vec<Arc<dyn Plugin>>,
    /// HTTP-aware plugins, tracked separately so their `http_modules()`
    /// can be aggregated alongside user-provided modules.
    http_plugins: Vec<Arc<dyn crate::HttpPlugin>>,
    /// Contributors registered globally on the bootstrap builder, used
    /// for cross-cutting per-request values (e.g. `CurrentUser` derived
    /// from auth headers). Combined with module- and plugin-level
    /// contributors into a single topo-sorted pipeline at boot.
    global_contributors: Vec<kick_rs_core::AnyContributor>,
    shutdown_timeout: Option<Duration>,
}

impl Bootstrap {
    /// Mount a module.
    pub fn module(mut self, m: HttpModule) -> Self {
        self.modules.push(m);
        self
    }

    /// Mount multiple modules at once.
    pub fn modules<I: IntoIterator<Item = HttpModule>>(mut self, ms: I) -> Self {
        self.modules.extend(ms);
        self
    }

    /// Register an adapter instance (singleton lifecycle).
    pub fn adapter<A: Adapter + 'static>(mut self, a: A) -> Self {
        self.adapters.push(Arc::new(a));
        self
    }

    /// Register a transport-agnostic plugin. Use [`Self::http_plugin`]
    /// if the plugin ships HTTP routes via [`HttpPlugin::http_modules`](crate::HttpPlugin::http_modules).
    pub fn plugin<P: Plugin + 'static>(mut self, p: P) -> Self {
        self.plugins.push(Arc::new(p));
        self
    }

    /// Register an HTTP-aware plugin. Behaves like [`Self::plugin`] but
    /// additionally folds the plugin's [`HttpModule`]s into the app
    /// router alongside user-provided modules.
    ///
    /// Use this when the plugin ships handler endpoints
    /// (e.g. `/auth/login`, `/health`, `/__debug`). Plain `Plugin`
    /// authors that don't carry routes can stick with [`Self::plugin`].
    pub fn http_plugin<P: crate::HttpPlugin + 'static>(mut self, p: P) -> Self {
        let arc: Arc<P> = Arc::new(p);
        // Store under both the `Plugin` and `HttpPlugin` views so the
        // base lifecycle (depends_on, contributors, adapters, modules)
        // and the HTTP extension (http_modules) both find it.
        self.plugins.push(Arc::clone(&arc) as Arc<dyn Plugin>);
        self.http_plugins.push(arc as Arc<dyn crate::HttpPlugin>);
        self
    }

    /// Register a [`ContextContributor`](kick_rs_core::ContextContributor)
    /// globally — runs on every request regardless of which module owns
    /// the handler. Typical use: auth-derived `CurrentUser`, request-id
    /// extension, multi-tenant routing.
    ///
    /// Module- and plugin-level contributors run alongside these in
    /// the same topo-sorted pipeline; missing or cyclic deps surface
    /// at boot with `RK_E_MISSING_CONTRIBUTOR` / `RK_E_CONTRIBUTOR_CYCLE`.
    pub fn contribute<C: kick_rs_core::ContextContributor>(mut self, c: C) -> Self {
        self.global_contributors
            .push(kick_rs_core::erase_contributor(c));
        self
    }

    /// Override the per-adapter shutdown timeout (default: 10s).
    pub fn shutdown_timeout(mut self, d: Duration) -> Self {
        self.shutdown_timeout = Some(d);
        self
    }

    /// Build the application as an [`axum::Router`] without binding any
    /// socket. Useful for tower-style testing via `ServiceExt::oneshot`.
    /// Errors are surfaced from container or topo-sort validation.
    pub fn into_router(self) -> KickResult<(axum::Router, AppState)> {
        let Bootstrap {
            modules,
            adapters,
            plugins,
            http_plugins,
            global_contributors,
            shutdown_timeout: _,
        } = self;

        // 1. Aggregate every input from every source. Plugins
        //    contribute providers (via register), adapters, core
        //    modules (transport-agnostic), and contributors. HTTP
        //    plugins additionally contribute HttpModules.

        // 1a. Modules: user-provided + http_plugin.http_modules().
        let mut modules: Vec<HttpModule> = modules;
        for hp in &http_plugins {
            modules.extend(hp.http_modules());
        }

        // 1b. Adapters: user-provided + plugin.adapters() for every plugin.
        let mut adapters: Vec<Arc<dyn Adapter>> = adapters;
        for p in &plugins {
            adapters.extend(p.adapters());
        }

        // 2. Fold module providers into a container.
        let mut builder = Container::builder();
        for m in &modules {
            builder = m.register_into(builder);
        }
        // Plugin::register() — give plugins a chance to bind their
        // singletons before container.build() seals the graph.
        for p in &plugins {
            p.register(&mut builder)?;
        }
        // Plugin::modules() (core transport-agnostic) also contribute
        // providers — fold them in too.
        for p in &plugins {
            for cm in p.modules() {
                builder = cm.register_into(builder);
            }
        }
        let container = builder.build()?;

        // 3. Topo-sort adapters by depends_on.
        let mount_items: Vec<MountAdapter> = adapters.into_iter().map(MountAdapter::from).collect();
        let sorted = topo_sort(mount_items)?;
        let adapters: Vec<Arc<dyn Adapter>> = sorted.into_iter().map(|m| m.inner).collect();

        // 4. Gather contributors from every source — bootstrap-global,
        //    modules, plugins, plugin-supplied core modules, and adapters.
        //    Matches KickJS's 5-level precedence sites
        //    (method > class > module > adapter > global). Topo-sort
        //    once. Missing/cyclic deps fail boot.
        let mut contributors = global_contributors;
        for m in &modules {
            contributors.extend(m.collect_contributors());
        }
        for p in &plugins {
            contributors.extend(p.contributors());
            for cm in p.modules() {
                contributors.extend(cm.collect_contributors());
            }
        }
        for a in &adapters {
            contributors.extend(a.contributors());
        }
        let pipeline = Arc::new(crate::ContributorPipeline::build(contributors)?);

        // 5. Build the router.
        let mut router = axum::Router::new();
        for m in modules {
            router = m.mount_onto(router);
        }

        // 5a. Install the contributor middleware FIRST so it ends up
        //     INNER in axum's layer stack. axum applies the last-added
        //     layer as the outermost wrapper, so on the request path:
        //         Extension(Container)  ← outermost (inserts container)
        //             ↓
        //         contributors_middleware  ← sees container in extensions,
        //                                    runs pipeline, inserts store
        //             ↓
        //         handler  ← sees both
        //     Reversing the order would have the middleware fire before
        //     the Extension inserts the container — which is exactly the
        //     bug that produced 500s before this swap landed.
        if !pipeline.is_empty() {
            let pipeline_for_layer = Arc::clone(&pipeline);
            router = router.layer(axum::middleware::from_fn(move |req, next| {
                let p = Arc::clone(&pipeline_for_layer);
                async move { crate::contributors_middleware(p, req, next).await }
            }));
        }

        // 5b. Install Extension(Container) LAST so it wraps outermost
        //     and the container is in request extensions before the
        //     contributors middleware needs it.
        router = router.layer(Extension(container.clone()));

        Ok((
            router,
            AppState {
                container,
                adapters,
                plugins,
            },
        ))
    }

    /// Start the server. Binds to `addr`, runs the full lifecycle, and
    /// gracefully shuts down on Ctrl-C / SIGINT.
    pub async fn listen(self, addr: &str) -> KickResult<()> {
        let shutdown_timeout = self.shutdown_timeout.unwrap_or(DEFAULT_SHUTDOWN_TIMEOUT);
        let (router, state) = self.into_router()?;
        let ctx = AdapterContext {
            container: state.container.clone(),
        };

        // 3 (deferred). before_mount — runs against the built container.
        for a in &state.adapters {
            a.before_mount(&ctx).await?;
        }

        // 6. before_start.
        for a in &state.adapters {
            a.before_start(&ctx).await?;
        }

        // 7. Bind + serve.
        let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
            KickError::new("RK_H_BIND_FAILED", format!("could not bind {addr}: {e}")).with_source(e)
        })?;
        let local = listener
            .local_addr()
            .ok()
            .map(|a| a.to_string())
            .unwrap_or_else(|| addr.to_owned());
        tracing::info!(target: "kick-rs", addr = %local, "listening");

        // 8. after_start — fire-and-forget; if any adapter errors here we
        //    log it (we don't fail the server since it's already serving).
        for a in &state.adapters {
            if let Err(e) = a.after_start(&ctx).await {
                tracing::warn!(target: "kick-rs", adapter = %a.name(), error = %e, "after_start error");
            }
        }

        // 8b. Plugin::on_ready — post-startup task. Errors are logged
        //     but never abort serving (matches KickJS `onReady()`).
        for p in &state.plugins {
            if let Err(e) = p.on_ready(&state.container).await {
                tracing::warn!(target: "kick-rs", plugin = %p.name(), error = %e, "on_ready error");
            }
        }

        let shutdown_signal = async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!(target: "kick-rs", "shutdown signal received");
        };

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(|e| {
                KickError::new("RK_H_SERVE_FAILED", format!("axum::serve failed: {e}"))
                    .with_source(e)
            })?;

        // 9. Cooperative shutdown — adapters first (own connection state),
        //    then plugins (own anything outside an adapter's purview).
        shutdown_adapters(&state.adapters, shutdown_timeout).await;
        shutdown_plugins(&state.plugins, shutdown_timeout).await;
        Ok(())
    }
}

/// State produced by [`Bootstrap::into_router`]. Mostly useful for tests
/// and embedded scenarios where the caller drives the lifecycle manually.
pub struct AppState {
    /// The DI container.
    pub container: Container,
    /// Adapters, already topo-sorted.
    pub adapters: Vec<Arc<dyn Adapter>>,
    /// Plugins (no topo-sort yet — Phase 1.4 does not surface plugin
    /// providers at runtime; their layers/contributors land later).
    pub plugins: Vec<Arc<dyn Plugin>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let adapter_names: Vec<_> = self.adapters.iter().map(|a| a.name()).collect();
        let plugin_names: Vec<_> = self.plugins.iter().map(|p| p.name()).collect();
        f.debug_struct("AppState")
            .field("container", &self.container)
            .field("adapters", &adapter_names)
            .field("plugins", &plugin_names)
            .finish()
    }
}

// ─────────────────────────── Adapter topo-sort ─────────────────────────────

/// Adapter wrapped so it can participate in [`topo_sort`]. Owns its name as
/// `String` to allow scoped-adapter names like `"postgres:reads"`.
struct MountAdapter {
    name: String,
    inner: Arc<dyn Adapter>,
}

impl From<Arc<dyn Adapter>> for MountAdapter {
    fn from(inner: Arc<dyn Adapter>) -> Self {
        Self {
            name: inner.name().to_owned(),
            inner,
        }
    }
}

impl MountItem for MountAdapter {
    fn name(&self) -> &str {
        &self.name
    }
    fn depends_on(&self) -> &[&str] {
        self.inner.depends_on()
    }
}

// ──────────────────────────── Shutdown helper ──────────────────────────────

async fn shutdown_adapters(adapters: &[Arc<dyn Adapter>], per_timeout: Duration) {
    let futures = adapters.iter().map(|a| {
        let a = Arc::clone(a);
        async move {
            let name = a.name().to_owned();
            match tokio::time::timeout(per_timeout, a.shutdown()).await {
                Ok(Ok(())) => tracing::info!(target: "kick-rs", adapter = %name, "shut down cleanly"),
                Ok(Err(e)) => tracing::warn!(target: "kick-rs", adapter = %name, error = %e, "shutdown failed"),
                Err(_) => tracing::warn!(target: "kick-rs", adapter = %name, "shutdown timed out"),
            }
        }
    });
    join_all(futures).await;
}

async fn shutdown_plugins(plugins: &[Arc<dyn Plugin>], per_timeout: Duration) {
    let futures = plugins.iter().map(|p| {
        let p = Arc::clone(p);
        async move {
            let name = p.name().to_owned();
            match tokio::time::timeout(per_timeout, p.shutdown()).await {
                Ok(Ok(())) => {
                    tracing::info!(target: "kick-rs", plugin = %name, "shut down cleanly")
                }
                Ok(Err(e)) => {
                    tracing::warn!(target: "kick-rs", plugin = %name, error = %e, "shutdown failed")
                }
                Err(_) => tracing::warn!(target: "kick-rs", plugin = %name, "shutdown timed out"),
            }
        }
    });
    join_all(futures).await;
}

// ──────────────────────────────── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::define_module;
    use crate::Inject;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    #[derive(Debug)]
    struct Counter {
        n: AtomicUsize,
    }
    impl Counter {
        fn bump(&self) -> usize {
            self.n.fetch_add(1, Ordering::SeqCst) + 1
        }
    }

    async fn count(c: Inject<Counter>) -> String {
        c.bump().to_string()
    }

    #[tokio::test]
    async fn into_router_builds_a_working_app() {
        let m = define_module("c")
            .service_value(Counter {
                n: AtomicUsize::new(0),
            })
            .get("/count", count)
            .build();

        let (router, _state) = bootstrap().module(m).into_router().unwrap();

        let res = router
            .clone()
            .oneshot(Request::get("/count").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 16).await.unwrap();
        assert_eq!(&body[..], b"1");

        let res2 = router
            .oneshot(Request::get("/count").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body2 = axum::body::to_bytes(res2.into_body(), 16).await.unwrap();
        assert_eq!(&body2[..], b"2", "singleton must persist across requests");
    }

    #[tokio::test]
    async fn into_router_surfaces_duplicate_provider_errors() {
        #[derive(Debug)]
        struct Foo;
        let m1 = define_module("a").service_value(Foo).build();
        let m2 = define_module("b").service_value(Foo).build();
        let err = bootstrap().module(m1).module(m2).into_router().unwrap_err();
        assert_eq!(err.code, "RK_E_AMBIGUOUS_BIND");
    }

    // ── adapter topo-sort behavior ──

    struct TestAdapter {
        name: String,
        deps: Vec<&'static str>,
        log: Arc<std::sync::Mutex<Vec<String>>>,
        phase: &'static str,
    }

    #[async_trait]
    impl Adapter for TestAdapter {
        fn name(&self) -> &str {
            &self.name
        }
        fn depends_on(&self) -> &[&str] {
            &self.deps
        }
        async fn before_start(&self, _ctx: &AdapterContext) -> KickResult<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}:{}", self.phase, self.name));
            Ok(())
        }
    }

    #[tokio::test]
    async fn adapters_are_topo_sorted_before_lifecycle_runs() {
        let log: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
        let a = TestAdapter {
            name: "a".into(),
            deps: vec![],
            log: Arc::clone(&log),
            phase: "start",
        };
        let b = TestAdapter {
            name: "b".into(),
            deps: vec!["a"],
            log: Arc::clone(&log),
            phase: "start",
        };
        let c = TestAdapter {
            name: "c".into(),
            deps: vec!["b"],
            log: Arc::clone(&log),
            phase: "start",
        };

        // Insert in reverse order — topo_sort must reorder them.
        let (_router, state) = bootstrap()
            .adapter(c)
            .adapter(b)
            .adapter(a)
            .into_router()
            .unwrap();

        let names: Vec<_> = state.adapters.iter().map(|a| a.name().to_owned()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);

        // Drive before_start manually so we can assert call order.
        let ctx = AdapterContext {
            container: state.container.clone(),
        };
        for ad in &state.adapters {
            ad.before_start(&ctx).await.unwrap();
        }
        assert_eq!(*log.lock().unwrap(), vec!["start:a", "start:b", "start:c"]);
    }

    #[tokio::test]
    async fn adapter_dependency_cycle_fails_bootstrap() {
        let log: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
        let a = TestAdapter {
            name: "a".into(),
            deps: vec!["b"],
            log: Arc::clone(&log),
            phase: "start",
        };
        let b = TestAdapter {
            name: "b".into(),
            deps: vec!["a"],
            log,
            phase: "start",
        };

        let err = bootstrap().adapter(a).adapter(b).into_router().unwrap_err();
        assert_eq!(err.code, "RK_E_MOUNT_CYCLE");
    }

    // ── End-to-end: module-registered contributor reachable via Ctx<T> ──────

    use crate::Ctx;
    use kick_rs_core::ContributorRequest;

    #[derive(Debug, Clone)]
    struct Tenant {
        slug: String,
    }

    struct LoadTenantFromHeader;
    impl kick_rs_core::ContextContributor for LoadTenantFromHeader {
        type Key = Tenant;
        type Deps = ();
        async fn resolve<'a>(
            &'a self,
            _ctx: &'a dyn ContributorRequest,
            _: (),
        ) -> KickResult<Tenant> {
            // A real impl would read from ctx (request headers via a
            // contributor that exposes them). For this end-to-end test
            // we just emit a fixed value.
            Ok(Tenant {
                slug: "acme".into(),
            })
        }
    }

    async fn tenant_handler(tenant: Ctx<Tenant>) -> String {
        tenant.slug.clone()
    }

    #[tokio::test]
    async fn contributor_pipeline_runs_per_request_and_ctx_extracts() {
        let m = define_module("tenancy")
            .contribute(LoadTenantFromHeader)
            .get("/tenant", tenant_handler)
            .build();

        let (router, _state) = bootstrap().module(m).into_router().unwrap();

        let res = router
            .oneshot(Request::get("/tenant").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 64).await.unwrap();
        assert_eq!(&body[..], b"acme");
    }

    // ── HttpPlugin: full mini-framework in a single package ───────────────

    struct HealthAdapter {
        log: Arc<std::sync::Mutex<Vec<String>>>,
    }
    #[async_trait]
    impl Adapter for HealthAdapter {
        fn name(&self) -> &str {
            "health"
        }
        async fn before_start(&self, _: &AdapterContext) -> KickResult<()> {
            self.log
                .lock()
                .unwrap()
                .push("health:before_start".to_owned());
            Ok(())
        }
    }

    #[derive(Debug, Clone)]
    struct AppVersion(&'static str);

    struct ReadAppVersion;
    impl kick_rs_core::ContextContributor for ReadAppVersion {
        type Key = AppVersion;
        type Deps = ();
        async fn resolve<'a>(
            &'a self,
            _: &'a dyn ContributorRequest,
            _: (),
        ) -> KickResult<AppVersion> {
            Ok(AppVersion("0.0.0-test"))
        }
    }

    async fn health_handler(v: Ctx<AppVersion>) -> String {
        // Ctx<AppVersion> -> deref -> AppVersion -> .0 -> &'static str.
        format!("ok {}", (*v).0)
    }

    struct HealthPlugin {
        log: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl Plugin for HealthPlugin {
        fn name(&self) -> &str {
            "health-plugin"
        }
        fn contributors(&self) -> Vec<kick_rs_core::AnyContributor> {
            vec![kick_rs_core::erase_contributor(ReadAppVersion)]
        }
        fn adapters(&self) -> Vec<Arc<dyn Adapter>> {
            vec![Arc::new(HealthAdapter {
                log: Arc::clone(&self.log),
            })]
        }
    }

    impl crate::HttpPlugin for HealthPlugin {
        fn http_modules(&self) -> Vec<HttpModule> {
            vec![define_module("health")
                .get("/healthz", health_handler)
                .build()]
        }
    }

    #[tokio::test]
    async fn http_plugin_mounts_route_adapter_and_contributor() {
        let log: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
        let plugin = HealthPlugin {
            log: Arc::clone(&log),
        };

        let (router, state) = bootstrap().http_plugin(plugin).into_router().unwrap();

        // Adapter shipped by the plugin made it through to AppState.
        let names: Vec<_> = state.adapters.iter().map(|a| a.name().to_owned()).collect();
        assert_eq!(names, vec!["health"]);

        // Route shipped by the plugin is reachable AND the contributor's
        // value flows through Ctx<T>.
        let res = router
            .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 64).await.unwrap();
        assert_eq!(&body[..], b"ok 0.0.0-test");
    }

    #[tokio::test]
    async fn contributor_missing_producer_fails_bootstrap() {
        struct WantsTenant;
        impl kick_rs_core::ContextContributor for WantsTenant {
            type Key = String;
            type Deps = (Tenant,);
            async fn resolve<'a>(
                &'a self,
                _ctx: &'a dyn ContributorRequest,
                (t,): (&'a Tenant,),
            ) -> KickResult<String> {
                Ok(t.slug.clone())
            }
        }

        // Register the consumer but NOT the producer.
        let m = define_module("broken").contribute(WantsTenant).build();
        let err = bootstrap().module(m).into_router().unwrap_err();
        assert_eq!(err.code, "RK_E_MISSING_CONTRIBUTOR");
    }
}

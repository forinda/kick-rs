//! HTTP-aware module wrapper around [`kick_rs_core::Module`].
//!
//! Core's [`kick_rs_core::Module`] holds providers only — it's
//! transport-agnostic on purpose. [`HttpModule`] adds the route storage
//! that's specific to HTTP, keeping core free of axum.
//!
//! Application code should use this crate's [`define_module`] (re-exported
//! by the umbrella `kick-rs`) rather than the core variant.

use axum::handler::Handler;
use axum::Router;
use kick_rs_core::{ContainerBuilder, Module as CoreModule, ModuleBuilder as CoreModuleBuilder};
use std::sync::Arc;

/// Type-erased "mount this route onto this router" closure. FnOnce because
/// each registrar consumes its captured handler exactly once at bootstrap.
type RouteRegistrar = Box<dyn FnOnce(Router) -> Router + Send>;

/// Type-erased OpenAPI path recorder. Each entry, when called, adds
/// one handler's `(path, methods, operation)` to the given utoipa
/// `Paths` map. Monomorphized over the handler's `__path_<name>` type
/// at registration time so no per-call allocation or virtual dispatch.
#[cfg(feature = "openapi")]
pub(crate) type OpenApiPathRecorder = fn(&mut utoipa::openapi::path::Paths);

/// HTTP module — core providers + zero or more routes + sub-modules.
///
/// Created by [`define_module`] and consumed by `bootstrap()`.
pub struct HttpModule {
    pub(crate) core: CoreModule,
    pub(crate) routes: Vec<RouteRegistrar>,
    pub(crate) sub_modules: Vec<HttpModule>,
    /// utoipa path recorders. Populated by
    /// [`HttpModuleBuilder::openapi_path`].
    #[cfg(feature = "openapi")]
    pub(crate) openapi_paths: Vec<OpenApiPathRecorder>,
}

impl HttpModule {
    /// Stable name (forwards to the core module).
    pub fn name(&self) -> &str {
        &self.core.name
    }

    /// URL prefix applied to routes in this module.
    pub fn prefix(&self) -> &str {
        &self.core.prefix
    }

    /// Fold every provider this module (recursively) declares into a
    /// container builder. Sub-modules' providers are folded too.
    pub fn register_into(&self, mut builder: ContainerBuilder) -> ContainerBuilder {
        builder = self.core.register_into(builder);
        for sub in &self.sub_modules {
            builder = sub.register_into(builder);
        }
        builder
    }

    /// Gather every [`ContextContributor`](kick_rs_core::ContextContributor)
    /// this module (and sub-modules) registered. Used by `bootstrap()`
    /// to build the per-app contributor pipeline.
    pub fn collect_contributors(&self) -> Vec<kick_rs_core::AnyContributor> {
        let mut out = self.core.collect_contributors();
        for sub in &self.sub_modules {
            out.extend(sub.collect_contributors());
        }
        out
    }

    /// Consume `self`, mounting every route onto `router`. Sub-modules'
    /// routes are mounted recursively.
    pub fn mount_onto(self, mut router: Router) -> Router {
        for registrar in self.routes {
            router = registrar(router);
        }
        for sub in self.sub_modules {
            router = sub.mount_onto(router);
        }
        router
    }

    /// Total number of routes declared by this module + sub-modules.
    pub fn route_count(&self) -> usize {
        self.routes.len()
            + self
                .sub_modules
                .iter()
                .map(|s| s.route_count())
                .sum::<usize>()
    }

    /// Apply every registered utoipa path recorder (recursively
    /// including sub-modules) to the given `Paths` map. Used by
    /// [`crate::openapi::OpenApiPlugin::from_modules`].
    #[cfg(feature = "openapi")]
    pub fn record_openapi_paths(&self, paths: &mut utoipa::openapi::path::Paths) {
        for recorder in &self.openapi_paths {
            recorder(paths);
        }
        for sub in &self.sub_modules {
            sub.record_openapi_paths(paths);
        }
    }
}

impl std::fmt::Debug for HttpModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpModule")
            .field("name", &self.core.name)
            .field("prefix", &self.core.prefix)
            .field("routes", &self.routes.len())
            .field("sub_modules", &self.sub_modules.len())
            .finish()
    }
}

// ───────────────────────────────── Builder ─────────────────────────────────

/// Fluent builder for [`HttpModule`].
pub struct HttpModuleBuilder {
    core: CoreModuleBuilder,
    routes: Vec<RouteRegistrar>,
    sub_modules: Vec<HttpModule>,
    /// Mirrored on the side so route handlers can prepend the prefix
    /// without round-tripping through `core_builder`.
    prefix: String,
    #[cfg(feature = "openapi")]
    openapi_paths: Vec<OpenApiPathRecorder>,
}

/// Begin composing an HTTP module.
pub fn define_module(name: impl Into<String>) -> HttpModuleBuilder {
    HttpModuleBuilder {
        core: kick_rs_core::define_module(name),
        routes: Vec::new(),
        sub_modules: Vec::new(),
        prefix: String::new(),
        #[cfg(feature = "openapi")]
        openapi_paths: Vec::new(),
    }
}

impl HttpModuleBuilder {
    /// URL prefix applied to every route declared on this module.
    pub fn prefix(mut self, p: impl Into<String>) -> Self {
        let p = p.into();
        self.prefix = p.clone();
        self.core = self.core.prefix(p);
        self
    }

    /// Bind a pre-built singleton service.
    pub fn service_value<T: 'static + Send + Sync>(mut self, value: T) -> Self {
        self.core = self.core.service_value(value);
        self
    }

    /// Bind a type that implements [`ServiceImpl`](kick_rs_core::ServiceImpl).
    /// Forwarded straight to [`ModuleBuilder::service`](kick_rs_core::ModuleBuilder::service).
    pub fn service<T: kick_rs_core::ServiceImpl>(mut self) -> Self {
        self.core = self.core.service::<T>();
        self
    }

    /// Register a [`ContextContributor`](kick_rs_core::ContextContributor).
    /// Forwarded straight to [`ModuleBuilder::contribute`](kick_rs_core::ModuleBuilder::contribute).
    /// The contributor participates in the app-wide pipeline topo-sort
    /// at boot.
    pub fn contribute<C: kick_rs_core::ContextContributor>(mut self, c: C) -> Self {
        self.core = self.core.contribute(c);
        self
    }

    /// Bind a singleton constructed lazily via a closure.
    pub fn service_factory<T, F>(mut self, factory: F) -> Self
    where
        T: 'static + Send + Sync,
        F: Fn(&kick_rs_core::Container) -> Arc<T> + Send + Sync + 'static,
    {
        self.core = self.core.service_factory(factory);
        self
    }

    /// Bind a transient — runs per resolve.
    pub fn transient<T, F>(mut self, factory: F) -> Self
    where
        T: 'static + Send + Sync,
        F: Fn(&kick_rs_core::Container) -> T + Send + Sync + 'static,
    {
        self.core = self.core.transient(factory);
        self
    }

    /// Mount a sub-module under this one. Sub-module routes are prepended
    /// with **the sub-module's own prefix only** — the parent prefix is not
    /// automatically applied (this matches axum's `nest` semantics).
    pub fn sub_module(mut self, m: HttpModule) -> Self {
        self.sub_modules.push(m);
        self
    }

    /// Register a `GET` route.
    pub fn get<H, T>(self, path: &str, handler: H) -> Self
    where
        H: Handler<T, ()>,
        T: 'static,
    {
        self.route(axum::routing::get(handler), path)
    }

    /// Register a `POST` route.
    pub fn post<H, T>(self, path: &str, handler: H) -> Self
    where
        H: Handler<T, ()>,
        T: 'static,
    {
        self.route(axum::routing::post(handler), path)
    }

    /// Register a `PUT` route.
    pub fn put<H, T>(self, path: &str, handler: H) -> Self
    where
        H: Handler<T, ()>,
        T: 'static,
    {
        self.route(axum::routing::put(handler), path)
    }

    /// Register a `PATCH` route.
    pub fn patch<H, T>(self, path: &str, handler: H) -> Self
    where
        H: Handler<T, ()>,
        T: 'static,
    {
        self.route(axum::routing::patch(handler), path)
    }

    /// Register a `DELETE` route.
    pub fn delete<H, T>(self, path: &str, handler: H) -> Self
    where
        H: Handler<T, ()>,
        T: 'static,
    {
        self.route(axum::routing::delete(handler), path)
    }

    /// Attach a route registered by the `#[get]`/`#[post]`/etc.
    /// attribute macros from `kick-rs-macros`. Each macro generates a
    /// `pub fn <handler>_route(Router) -> Router` next to the handler;
    /// pass that fn here to mount it on the module.
    ///
    /// ```ignore
    /// #[get("/users/:id")]
    /// async fn show(svc: Inject<UserService>) -> Json<User> { … }
    ///
    /// define_module("users")
    ///     .handler(show_route)
    ///     .build()
    /// ```
    ///
    /// The explicit `.get(path, fn)` / `.post(path, fn)` style still
    /// works — `.handler(...)` is the macro-driven complement, not a
    /// replacement.
    pub fn handler<F>(mut self, registrar: F) -> Self
    where
        F: FnOnce(Router) -> Router + Send + 'static,
    {
        self.routes.push(Box::new(registrar));
        self
    }

    /// Attach multiple `_route` registrars in one chain. Convenience
    /// over calling `.handler(...)` per item.
    ///
    /// ```ignore
    /// define_module("users").handlers([show_route, list_route, create_route]).build()
    /// ```
    pub fn handlers<I, F>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = F>,
        F: FnOnce(Router) -> Router + Send + 'static,
    {
        for registrar in iter {
            self.routes.push(Box::new(registrar));
        }
        self
    }

    fn route(mut self, method_router: axum::routing::MethodRouter, path: &str) -> Self {
        let full_path = if self.prefix.is_empty() {
            path.to_owned()
        } else {
            format!("{}{}", self.prefix, path)
        };
        let registrar: RouteRegistrar =
            Box::new(move |r: Router| r.route(&full_path, method_router));
        self.routes.push(registrar);
        self
    }

    /// Register a utoipa-annotated handler's OpenAPI path metadata on
    /// this module. The type parameter is the `__path_<fn_name>` type
    /// generated by `#[utoipa::path(...)]` next to the handler:
    ///
    /// ```ignore
    /// #[utoipa::path(get, path = "/users/{id}", responses(...))]
    /// async fn get_user(/* ... */) { /* ... */ }
    ///
    /// define_module("users")
    ///     .get("/users/:id", get_user)
    ///     .openapi_path::<__path_get_user>()
    ///     .build()
    /// ```
    ///
    /// Then `OpenApiPlugin::from_modules(info, [&users])` walks the
    /// registered paths and produces the spec automatically — no
    /// parallel `#[derive(OpenApi)]` enumeration needed.
    #[cfg(feature = "openapi")]
    pub fn openapi_path<T: utoipa::Path + 'static>(mut self) -> Self {
        // Monomorphized over T so each push is a normal fn pointer
        // with no captures.
        fn record<T: utoipa::Path>(paths: &mut utoipa::openapi::path::Paths) {
            paths.add_path_operation(T::path(), T::methods(), T::operation());
        }
        self.openapi_paths.push(record::<T>);
        self
    }

    /// Finalize.
    pub fn build(self) -> HttpModule {
        HttpModule {
            core: self.core.build(),
            routes: self.routes,
            sub_modules: self.sub_modules,
            #[cfg(feature = "openapi")]
            openapi_paths: self.openapi_paths,
        }
    }
}

// ──────────────────────────────── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Inject;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Extension;
    use kick_rs_core::Container;
    use tower::ServiceExt;

    #[derive(Debug)]
    struct Greeter(String);

    async fn root() -> &'static str {
        "hi"
    }

    async fn greet(svc: Inject<Greeter>) -> String {
        // Inject<Greeter> derefs to Greeter via Deref; `(*svc).0` reaches
        // the inner String field. Direct `svc.0.0.clone()` would parse
        // `0.0` as a float literal.
        (*svc).0.clone()
    }

    #[tokio::test]
    async fn basic_get_route_responds() {
        let m = define_module("test").get("/ping", root).build();
        let router = m
            .mount_onto(Router::new())
            .layer(Extension(Container::builder().build().unwrap()));

        let res = router
            .oneshot(Request::get("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"hi");
    }

    #[tokio::test]
    async fn prefix_is_applied_to_routes() {
        let m = define_module("test")
            .prefix("/api/v1")
            .get("/ping", root)
            .build();
        let router = m
            .mount_onto(Router::new())
            .layer(Extension(Container::builder().build().unwrap()));

        let res = router
            .clone()
            .oneshot(Request::get("/api/v1/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let unmatched = router
            .oneshot(Request::get("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(unmatched.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn inject_resolves_from_container() {
        let m = define_module("test")
            .service_value(Greeter("hello there".into()))
            .get("/g", greet)
            .build();

        // register_into borrows; mount_onto consumes — so build the container
        // first, then mount routes.
        let container = m.register_into(Container::builder()).build().unwrap();
        let router = m.mount_onto(Router::new()).layer(Extension(container));

        let res = router
            .oneshot(Request::get("/g").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"hello there");
    }

    #[tokio::test]
    async fn missing_provider_returns_problem_json() {
        // Handler asks for Greeter, but the module never registers one.
        let m = define_module("test").get("/g", greet).build();
        let router = m
            .mount_onto(Router::new())
            .layer(Extension(Container::builder().build().unwrap()));

        let res = router
            .oneshot(Request::get("/g").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
        let body_str = String::from_utf8_lossy(&body);
        assert!(
            body_str.contains("RK_E_UNKNOWN_TOKEN"),
            "body was: {body_str}"
        );
    }

    #[tokio::test]
    async fn sub_module_routes_are_mounted() {
        async fn inner_handler() -> &'static str {
            "from-inner"
        }
        let inner = define_module("inner")
            .prefix("/inner")
            .get("/ping", inner_handler)
            .build();
        let outer = define_module("outer").sub_module(inner).build();

        assert_eq!(outer.route_count(), 1);

        let router = outer
            .mount_onto(Router::new())
            .layer(Extension(Container::builder().build().unwrap()));
        let res = router
            .oneshot(Request::get("/inner/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"from-inner");
    }
}

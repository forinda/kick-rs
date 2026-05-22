//! `AssetsPlugin` — serve a compile-time-embedded asset tree.
//!
//! Pairs with [`kick_rs_assets::AssetManifest`] +
//! [`kick_rs_assets::EmbeddedAssets`]. The manifest supplies the
//! URL prefix and the logical-→-hashed mapping; the embedded tree
//! supplies the actual bytes.
//!
//! ```ignore
//! use kick_rs_http::plugins::assets::AssetsPlugin;
//! use kick_rs_assets::{embed_assets, AssetManifest, EmbeddedAssets};
//!
//! static ASSETS: EmbeddedAssets = embed_assets!("$CARGO_MANIFEST_DIR/dist");
//!
//! let manifest = AssetManifest::load("dist/manifest.json")?
//!     .with_url_prefix("/static");
//!
//! bootstrap()
//!     .http_plugin(AssetsPlugin::new(manifest, &ASSETS))
//!     .module(users::define())
//!     .listen(addr).await
//! ```
//!
//! `AssetsPlugin` registers the `AssetManifest` as a DI singleton so
//! handlers / templates can `Inject<AssetManifest>` to resolve URLs:
//! `m.resolve("app.js")` → `"/static/app.a1b2c3.js"`.
//!
//! Cache headers: every served asset gets
//! `cache-control: public, immutable, max-age=31536000` because the
//! filename itself carries the content hash — old URLs never change.

use crate::define_module;
use crate::module::HttpModule;
use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use kick_rs_assets::{content_type_for, AssetManifest, EmbeddedAssets};
use kick_rs_core::{KickResult, Plugin};
use std::sync::Arc;

/// Cache-Control value for hash-named static assets. One year, public,
/// `immutable` so browsers don't bother to revalidate.
const IMMUTABLE_CACHE: &str = "public, immutable, max-age=31536000";

/// Asset-serving plugin. Cloneable so the builder can pass it through
/// the lifecycle without an `Arc<Self>` indirection.
#[derive(Clone)]
pub struct AssetsPlugin {
    manifest: Arc<AssetManifest>,
    embedded: &'static EmbeddedAssets,
}

impl AssetsPlugin {
    /// Build the plugin. `manifest` is wrapped in an `Arc` internally
    /// — the DI binding shares the same instance with handlers via
    /// `Inject<AssetManifest>`.
    pub fn new(manifest: AssetManifest, embedded: &'static EmbeddedAssets) -> Self {
        Self {
            manifest: Arc::new(manifest),
            embedded,
        }
    }

    /// Reference to the wrapped manifest (handy for tests + debug
    /// inspection).
    pub fn manifest(&self) -> &AssetManifest {
        &self.manifest
    }
}

impl std::fmt::Debug for AssetsPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetsPlugin")
            .field("prefix", &self.manifest.url_prefix())
            .field("entries", &self.manifest.len())
            .finish()
    }
}

impl Plugin for AssetsPlugin {
    fn name(&self) -> &str {
        "assets"
    }

    fn register(&self, b: &mut kick_rs_core::ContainerBuilder) -> KickResult<()> {
        // Bind the *manifest* (not the whole plugin) as a singleton so
        // adopters can `Inject<AssetManifest>` to resolve URLs in
        // templates / handlers. The Arc::clone is cheap; the inner
        // BTreeMap is shared.
        //
        // ContainerBuilder's `singleton_arc` consumes self by value —
        // standard for fluent builders. The trait gives us `&mut`, so
        // we swap in a Default placeholder, mutate the owned value,
        // and swap back.
        let taken = std::mem::take(b);
        *b = taken.singleton_arc(self.manifest.clone());
        Ok(())
    }
}

impl crate::HttpPlugin for AssetsPlugin {
    fn http_modules(&self) -> Vec<HttpModule> {
        // Manifest's url_prefix is the segment we own. Axum's
        // wildcard syntax in 0.7 is `*name` to capture the rest of
        // the path (one *capture name* per segment is not enough —
        // we want everything after the prefix).
        let prefix = self.manifest.url_prefix();
        // An empty prefix means "serve from the root" — rare but
        // valid. Build the route accordingly.
        let route = if prefix.is_empty() {
            "/*path".to_owned()
        } else {
            format!("{prefix}/*path")
        };

        let embedded = self.embedded;
        let module = define_module("assets")
            .get(&route, move |Path(path): Path<String>| {
                let embedded = embedded;
                async move { serve(embedded, &path) }
            })
            .build();
        vec![module]
    }
}

fn serve(embedded: &'static EmbeddedAssets, rel: &str) -> Response {
    let Some(file) = embedded.get_file(rel) else {
        return (StatusCode::NOT_FOUND, "asset not found").into_response();
    };
    let mime = content_type_for(rel);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, IMMUTABLE_CACHE)
        .body(Body::from(file.contents()))
        .expect("response builder with statically-typed headers can't fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap;
    use axum::http::Request;
    use kick_rs_assets::embed_assets;
    use tower::ServiceExt;

    // Embed a directory we know exists in the workspace so we have
    // real bytes to serve. The crate's own `src/plugins/` folder has
    // a stable set of files. We don't care WHICH files — we just need
    // an embedded tree we can probe.
    static FIXTURE: EmbeddedAssets = embed_assets!("$CARGO_MANIFEST_DIR/src/plugins");

    fn build_app() -> axum::Router {
        let manifest = AssetManifest::from_json(
            r#"{ "compression.rs": "compression.rs", "absent.rs": "still-absent.rs" }"#,
        )
        .unwrap()
        .with_url_prefix("/static");

        let (router, _) = bootstrap()
            .http_plugin(AssetsPlugin::new(manifest, &FIXTURE))
            .into_router()
            .unwrap();
        router
    }

    #[tokio::test]
    async fn serves_embedded_file_with_cache_headers() {
        let res = build_app()
            .oneshot(
                Request::get("/static/compression.rs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // `.rs` isn't in the MIME table → falls back to octet-stream.
        // What we really care about is that the cache header is set
        // and the body is non-empty.
        assert!(!ct.is_empty(), "missing content-type");
        let cache = res
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(cache.contains("immutable"), "got cache-control: {cache:?}");
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        assert!(!bytes.is_empty(), "body should carry the file contents");
    }

    #[tokio::test]
    async fn missing_path_returns_404() {
        let res = build_app()
            .oneshot(
                Request::get("/static/does-not-exist.rs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn off_prefix_does_not_match() {
        let res = build_app()
            .oneshot(
                Request::get("/elsewhere/compression.rs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // axum returns 404 for unmatched routes.
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn plugin_debug_format_includes_summary() {
        let manifest = AssetManifest::from_json(r#"{ "a": "b" }"#)
            .unwrap()
            .with_url_prefix("/static");
        let p = AssetsPlugin::new(manifest, &FIXTURE);
        let s = format!("{p:?}");
        assert!(s.contains("/static"), "got: {s}");
        assert!(s.contains("entries: 1"), "got: {s}");
    }
}

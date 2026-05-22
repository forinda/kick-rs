//! Built-in HTTP plugins — see [`SPEC.md` §5.7](../SPEC.md#57-built-in-plugins-shipped-with-kick-rs).
//!
//! Each lives behind a small module so end-users can pick what they want.
//! Concrete implementations land in Phase 5.

/// `X-Request-Id` propagation + binding of `RequestId` singleton.
pub mod request_id {
    /// Placeholder for the request-id plugin.
    pub fn request_id() {
        // todo: wire up real tower layer
    }
}

/// Pino-style structured request logging via `tracing`.
pub mod request_logger {
    /// Placeholder for the request-logger plugin.
    pub fn request_logger() {}
}

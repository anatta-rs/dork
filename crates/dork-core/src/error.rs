//! Error types — backend-agnostic.

use thiserror::Error;

/// Errors that can occur during a search.
#[derive(Debug, Error)]
pub enum DorkError {
    /// The query is malformed or empty.
    #[error("invalid query: {0}")]
    InvalidQuery(String),

    /// The backend exceeded its rate limit. Callers should back off.
    #[error("rate limited (retry after {retry_after_seconds}s)")]
    RateLimited {
        /// Suggested wait, in seconds.
        retry_after_seconds: u64,
    },

    /// The backend's authentication failed (missing or invalid key).
    #[error("authentication failed: {0}")]
    Auth(String),

    /// Network-level failure (DNS, connect, TLS).
    #[error("network error: {0}")]
    Network(String),

    /// The backend's response could not be parsed (HTML drift, JSON error, …).
    #[error("parse error: {0}")]
    Parse(String),

    /// Catch-all for backend-specific failures.
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, DorkError>;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn invalid_query_displays() {
        let e = DorkError::InvalidQuery("empty".into());
        assert_eq!(e.to_string(), "invalid query: empty");
    }

    #[test]
    fn rate_limited_carries_retry_after() {
        let e = DorkError::RateLimited {
            retry_after_seconds: 30,
        };
        let s = e.to_string();
        assert!(s.contains("30"));
    }

    #[test]
    fn auth_displays() {
        let e = DorkError::Auth("missing API key".into());
        assert!(e.to_string().contains("missing API key"));
    }

    #[test]
    fn backend_wraps_inner() {
        let inner = std::io::Error::other("oops");
        let e = DorkError::Backend(Box::new(inner));
        assert!(e.to_string().starts_with("backend error"));
        assert!(std::error::Error::source(&e).is_some());
    }

    #[test]
    fn debug_renders() {
        let e = DorkError::InvalidQuery("x".into());
        assert!(format!("{e:?}").contains("InvalidQuery"));
    }

    #[test]
    fn result_alias_works() {
        fn maybe(ok: bool) -> Result<i32> {
            if ok {
                Ok(42)
            } else {
                Err(DorkError::InvalidQuery("nope".into()))
            }
        }
        assert_eq!(maybe(true).expect("ok"), 42);
        assert!(maybe(false).is_err());
    }
}

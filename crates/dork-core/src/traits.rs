//! `SearchEngine` — the trait every backend implements.

use crate::error::Result;
use crate::query::Query;
use crate::result::SearchHit;
use async_trait::async_trait;

/// A backend that can execute a [`Query`] and return ranked [`SearchHit`]s.
///
/// Implementations are stateless from the caller's perspective: any
/// per-request state (rate-limit counters, auth tokens) lives inside the
/// engine instance.
///
/// `Send + Sync` so engines can be shared across async tasks.
#[async_trait]
pub trait SearchEngine: Send + Sync {
    /// Backend identifier (`"duckduckgo"`, `"google"`, …) — used by
    /// consumers for logging and to tag [`SearchHit::source`].
    fn name(&self) -> &'static str;

    /// Execute `query` and return the ranked hits, capped at `query.limit`.
    ///
    /// # Errors
    ///
    /// See [`crate::DorkError`] for the failure modes. Empty result sets
    /// are NOT errors — return `Ok(vec![])`.
    async fn search(&self, query: &Query) -> Result<Vec<SearchHit>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DorkError;

    struct FakeEngine {
        name: &'static str,
    }

    #[async_trait]
    impl SearchEngine for FakeEngine {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn search(&self, q: &Query) -> Result<Vec<SearchHit>> {
            if !q.is_valid() {
                return Err(DorkError::InvalidQuery("empty".into()));
            }
            Ok(vec![
                SearchHit::new(1, "T", "https://x.test", "s").with_source(self.name),
            ])
        }
    }

    #[tokio::test]
    async fn fake_engine_returns_one_hit_for_valid_query() {
        let e = FakeEngine { name: "fake" };
        let hits = e.search(&Query::new("rust")).await.expect("ok");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source.as_deref(), Some("fake"));
    }

    #[tokio::test]
    async fn fake_engine_rejects_empty_query() {
        let e = FakeEngine { name: "fake" };
        let err = e.search(&Query::new("")).await.expect_err("must fail");
        assert!(matches!(err, DorkError::InvalidQuery(_)));
    }

    #[tokio::test]
    async fn name_is_stable() {
        let e = FakeEngine { name: "fake" };
        assert_eq!(e.name(), "fake");
    }
}

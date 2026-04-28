//! Core types: `SearchEngine` trait, `Query`, and `SearchHit`.

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

// ===== SearchEngine trait =====

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

// ===== Query =====

/// `SafeSearch` / content-filter level. Backends map this to whatever knob
/// they expose; not all of them honour every level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SafeSearch {
    /// Strictly filtered.
    Strict,
    /// Backend default (typically moderate).
    #[default]
    Moderate,
    /// Filtering off.
    Off,
}

/// A single search request.
///
/// Built up via the chainable setters; backends may interpret optional
/// fields differently (or ignore them) depending on what their API
/// surface allows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    /// The free-text query. Required.
    pub q: String,
    /// Maximum hits the backend should return.
    pub limit: usize,
    /// Optional region/locale hint (e.g. `"us-en"`, `"fr-fr"`).
    pub region: Option<String>,
    /// `SafeSearch` level.
    pub safe_search: SafeSearch,
    /// Optional `site:` restriction (e.g. `"github.com"`).
    pub site: Option<String>,
    /// Optional `filetype:` restriction (e.g. `"pdf"`).
    pub filetype: Option<String>,
}

impl Query {
    /// Build a query from a free-text string with sensible defaults.
    #[must_use]
    pub fn new(q: impl Into<String>) -> Self {
        Self {
            q: q.into(),
            limit: 10,
            region: None,
            safe_search: SafeSearch::default(),
            site: None,
            filetype: None,
        }
    }

    /// Builder: cap result count.
    #[must_use]
    pub fn with_limit(mut self, n: usize) -> Self {
        self.limit = n.max(1);
        self
    }

    /// Builder: region/locale hint.
    #[must_use]
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Builder: `SafeSearch` level.
    #[must_use]
    pub fn with_safe_search(mut self, level: SafeSearch) -> Self {
        self.safe_search = level;
        self
    }

    /// Builder: restrict to a single site.
    #[must_use]
    pub fn with_site(mut self, site: impl Into<String>) -> Self {
        self.site = Some(site.into());
        self
    }

    /// Builder: filter by file type.
    #[must_use]
    pub fn with_filetype(mut self, filetype: impl Into<String>) -> Self {
        self.filetype = Some(filetype.into());
        self
    }

    /// Render the query as a string the backend can hand to its API,
    /// folding `site:` and `filetype:` operators into the free-text part.
    /// Backends that have native parameters can ignore this and read the
    /// fields directly.
    #[must_use]
    pub fn render_with_operators(&self) -> String {
        let mut out = self.q.trim().to_string();
        if let Some(site) = &self.site {
            let _ = write!(out, " site:{site}");
        }
        if let Some(ft) = &self.filetype {
            let _ = write!(out, " filetype:{ft}");
        }
        out
    }

    /// True iff the query has actual searchable content.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.q.trim().is_empty() && self.limit >= 1
    }
}

// ===== SearchHit =====

/// One ranked search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// 1-based rank within the response (1 = top result).
    pub rank: usize,
    /// Page title.
    pub title: String,
    /// Canonical URL.
    pub url: String,
    /// Engine-provided snippet (typically a one-paragraph summary).
    pub snippet: String,
    /// Optional source identifier (engine name / region / domain).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl SearchHit {
    /// Construct a new hit with the required fields.
    #[must_use]
    pub fn new(
        rank: usize,
        title: impl Into<String>,
        url: impl Into<String>,
        snippet: impl Into<String>,
    ) -> Self {
        Self {
            rank,
            title: title.into(),
            url: url.into(),
            snippet: snippet.into(),
            source: None,
        }
    }

    /// Builder-style: tag the hit with a source identifier.
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ===== Query tests =====

    #[test]
    fn query_new_uses_sensible_defaults() {
        let q = Query::new("rust");
        assert_eq!(q.q, "rust");
        assert_eq!(q.limit, 10);
        assert_eq!(q.safe_search, SafeSearch::Moderate);
        assert!(q.region.is_none());
        assert!(q.site.is_none());
        assert!(q.filetype.is_none());
    }

    #[test]
    fn query_builders_chain() {
        let q = Query::new("rust async")
            .with_limit(5)
            .with_region("us-en")
            .with_safe_search(SafeSearch::Off)
            .with_site("docs.rs")
            .with_filetype("html");
        assert_eq!(q.limit, 5);
        assert_eq!(q.region.as_deref(), Some("us-en"));
        assert_eq!(q.safe_search, SafeSearch::Off);
        assert_eq!(q.site.as_deref(), Some("docs.rs"));
        assert_eq!(q.filetype.as_deref(), Some("html"));
    }

    #[test]
    fn query_with_limit_clamps_zero_to_one() {
        let q = Query::new("x").with_limit(0);
        assert_eq!(q.limit, 1);
    }

    #[test]
    fn query_render_with_operators_folds_site_and_filetype() {
        let q = Query::new("rust")
            .with_site("github.com")
            .with_filetype("rs");
        assert_eq!(
            q.render_with_operators(),
            "rust site:github.com filetype:rs"
        );
    }

    #[test]
    fn query_render_omits_unset_operators() {
        let q = Query::new("rust");
        assert_eq!(q.render_with_operators(), "rust");
    }

    #[test]
    fn query_is_valid_false_for_empty_query() {
        assert!(!Query::new("").is_valid());
        assert!(!Query::new("   ").is_valid());
        assert!(Query::new("rust").is_valid());
    }

    #[test]
    fn safe_search_default_is_moderate() {
        assert_eq!(SafeSearch::default(), SafeSearch::Moderate);
    }

    #[test]
    fn query_serde_roundtrip() {
        let q = Query::new("rust").with_limit(5).with_site("docs.rs");
        let json = serde_json::to_string(&q).expect("serialize");
        let back: Query = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(q, back);
    }

    // ===== SearchHit tests =====

    #[test]
    fn hit_new_constructs_with_no_source() {
        let h = SearchHit::new(1, "Title", "https://x.test", "snippet");
        assert_eq!(h.rank, 1);
        assert_eq!(h.title, "Title");
        assert_eq!(h.url, "https://x.test");
        assert_eq!(h.snippet, "snippet");
        assert!(h.source.is_none());
    }

    #[test]
    fn hit_with_source_attaches() {
        let h = SearchHit::new(1, "T", "u", "s").with_source("ddg");
        assert_eq!(h.source.as_deref(), Some("ddg"));
    }

    #[test]
    fn hit_serde_roundtrip() {
        let h = SearchHit::new(1, "T", "u", "s").with_source("ddg");
        let json = serde_json::to_string(&h).expect("serialize");
        let back: SearchHit = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(h, back);
    }

    #[test]
    fn hit_serde_omits_empty_source() {
        let h = SearchHit::new(1, "T", "u", "s");
        let json = serde_json::to_string(&h).expect("serialize");
        assert!(
            !json.contains("source"),
            "empty source should be omitted: {json}"
        );
    }

    // ===== SearchEngine trait tests =====

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
                use crate::DorkError;
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
        use crate::DorkError;
        let e = FakeEngine { name: "fake" };
        let err = e.search(&Query::new("")).await.expect_err("must fail");
        assert!(matches!(err, DorkError::InvalidQuery(_)));
    }

    #[tokio::test]
    async fn fake_engine_name_is_stable() {
        let e = FakeEngine { name: "fake" };
        assert_eq!(e.name(), "fake");
    }
}

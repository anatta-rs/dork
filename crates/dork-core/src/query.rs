//! `Query` — what the consumer asks of a [`crate::SearchEngine`].

use serde::{Deserialize, Serialize};
use std::fmt::Write;

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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn new_uses_sensible_defaults() {
        let q = Query::new("rust");
        assert_eq!(q.q, "rust");
        assert_eq!(q.limit, 10);
        assert_eq!(q.safe_search, SafeSearch::Moderate);
        assert!(q.region.is_none());
        assert!(q.site.is_none());
        assert!(q.filetype.is_none());
    }

    #[test]
    fn builders_chain() {
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
    fn with_limit_clamps_zero_to_one() {
        let q = Query::new("x").with_limit(0);
        assert_eq!(q.limit, 1);
    }

    #[test]
    fn render_with_operators_folds_site_and_filetype() {
        let q = Query::new("rust")
            .with_site("github.com")
            .with_filetype("rs");
        assert_eq!(
            q.render_with_operators(),
            "rust site:github.com filetype:rs"
        );
    }

    #[test]
    fn render_omits_unset_operators() {
        let q = Query::new("rust");
        assert_eq!(q.render_with_operators(), "rust");
    }

    #[test]
    fn is_valid_false_for_empty_query() {
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
}

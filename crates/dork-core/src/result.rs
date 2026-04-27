//! `SearchHit` — a single ranked result.

use serde::{Deserialize, Serialize};

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

    #[test]
    fn new_constructs_with_no_source() {
        let h = SearchHit::new(1, "Title", "https://x.test", "snippet");
        assert_eq!(h.rank, 1);
        assert_eq!(h.title, "Title");
        assert_eq!(h.url, "https://x.test");
        assert_eq!(h.snippet, "snippet");
        assert!(h.source.is_none());
    }

    #[test]
    fn with_source_attaches() {
        let h = SearchHit::new(1, "T", "u", "s").with_source("ddg");
        assert_eq!(h.source.as_deref(), Some("ddg"));
    }

    #[test]
    fn serde_roundtrip() {
        let h = SearchHit::new(1, "T", "u", "s").with_source("ddg");
        let json = serde_json::to_string(&h).expect("serialize");
        let back: SearchHit = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(h, back);
    }

    #[test]
    fn serde_omits_empty_source() {
        let h = SearchHit::new(1, "T", "u", "s");
        let json = serde_json::to_string(&h).expect("serialize");
        assert!(
            !json.contains("source"),
            "empty source should be omitted: {json}"
        );
    }
}

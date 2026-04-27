//! HTML parsing for the DuckDuckGo HTML lite result page.
//!
//! Layout (stable as of 2026-Q2): each result is a `<div class="result">`
//! holding:
//!
//! - `a.result__a`            — title + canonical-ish href (DDG redirect URL)
//! - `a.result__url`          — visible display URL
//! - `a.result__snippet`      — snippet text
//!
//! The `href` on `result__a` looks like `//duckduckgo.com/l/?uddg=<encoded>&...`.
//! We unwrap that to get the real target.
#![allow(clippy::doc_markdown)] // brand "DuckDuckGo" appears throughout

use dork_core::{DorkError, Result, SearchHit};
use scraper::{ElementRef, Html, Selector};

pub fn parse_results(html: &str) -> Result<Vec<SearchHit>> {
    let doc = Html::parse_document(html);

    let result_sel = Selector::parse("div.result, div.web-result").map_err(selector_err)?;
    let title_sel = Selector::parse("a.result__a").map_err(selector_err)?;
    let snippet_sel =
        Selector::parse("a.result__snippet, .result__snippet").map_err(selector_err)?;
    let url_sel = Selector::parse("a.result__url, .result__url").map_err(selector_err)?;

    let mut hits = Vec::new();
    let mut rank = 1_usize;

    for block in doc.select(&result_sel) {
        let Some(title_link) = block.select(&title_sel).next() else {
            continue;
        };

        let title = collect_text(&title_link);
        if title.trim().is_empty() {
            continue;
        }

        let raw_href = title_link.value().attr("href").unwrap_or_default();
        let url = if let Some(visible) = block.select(&url_sel).next() {
            let visible_text = collect_text(&visible).trim().to_string();
            if visible_text.is_empty() {
                resolve_href(raw_href)
            } else {
                normalize_visible_url(&visible_text)
            }
        } else {
            resolve_href(raw_href)
        };

        let snippet = block
            .select(&snippet_sel)
            .next()
            .map(|el| collect_text(&el).trim().to_string())
            .unwrap_or_default();

        hits.push(SearchHit::new(rank, title.trim(), url, snippet));
        rank += 1;
    }

    Ok(hits)
}

fn selector_err(_e: impl std::fmt::Debug) -> DorkError {
    DorkError::Parse("invalid CSS selector (this is a dork-duckduckgo bug)".into())
}

fn collect_text(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join("")
}

/// DDG wraps targets behind `https://duckduckgo.com/l/?uddg=<percent-encoded>&...`.
/// Unwrap when we see that shape, else return the href as-is.
fn resolve_href(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let with_scheme = if let Some(rest) = trimmed.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        trimmed.to_string()
    };

    if let Ok(url) = url::Url::parse(&with_scheme)
        && url.host_str() == Some("duckduckgo.com")
        && url.path() == "/l/"
    {
        for (k, v) in url.query_pairs() {
            if k == "uddg" {
                return v.into_owned();
            }
        }
    }
    with_scheme
}

/// The visible display URL is shown without scheme — `example.com/foo`.
/// Promote to `https://` so callers can fetch it directly.
fn normalize_visible_url(s: &str) -> String {
    let s = s.trim();
    if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else {
        format!("https://{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn resolve_href_unwraps_uddg() {
        let raw = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Ffoo&rut=abc";
        assert_eq!(resolve_href(raw), "https://example.com/foo");
    }

    #[test]
    fn resolve_href_passthrough_for_direct_url() {
        assert_eq!(resolve_href("https://docs.rs/x"), "https://docs.rs/x");
    }

    #[test]
    fn resolve_href_handles_empty() {
        assert_eq!(resolve_href(""), "");
        assert_eq!(resolve_href("   "), "");
    }

    #[test]
    fn normalize_visible_url_adds_https() {
        assert_eq!(
            normalize_visible_url("example.com/foo"),
            "https://example.com/foo"
        );
        assert_eq!(
            normalize_visible_url("https://example.com"),
            "https://example.com"
        );
    }

    #[test]
    fn parse_extracts_hits_from_fixture() {
        let html = include_str!("../tests/fixtures/ddg_results.html");
        let hits = parse_results(html).expect("parse ok");
        assert!(hits.len() >= 2, "fixture has multiple results: {hits:?}");
        for (i, h) in hits.iter().enumerate() {
            assert_eq!(h.rank, i + 1);
            assert!(!h.title.is_empty());
            assert!(h.url.starts_with("http"));
        }
    }

    #[test]
    fn parse_empty_html_returns_empty_vec() {
        let hits = parse_results("<html><body></body></html>").expect("parse ok");
        assert!(hits.is_empty());
    }

    #[test]
    fn parse_skips_blocks_without_title() {
        let html = r#"
            <html><body>
              <div class="result"></div>
              <div class="result">
                <a class="result__a" href="https://x.test/a">Real</a>
                <a class="result__url">x.test/a</a>
                <a class="result__snippet">s</a>
              </div>
            </body></html>
        "#;
        let hits = parse_results(html).expect("parse ok");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Real");
    }
}

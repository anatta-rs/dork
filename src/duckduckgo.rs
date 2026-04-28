//! `duckduckgo` — `SearchEngine` backend that scrapes the DuckDuckGo
//! HTML lite endpoint at <https://html.duckduckgo.com/html/>.
//!
//! No API key required. Be respectful: rate-limit your callers and identify
//! your client via [`DuckDuckGoEngineBuilder::user_agent`] — the default is
//! generic enough that DDG won't block it on low traffic, but you should
//! always send something honest.
//!
//! ## Example
//!
//! ```no_run
//! use dork::{Query, SearchEngine};
//! use dork::duckduckgo::DuckDuckGoEngine;
//!
//! # async fn run() -> dork::Result<()> {
//! let engine = DuckDuckGoEngine::new();
//! let hits = engine.search(&Query::new("rust async patterns").with_limit(5)).await?;
//! for hit in hits {
//!     println!("{rank}. {title} — {url}", rank = hit.rank, title = hit.title, url = hit.url);
//! }
//! # Ok(()) }
//! ```

#![warn(missing_docs)]
#![deny(unsafe_code)]
#![allow(clippy::doc_markdown)] // brand "DuckDuckGo" appears throughout

use crate::{DorkError, Query, Result, SafeSearch, SearchEngine, SearchHit};
use async_trait::async_trait;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, USER_AGENT};
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};

const ENGINE_NAME: &str = "duckduckgo";
const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; dork/0.1; +https://github.com/anatta-rs/dork)";
const DEFAULT_ENDPOINT: &str = "https://html.duckduckgo.com/html/";

/// Backend that hits the DuckDuckGo HTML endpoint and parses the result page.
#[derive(Debug, Clone)]
pub struct DuckDuckGoEngine {
    client: Client,
    endpoint: String,
    user_agent: String,
}

impl DuckDuckGoEngine {
    /// Construct an engine with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Start a builder for fine-tuning the underlying HTTP client.
    #[must_use]
    pub fn builder() -> DuckDuckGoEngineBuilder {
        DuckDuckGoEngineBuilder::default()
    }
}

impl Default for DuckDuckGoEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Configurable builder for [`DuckDuckGoEngine`].
#[derive(Debug, Clone)]
pub struct DuckDuckGoEngineBuilder {
    client: Option<Client>,
    endpoint: String,
    user_agent: String,
}

impl Default for DuckDuckGoEngineBuilder {
    fn default() -> Self {
        Self {
            client: None,
            endpoint: DEFAULT_ENDPOINT.into(),
            user_agent: DEFAULT_USER_AGENT.into(),
        }
    }
}

impl DuckDuckGoEngineBuilder {
    /// Override the underlying `reqwest` client (use this to set timeouts,
    /// proxies, etc.).
    #[must_use]
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Override the endpoint URL — primarily for tests against a local
    /// mock server. Production callers should leave this alone.
    #[must_use]
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Override the `User-Agent` header. Be honest — identify your tool.
    #[must_use]
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }

    /// Finalise the engine.
    #[must_use]
    pub fn build(self) -> DuckDuckGoEngine {
        DuckDuckGoEngine {
            client: self.client.unwrap_or_default(),
            endpoint: self.endpoint,
            user_agent: self.user_agent,
        }
    }
}

#[async_trait]
impl SearchEngine for DuckDuckGoEngine {
    fn name(&self) -> &'static str {
        ENGINE_NAME
    }

    async fn search(&self, query: &Query) -> Result<Vec<SearchHit>> {
        if !query.is_valid() {
            return Err(DorkError::InvalidQuery(
                "query is empty or limit < 1".into(),
            ));
        }

        let url = build_request_url(&self.endpoint, query)?;

        let response = self
            .client
            .get(url)
            .header(USER_AGENT, &self.user_agent)
            .header(ACCEPT, "text/html")
            .header(
                ACCEPT_LANGUAGE,
                query.region.as_deref().unwrap_or("en-US,en;q=0.9"),
            )
            .send()
            .await
            .map_err(|e| DorkError::Network(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(DorkError::RateLimited {
                retry_after_seconds: 30,
            });
        }
        if !status.is_success() {
            return Err(DorkError::Backend(
                format!("DuckDuckGo returned HTTP {status}").into(),
            ));
        }

        let body = response
            .text()
            .await
            .map_err(|e| DorkError::Network(e.to_string()))?;

        let mut hits = parse_results(&body)?;
        hits.truncate(query.limit);
        for hit in &mut hits {
            hit.source = Some(ENGINE_NAME.into());
        }
        Ok(hits)
    }
}

/// Build the GET URL with the rendered query and DDG-specific knobs.
fn build_request_url(endpoint: &str, query: &Query) -> Result<Url> {
    let mut url = Url::parse(endpoint).map_err(|e| DorkError::Backend(e.to_string().into()))?;

    let safe = match query.safe_search {
        SafeSearch::Strict => "1",
        SafeSearch::Moderate => "-1",
        SafeSearch::Off => "-2",
    };

    url.query_pairs_mut()
        .clear()
        .append_pair("q", &query.render_with_operators())
        .append_pair("kp", safe);

    if let Some(region) = &query.region {
        url.query_pairs_mut().append_pair("kl", region);
    }

    Ok(url)
}

// ===== HTML parsing =====

/// HTML parsing for the DuckDuckGo HTML lite result page.
///
/// Layout (stable as of 2026-Q2): each result is a `<div class="result">`
/// holding:
///
/// - `a.result__a`            — title + canonical-ish href (DDG redirect URL)
/// - `a.result__url`          — visible display URL
/// - `a.result__snippet`      — snippet text
///
/// The `href` on `result__a` looks like `//duckduckgo.com/l/?uddg=<encoded>&...`.
/// We unwrap that to get the real target.
fn parse_results(html: &str) -> Result<Vec<SearchHit>> {
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
    fn build_url_includes_query_and_safesearch() {
        let q = Query::new("rust async").with_safe_search(SafeSearch::Strict);
        let url = build_request_url(DEFAULT_ENDPOINT, &q).expect("url");
        let pairs: Vec<_> = url.query_pairs().collect();
        assert!(pairs.iter().any(|(k, v)| k == "q" && v == "rust async"));
        assert!(pairs.iter().any(|(k, v)| k == "kp" && v == "1"));
    }

    #[test]
    fn build_url_renders_site_and_filetype_operators() {
        let q = Query::new("crate")
            .with_site("docs.rs")
            .with_filetype("html");
        let url = build_request_url(DEFAULT_ENDPOINT, &q).expect("url");
        let q_pair = url
            .query_pairs()
            .find(|(k, _)| k == "q")
            .map(|(_, v)| v.into_owned())
            .expect("q present");
        assert_eq!(q_pair, "crate site:docs.rs filetype:html");
    }

    #[test]
    fn build_url_propagates_region() {
        let q = Query::new("x").with_region("fr-fr");
        let url = build_request_url(DEFAULT_ENDPOINT, &q).expect("url");
        assert!(url.query_pairs().any(|(k, v)| k == "kl" && v == "fr-fr"));
    }

    #[test]
    fn safesearch_off_is_minus_two() {
        let q = Query::new("x").with_safe_search(SafeSearch::Off);
        let url = build_request_url(DEFAULT_ENDPOINT, &q).expect("url");
        assert!(url.query_pairs().any(|(k, v)| k == "kp" && v == "-2"));
    }

    #[test]
    fn engine_name_is_duckduckgo() {
        assert_eq!(DuckDuckGoEngine::new().name(), "duckduckgo");
    }

    #[tokio::test]
    async fn search_rejects_empty_query() {
        let engine = DuckDuckGoEngine::new();
        let err = engine
            .search(&Query::new(""))
            .await
            .expect_err("must reject");
        assert!(matches!(err, DorkError::InvalidQuery(_)));
    }

    #[tokio::test]
    async fn search_against_mock_server_returns_hits() {
        let mut server = mockito::Server::new_async().await;
        let body = include_str!("../tests/fixtures/ddg_results.html");
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/.*".into()))
            .with_status(200)
            .with_header("content-type", "text/html; charset=utf-8")
            .with_body(body)
            .create_async()
            .await;

        let engine = DuckDuckGoEngine::builder()
            .endpoint(format!("{}/", server.url()))
            .build();

        let hits = engine
            .search(&Query::new("rust").with_limit(10))
            .await
            .expect("ok");
        mock.assert_async().await;

        assert!(!hits.is_empty(), "fixture has at least one result");
        let first = &hits[0];
        assert_eq!(first.rank, 1);
        assert!(first.url.starts_with("http"));
        assert_eq!(first.source.as_deref(), Some("duckduckgo"));
    }

    #[tokio::test]
    async fn search_truncates_to_query_limit() {
        let mut server = mockito::Server::new_async().await;
        let body = include_str!("../tests/fixtures/ddg_results.html");
        let _m = server
            .mock("GET", mockito::Matcher::Regex("/.*".into()))
            .with_status(200)
            .with_body(body)
            .create_async()
            .await;

        let engine = DuckDuckGoEngine::builder()
            .endpoint(format!("{}/", server.url()))
            .build();

        let hits = engine
            .search(&Query::new("rust").with_limit(1))
            .await
            .expect("ok");
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn search_maps_429_to_rate_limited() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", mockito::Matcher::Regex("/.*".into()))
            .with_status(429)
            .with_body("rate limited")
            .create_async()
            .await;

        let engine = DuckDuckGoEngine::builder()
            .endpoint(format!("{}/", server.url()))
            .build();

        let err = engine
            .search(&Query::new("x"))
            .await
            .expect_err("must error");
        assert!(matches!(err, DorkError::RateLimited { .. }));
    }

    #[tokio::test]
    async fn search_maps_5xx_to_backend() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", mockito::Matcher::Regex("/.*".into()))
            .with_status(503)
            .with_body("oops")
            .create_async()
            .await;

        let engine = DuckDuckGoEngine::builder()
            .endpoint(format!("{}/", server.url()))
            .build();

        let err = engine
            .search(&Query::new("x"))
            .await
            .expect_err("must error");
        assert!(matches!(err, DorkError::Backend(_)));
    }

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

//! `dork-google` — `SearchEngine` backend wrapping Google's [Custom Search
//! JSON API](https://developers.google.com/custom-search/v1/overview).
//!
//! Requires an API key (`key`) and a Programmable Search Engine ID (`cx`).
//! The free tier allows 100 queries/day; over that the API charges per
//! query, so wire your own caching upstream if you call this in a loop.
//!
//! ## Example
//!
//! ```no_run
//! use dork_core::{Query, SearchEngine};
//! use dork_google::GoogleEngine;
//!
//! # async fn run() -> dork_core::Result<()> {
//! let engine = GoogleEngine::builder()
//!     .api_key("AIza…")
//!     .cx("0123…")
//!     .build()
//!     .expect("api key + cx required");
//! let hits = engine.search(&Query::new("rust async patterns").with_limit(5)).await?;
//! # let _ = hits;
//! # Ok(()) }
//! ```

#![warn(missing_docs)]
#![deny(unsafe_code)]

use async_trait::async_trait;
use dork_core::{DorkError, Query, Result, SafeSearch, SearchEngine, SearchHit};
use reqwest::{Client, Url};
use serde::Deserialize;

const ENGINE_NAME: &str = "google";
const DEFAULT_ENDPOINT: &str = "https://www.googleapis.com/customsearch/v1";

/// Google Custom Search backend.
#[derive(Debug, Clone)]
pub struct GoogleEngine {
    client: Client,
    endpoint: String,
    api_key: String,
    cx: String,
}

impl GoogleEngine {
    /// Start a builder. Requires `api_key` + `cx` before [`GoogleEngineBuilder::build`].
    #[must_use]
    pub fn builder() -> GoogleEngineBuilder {
        GoogleEngineBuilder::default()
    }
}

/// Configurable builder for [`GoogleEngine`].
#[derive(Debug, Clone, Default)]
pub struct GoogleEngineBuilder {
    client: Option<Client>,
    endpoint: Option<String>,
    api_key: Option<String>,
    cx: Option<String>,
}

impl GoogleEngineBuilder {
    /// Override the underlying `reqwest` client.
    #[must_use]
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Override the endpoint URL — primarily for tests.
    #[must_use]
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Set the Google API key (required).
    #[must_use]
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Set the Programmable Search Engine ID (required).
    #[must_use]
    pub fn cx(mut self, cx: impl Into<String>) -> Self {
        self.cx = Some(cx.into());
        self
    }

    /// Finalise the engine.
    ///
    /// # Errors
    ///
    /// Returns [`DorkError::Auth`] if `api_key` or `cx` is missing.
    pub fn build(self) -> Result<GoogleEngine> {
        let api_key = self
            .api_key
            .ok_or_else(|| DorkError::Auth("api_key is required for Google".into()))?;
        let cx = self.cx.ok_or_else(|| {
            DorkError::Auth("cx (search engine id) is required for Google".into())
        })?;
        Ok(GoogleEngine {
            client: self.client.unwrap_or_default(),
            endpoint: self.endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.into()),
            api_key,
            cx,
        })
    }
}

#[derive(Debug, Deserialize)]
struct GoogleResponse {
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    snippet: String,
}

#[async_trait]
impl SearchEngine for GoogleEngine {
    fn name(&self) -> &'static str {
        ENGINE_NAME
    }

    async fn search(&self, query: &Query) -> Result<Vec<SearchHit>> {
        if !query.is_valid() {
            return Err(DorkError::InvalidQuery(
                "query is empty or limit < 1".into(),
            ));
        }

        let url = build_request_url(&self.endpoint, &self.api_key, &self.cx, query)?;

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| DorkError::Network(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(DorkError::Auth(format!("Google returned HTTP {status}")));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(DorkError::RateLimited {
                retry_after_seconds: 60,
            });
        }
        if !status.is_success() {
            return Err(DorkError::Backend(
                format!("Google returned HTTP {status}").into(),
            ));
        }

        let body: GoogleResponse = response
            .json()
            .await
            .map_err(|e| DorkError::Parse(e.to_string()))?;

        let hits: Vec<SearchHit> = body
            .items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                SearchHit::new(i + 1, item.title, item.link, item.snippet).with_source(ENGINE_NAME)
            })
            .collect();

        Ok(hits)
    }
}

fn build_request_url(endpoint: &str, key: &str, cx: &str, query: &Query) -> Result<Url> {
    let mut url = Url::parse(endpoint).map_err(|e| DorkError::Backend(e.to_string().into()))?;

    let safe = match query.safe_search {
        SafeSearch::Strict | SafeSearch::Moderate => "active",
        SafeSearch::Off => "off",
    };

    let limit = query.limit.clamp(1, 10).to_string();

    {
        let mut q = url.query_pairs_mut();
        q.append_pair("key", key);
        q.append_pair("cx", cx);
        q.append_pair("q", &query.render_with_operators());
        q.append_pair("num", &limit);
        q.append_pair("safe", safe);
        if let Some(region) = &query.region {
            q.append_pair("gl", region);
        }
        if let Some(filetype) = &query.filetype {
            q.append_pair("fileType", filetype);
        }
        if let Some(site) = &query.site {
            q.append_pair("siteSearch", site);
        }
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn builder_requires_api_key_and_cx() {
        let err = GoogleEngine::builder().build().expect_err("missing key");
        assert!(matches!(err, DorkError::Auth(_)));
        let err = GoogleEngine::builder()
            .api_key("k")
            .build()
            .expect_err("missing cx");
        assert!(matches!(err, DorkError::Auth(_)));
    }

    #[test]
    fn build_url_includes_required_params() {
        let q = Query::new("rust async").with_limit(5);
        let url = build_request_url(DEFAULT_ENDPOINT, "K", "CX", &q).expect("ok");
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(a, b)| (a.into_owned(), b.into_owned()))
            .collect();
        assert!(pairs.contains(&("key".into(), "K".into())));
        assert!(pairs.contains(&("cx".into(), "CX".into())));
        assert!(pairs.contains(&("q".into(), "rust async".into())));
        assert!(pairs.contains(&("num".into(), "5".into())));
    }

    #[test]
    fn build_url_caps_limit_at_ten() {
        let q = Query::new("x").with_limit(50);
        let url = build_request_url(DEFAULT_ENDPOINT, "K", "CX", &q).expect("ok");
        assert!(url.query_pairs().any(|(k, v)| k == "num" && v == "10"));
    }

    #[test]
    fn build_url_safe_off() {
        let q = Query::new("x").with_safe_search(SafeSearch::Off);
        let url = build_request_url(DEFAULT_ENDPOINT, "K", "CX", &q).expect("ok");
        assert!(url.query_pairs().any(|(k, v)| k == "safe" && v == "off"));
    }

    #[test]
    fn engine_name_is_google() {
        let engine = GoogleEngine::builder()
            .api_key("k")
            .cx("c")
            .build()
            .expect("ok");
        assert_eq!(engine.name(), "google");
    }

    #[tokio::test]
    async fn search_against_mock_returns_hits() {
        let mut server = mockito::Server::new_async().await;
        let body = r#"{
            "items": [
                {"title": "Rust", "link": "https://www.rust-lang.org/", "snippet": "lang"},
                {"title": "Crates", "link": "https://crates.io/", "snippet": "registry"}
            ]
        }"#;
        let _m = server
            .mock("GET", mockito::Matcher::Regex("/.*".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let engine = GoogleEngine::builder()
            .endpoint(server.url())
            .api_key("k")
            .cx("c")
            .build()
            .expect("ok");
        let hits = engine.search(&Query::new("rust")).await.expect("ok");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].rank, 1);
        assert_eq!(hits[0].url, "https://www.rust-lang.org/");
        assert_eq!(hits[1].rank, 2);
        assert_eq!(hits[0].source.as_deref(), Some("google"));
    }

    #[tokio::test]
    async fn search_rejects_empty_query() {
        let engine = GoogleEngine::builder()
            .api_key("k")
            .cx("c")
            .build()
            .expect("ok");
        let err = engine.search(&Query::new("")).await.expect_err("must err");
        assert!(matches!(err, DorkError::InvalidQuery(_)));
    }

    #[tokio::test]
    async fn search_maps_403_to_auth() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", mockito::Matcher::Regex("/.*".into()))
            .with_status(403)
            .with_body("forbidden")
            .create_async()
            .await;

        let engine = GoogleEngine::builder()
            .endpoint(server.url())
            .api_key("k")
            .cx("c")
            .build()
            .expect("ok");
        let err = engine.search(&Query::new("x")).await.expect_err("err");
        assert!(matches!(err, DorkError::Auth(_)));
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

        let engine = GoogleEngine::builder()
            .endpoint(server.url())
            .api_key("k")
            .cx("c")
            .build()
            .expect("ok");
        let err = engine.search(&Query::new("x")).await.expect_err("err");
        assert!(matches!(err, DorkError::RateLimited { .. }));
    }
}

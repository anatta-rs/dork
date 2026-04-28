//! `dork` — Unified search-engine abstraction with `DuckDuckGo` + Google backends.
//!
//! Backends implement the [`SearchEngine`] trait and the consumer plugs whichever
//! backend it wants. The trait is intentionally minimal — query in, ordered hits out,
//! no streaming, no facets.
//!
//! ## Features
//!
//! - `duckduckgo` (default): Scrape `DuckDuckGo` HTML endpoint (no API key required).
//! - `google`: Use Google Custom Search JSON API (requires API key + search engine ID).
//!
//! ## Example
//!
//! ```no_run
//! use dork::{SearchEngine, Query};
//! use dork::duckduckgo::DuckDuckGoEngine;
//!
//! # async fn search() -> dork::Result<()> {
//! let engine = DuckDuckGoEngine::new();
//! let hits = engine.search(&Query::new("rust async patterns").with_limit(10)).await?;
//! for hit in hits {
//!     println!("{} — {}", hit.title, hit.url);
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![deny(unsafe_code)]

pub mod engine;
pub mod error;

#[cfg(feature = "duckduckgo")]
pub mod duckduckgo;

#[cfg(feature = "google")]
pub mod google;

// Re-export public API
pub use engine::{Query, SafeSearch, SearchEngine, SearchHit};
pub use error::{DorkError, Result};

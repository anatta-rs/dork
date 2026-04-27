//! `dork-core` — abstract search-engine interface.
//!
//! Backends ([`dork-duckduckgo`](https://crates.io/crates/dork-duckduckgo),
//! [`dork-google`](https://crates.io/crates/dork-google), …) implement
//! the [`SearchEngine`] trait and the consumer ([`anatta-rs/search`](https://github.com/anatta-rs/search))
//! plugs whichever backend it wants. The trait is intentionally minimal —
//! query in, ordered hits out, no streaming, no facets.
//!
//! ## Example
//!
//! ```no_run
//! use dork_core::{SearchEngine, Query};
//!
//! async fn search<E: SearchEngine>(engine: &E) -> dork_core::Result<()> {
//!     let hits = engine.search(&Query::new("rust async patterns").with_limit(10)).await?;
//!     for hit in hits {
//!         println!("{} — {}", hit.title, hit.url);
//!     }
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![deny(unsafe_code)]

pub mod error;
pub mod query;
pub mod result;
pub mod traits;

pub use error::{DorkError, Result};
pub use query::{Query, SafeSearch};
pub use result::SearchHit;
pub use traits::SearchEngine;

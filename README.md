# dork

> Pluggable search-engine backends behind one trait. No daemon, no
> ingestion, no opinions on storage. Query in, ranked hits out.

`dork` (as in [_Google dorking_](https://en.wikipedia.org/wiki/Google_hacking))
is the abstraction layer search clients have always wanted: pick a
backend (DuckDuckGo HTML, Google Custom Search, …), call `search()`, get
typed `SearchHit`s back. That's the whole API.

```rust
use dork_core::{Query, SearchEngine};
use dork_duckduckgo::DuckDuckGoEngine;

#[tokio::main]
async fn main() -> dork_core::Result<()> {
    let engine = DuckDuckGoEngine::new();
    let hits = engine.search(&Query::new("rust async patterns").with_limit(10)).await?;
    for hit in hits {
        println!("{rank:>2}. {title}\n    {url}", rank = hit.rank, title = hit.title, url = hit.url);
    }
    Ok(())
}
```

## Crates

| Crate | What it is |
|---|---|
| [`dork-core`](crates/dork-core)             | Trait + types. `SearchEngine`, `Query`, `SearchHit`, `DorkError`. Zero backends. |
| [`dork-duckduckgo`](crates/dork-duckduckgo) | DuckDuckGo HTML lite scraper. No API key. |
| [`dork-google`](crates/dork-google)         | Google [Custom Search JSON API](https://developers.google.com/custom-search/v1/overview). Needs `key` + `cx`. |

The trait lives in `dork-core` so backends can depend on it without
pulling each other in. Add your own backend by `impl SearchEngine` for
your type — that's it.

## Why split out the trait

The same way [`tower::Service`](https://docs.rs/tower/) decouples HTTP
servers from middleware: you build pipelines that take any
`SearchEngine` and don't care what's behind it. Want to combine three
backends with fallback? Round-robin? Mock for tests? All of that lives
in user code, not in the engines themselves.

## Operational notes

- **No retries, no caching, no rate-limit tracking** — the trait keeps
  one query honest. Wrap the engine in your own decorator if you want
  any of those behaviours; we don't mandate a flavour.
- **HTML scrapers will drift.** When DDG changes its layout the
  `dork-duckduckgo` parser will fail. CI catches it via the fixture in
  `crates/dork-duckduckgo/tests/fixtures/`; refresh from a real query
  page when needed.
- **Authentication** is the backend's concern, not the trait's
  (Google needs a key, DDG doesn't).

## Contributing

```sh
make hooks    # install pre-commit + pre-push
make check    # fmt + clippy + test
make ci       # full CI including coverage gate (≥ 95%)
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0. See [LICENSE](LICENSE).

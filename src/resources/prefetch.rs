// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Parallel prefetch for remote image URLs.
//!
//! Scan the event stream for unique remote URLs, fan fetches out to
//! worker threads (capped at [`MAX_PARALLEL_FETCHES`]), cache the
//! results in a [`CachingResourceHandler`].

use std::collections::{HashMap, HashSet};
use std::io::Result;
use std::sync::mpsc;
use std::thread;

use pulldown_cmark::{Event, Tag};
use tracing::{event, Level};
use url::Url;

use super::curl::CurlResourceHandler;
use super::{MimeData, ResourceUrlHandler};
use crate::references::UrlBase;
use crate::Environment;

/// Upper bound on threads fanned out for a single render. Prevents a
/// pathological markdown document with hundreds of image URLs from
/// spawning hundreds of threads against one remote host.
const MAX_PARALLEL_FETCHES: usize = 8;

/// Collect unique `http` / `https` image URLs referenced by `events`.
///
/// Relative URLs are resolved against `env.base_url`. Duplicates are
/// deduplicated so a README that references the same badge twice
/// still only fetches it once.
#[must_use]
pub fn scan_remote_image_urls(events: &[Event<'_>], env: &Environment) -> Vec<Url> {
    let mut seen = HashSet::new();
    let mut urls = Vec::new();
    for event in events {
        let Event::Start(Tag::Image { dest_url, .. }) = event else {
            continue;
        };
        let Some(resolved) = env.resolve_reference(dest_url) else {
            continue;
        };
        if !matches!(resolved.scheme(), "http" | "https") {
            continue;
        }
        if seen.insert(resolved.clone()) {
            urls.push(resolved);
        }
    }
    urls
}

/// Fetch every URL in `urls` in parallel and return the results keyed
/// by URL. Individual fetch failures are logged and dropped rather
/// than propagated — the render can still fall back to rendering the
/// image as a link, which is what we'd do on a network error anyway.
#[must_use]
pub fn prefetch_remote(
    urls: Vec<Url>,
    user_agent: &'static str,
    read_limit: u64,
) -> HashMap<Url, MimeData> {
    if urls.is_empty() {
        return HashMap::new();
    }
    let (tx, rx) = mpsc::channel::<(Url, Result<MimeData>)>();
    let mut handles = Vec::with_capacity(urls.len().min(MAX_PARALLEL_FETCHES));
    for chunk in urls.chunks(MAX_PARALLEL_FETCHES.max(1)) {
        for url in chunk {
            let tx = tx.clone();
            let url = url.clone();
            handles.push(thread::spawn(move || {
                let result = CurlResourceHandler::create(read_limit, user_agent)
                    .and_then(|h| h.read_resource(&url));
                let _ = tx.send((url, result));
            }));
            if handles.len() >= MAX_PARALLEL_FETCHES {
                break;
            }
        }
    }
    drop(tx);
    let mut cache = HashMap::new();
    for (url, result) in rx {
        match result {
            Ok(data) => {
                cache.insert(url, data);
            }
            Err(err) => {
                event!(Level::DEBUG, %url, %err, "prefetch failed, falling through");
            }
        }
    }
    for handle in handles {
        let _ = handle.join();
    }
    cache
}

/// Resource handler that serves prefetched URL bytes first, then
/// delegates everything else to an inner handler.
pub struct CachingResourceHandler<H: ResourceUrlHandler> {
    cache: HashMap<Url, MimeData>,
    inner: H,
}

impl<H: ResourceUrlHandler> CachingResourceHandler<H> {
    /// Wrap `inner` with the given prefetched cache.
    pub fn new(cache: HashMap<Url, MimeData>, inner: H) -> Self {
        Self { cache, inner }
    }

    /// Wrap `inner` with an empty cache (every read falls through).
    pub fn passthrough(inner: H) -> Self {
        Self::new(HashMap::new(), inner)
    }
}

impl<H: ResourceUrlHandler> ResourceUrlHandler for CachingResourceHandler<H> {
    fn read_resource(&self, url: &Url) -> Result<MimeData> {
        if let Some(data) = self.cache.get(url) {
            return Ok(data.clone());
        }
        self.inner.read_resource(url)
    }
}

/// Convenience: scan + prefetch in one call. Returns a wrapping
/// handler that the render pipeline can use transparently.
pub fn prefetch_and_wrap<H: ResourceUrlHandler>(
    events: &[Event<'_>],
    env: &Environment,
    user_agent: &'static str,
    read_limit: u64,
    inner: H,
) -> CachingResourceHandler<H> {
    let urls = scan_remote_image_urls(events, env);
    if urls.is_empty() {
        return CachingResourceHandler::passthrough(inner);
    }
    event!(
        Level::DEBUG,
        count = urls.len(),
        "prefetching remote image URLs in parallel"
    );
    let cache = prefetch_remote(urls, user_agent, read_limit);
    CachingResourceHandler::new(cache, inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Environment {
        Environment::for_local_directory(&std::env::current_dir().unwrap()).unwrap()
    }

    #[test]
    fn scan_deduplicates_and_filters() {
        use pulldown_cmark::{CowStr, LinkType};
        let events = vec![
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://example.com/a.png"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://example.com/a.png"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("./local.png"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("ftp://example.com/x.png"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
        ];
        let urls = scan_remote_image_urls(&events, &env());
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].as_str(), "https://example.com/a.png");
    }

    #[test]
    fn empty_prefetch_returns_empty_cache() {
        let cache = prefetch_remote(Vec::new(), "test/0.0", 1024);
        assert!(cache.is_empty());
    }

    use std::io::ErrorKind;

    #[test]
    fn caching_handler_serves_cached_then_delegates() {
        let mut cache = HashMap::new();
        let url: Url = "https://example.com/a.png".parse().unwrap();
        cache.insert(
            url.clone(),
            MimeData {
                mime_type: None,
                data: b"cached".to_vec(),
            },
        );
        let handler = CachingResourceHandler::new(cache, super::super::NoopResourceHandler);
        assert_eq!(handler.read_resource(&url).unwrap().data, b"cached");
        let missing: Url = "https://example.com/b.png".parse().unwrap();
        assert!(matches!(
            handler.read_resource(&missing).map_err(|e| e.kind()),
            Err(ErrorKind::Unsupported)
        ));
    }
}

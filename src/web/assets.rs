//! Static asset handlers for the read-only web view: the stylesheet and the
//! embedded web fonts. Presentation-only; imports nothing from `engine` (just
//! axum/std), which is fine under convention principle 3.

use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

/// The single stylesheet, embedded at compile time from the crate root.
const CSS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/lazyspec.css"));

/// Look up the embedded bytes for a requested font file. Returns `None` when the
/// font is not embedded, which the handler maps to a 404.
///
/// The woff2 binaries are not yet in the tree (sandbox blocker), so no fonts are
/// embedded today and every request 404s. Adding a font is a one-line edit:
/// uncomment the matching arm once the binary lands in `static/fonts/`.
// The match has only commented-out arms today, so clippy sees a single binding;
// the form is deliberate so enabling a font is a one-line uncomment.
#[allow(clippy::match_single_binding)]
fn font_bytes(name: &str) -> Option<&'static [u8]> {
    match name {
        // Uncomment once the woff2 binaries are added to static/fonts/ (sandbox blocker).
        // "archivo-latin.woff2" => Some(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/fonts/archivo-latin.woff2"))),
        // "ibm-plex-mono-latin.woff2" => Some(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/fonts/ibm-plex-mono-latin.woff2"))),
        _ => None,
    }
}

/// `GET /static/lazyspec.css` -- the embedded stylesheet.
pub async fn stylesheet() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], CSS)
}

/// `GET /static/fonts/{name}` -- an embedded woff2 font, or 404 when not present.
pub async fn font(Path(name): Path<String>) -> Response {
    match font_bytes(&name) {
        Some(bytes) => ([(header::CONTENT_TYPE, "font/woff2")], bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

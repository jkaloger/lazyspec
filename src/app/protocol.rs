//! The in-process transport (RFC-054 §"The in-process bridge"): adapt a
//! webview `http::Request` into a call on the existing [`web::server::router`]
//! (`tower::Service`) and hand back the `http::Response`. No route is
//! reimplemented and no TCP port is bound.
//!
//! The adapter here is deliberately Tauri-free: it takes an `http::Request`
//! and the built axum `Router`, so it can be unit-tested with
//! `tower::ServiceExt::oneshot` exactly like [`crate::web`]'s route tests. The
//! Tauri custom-scheme glue that feeds it lives in [`super`]; this keeps the
//! only interesting logic (request/response conversion + service call)
//! verifiable without spinning up a webview.

use axum::body::{to_bytes, Body};
use axum::Router;
use tower::ServiceExt;

/// Drive one webview request through the axum `Router` and collect the full
/// response into a buffered `http::Response<Vec<u8>>` the webview can render.
///
/// `Router::oneshot` consumes a clone of the router as a `tower::Service`; the
/// axum request body is built from the incoming bytes, and the response body is
/// buffered because the webview responder wants owned bytes, not a stream. The
/// awaiting happens on the caller's runtime (the app-owned tokio runtime in
/// [`super::run`]); this function is runtime-agnostic.
pub async fn handle(router: Router, request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
    let (parts, body) = request.into_parts();

    let mut builder = http::Request::builder().method(parts.method).uri(parts.uri);
    if let Some(headers) = builder.headers_mut() {
        *headers = parts.headers;
    }
    let axum_request = builder
        .body(Body::from(body))
        .expect("reconstructing the request from validated parts cannot fail");

    let response = router
        .oneshot(axum_request)
        .await
        .expect("axum Router is Infallible");

    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, usize::MAX)
        .await
        .expect("buffering an in-process response body cannot fail")
        .to_vec();

    http::Response::from_parts(parts, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;

    fn echo_router() -> Router {
        Router::new()
            .route("/", get(|| async { "root-ok" }))
            .route(
                "/static/thing.css",
                get(|| async {
                    (
                        [(http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
                        ":root{}",
                    )
                }),
            )
            .route(
                "/missing",
                get(|| async { (http::StatusCode::NOT_FOUND, "nope") }),
            )
    }

    async fn call(uri: &str) -> http::Response<Vec<u8>> {
        let request = http::Request::builder()
            .method("GET")
            .uri(uri)
            .body(Vec::new())
            .unwrap();
        handle(echo_router(), request).await
    }

    #[tokio::test]
    async fn routes_get_root_through_the_service_and_returns_the_body() {
        let response = call("lazyspec://localhost/").await;
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(response.body(), b"root-ok");
    }

    #[tokio::test]
    async fn routes_static_asset_and_preserves_content_type_header() {
        let response = call("lazyspec://localhost/static/thing.css").await;
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("text/css; charset=utf-8"),
            "static asset headers must survive the bridge for the page to be styled"
        );
        assert_eq!(response.body(), b":root{}");
    }

    #[tokio::test]
    async fn propagates_non_ok_status_from_the_router() {
        let response = call("lazyspec://localhost/missing").await;
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
        assert_eq!(response.body(), b"nope");
    }
}

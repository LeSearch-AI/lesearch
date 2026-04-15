//! Embedded web UI static file server.
//!
//! Serves the pre-built React app from `web/dist/` (embedded at compile time
//! via `rust-embed`). Falls back to `index.html` for client-side routing.

use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

/// Embedded web UI assets from `web/dist/`.
#[derive(Embed)]
#[folder = "../../web/dist/"]
struct WebAssets;

/// Serve a static file from the embedded web assets.
///
/// Falls back to `index.html` for SPA routing.
#[allow(clippy::unused_async)]
pub async fn static_handler(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    serve_file(&path)
}

/// Serve the index page.
#[allow(clippy::unused_async)]
pub async fn index_handler() -> Response {
    serve_file("index.html")
}

/// Look up and serve an embedded file.
fn serve_file(path: &str) -> Response {
    match WebAssets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                file.data,
            )
                .into_response()
        }
        // SPA fallback: serve index.html for unknown paths
        None => match WebAssets::get("index.html") {
            Some(file) => Html(
                String::from_utf8_lossy(&file.data).to_string(),
            )
            .into_response(),
            None => (StatusCode::NOT_FOUND, "not found").into_response(),
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_html_is_embedded() {
        assert!(
            WebAssets::get("index.html").is_some(),
            "web/dist/index.html should be embedded"
        );
    }

    #[test]
    fn assets_directory_is_embedded() {
        let count = WebAssets::iter().count();
        assert!(
            count >= 2,
            "expected at least index.html + JS bundle, got {count}"
        );
    }
}

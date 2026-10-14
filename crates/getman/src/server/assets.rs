//! The browser UI's files, built into the binary.

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Ui;

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; frame-src 'self' blob: data:; \
worker-src 'self' blob:; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const NOT_BUILT: &str = r#"<!doctype html>
<meta charset="utf-8">
<title>Getman</title>
<style>body{font:16px/1.5 system-ui,sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem;color:#1c1b1a;background:#faf9f7}code{background:#eceae6;padding:.1em .3em;border-radius:3px}</style>
<h1>Getman is running, but this build has no UI</h1>
<p>The browser UI is built separately. From the repository, run:</p>
<pre><code>cd web
npm ci
npm run build
cd ..
cargo build --release</code></pre>
<p>Meanwhile the command line works: <code>getman run</code>, <code>getman send</code> and <code>getman tui</code>.</p>
"#;

/// Whether the UI was built into this binary.
pub(crate) fn built() -> bool {
    Ui::get("index.html").is_some()
}

/// Serves a file of the UI, or the UI's page for any other path so the
/// app can route it.
pub(crate) async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "no such endpoint").into_response();
    }
    if let Some(file) = Ui::get(path).filter(|_| !path.is_empty()) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // Vite names built assets by their content, so they never change.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return Response::builder()
            .header(header::CONTENT_TYPE, mime.essence_str())
            .header(header::CACHE_CONTROL, cache)
            .body(Body::from(file.data.into_owned()))
            .unwrap_or_default();
    }
    let page = Ui::get("index.html")
        .map_or_else(|| NOT_BUILT.as_bytes().to_vec(), |f| f.data.into_owned());
    let mut response = Response::new(Body::from(page));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    response
}

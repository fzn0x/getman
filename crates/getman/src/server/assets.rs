//! The browser UI's files, built into the binary. `npm run build` in `web/`
//! stores each one Brotli-compressed, in about a quarter of the room. A
//! browser that takes Brotli gets a file as it is stored, any other gets it
//! decompressed.

use std::borrow::Cow;
use std::io::Read as _;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
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
    Ui::get("index.html.br").is_some()
}

/// Serves a file of the UI, or the UI's page for any other path so the
/// app can route it.
pub(crate) async fn serve(uri: Uri, request: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "no such endpoint").into_response();
    }
    let as_stored = takes_brotli(&request);
    if let Some(stored) = Ui::get(&format!("{path}.br")) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // Vite names built assets by their content, so they never change.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return file(stored.data, as_stored, mime.essence_str(), cache);
    }
    let html = "text/html; charset=utf-8";
    let mut response = match Ui::get("index.html.br") {
        Some(page) => file(page.data, as_stored, html, "no-cache"),
        None => Response::builder()
            .header(header::CONTENT_TYPE, html)
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(NOT_BUILT))
            .unwrap_or_default(),
    };
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    response
}

/// Answers with one of the UI's stored files: as it is stored when
/// `as_stored`, for a browser that takes Brotli, or else decompressed.
fn file(stored: Cow<'static, [u8]>, as_stored: bool, content_type: &str, cache: &str) -> Response {
    let response = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, cache)
        // Which body a browser gets depends on its Accept-Encoding.
        .header(header::VARY, "accept-encoding");
    if as_stored {
        return response
            .header(header::CONTENT_ENCODING, "br")
            .body(Body::from(stored))
            .unwrap_or_default();
    }
    let mut plain = Vec::new();
    if brotli::Decompressor::new(&*stored, 4096)
        .read_to_end(&mut plain)
        .is_err()
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "this build's copy of the UI is damaged; rebuild it with npm run build in web/",
        )
            .into_response();
    }
    response.body(Body::from(plain)).unwrap_or_default()
}

/// Whether a request's `Accept-Encoding` takes Brotli: `br`, or else `*`,
/// listed with a weight above zero.
fn takes_brotli(request: &HeaderMap) -> bool {
    let mut any = false;
    let items = request
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','));
    for item in items {
        let mut parts = item.split(';').map(str::trim);
        let coding = parts.next().unwrap_or_default();
        let weight = parts
            .find_map(|p| p.strip_prefix("q=").or_else(|| p.strip_prefix("Q=")))
            .map_or(1.0, |q| q.trim().parse::<f32>().unwrap_or(0.0));
        if coding.eq_ignore_ascii_case("br") {
            return weight > 0.0;
        }
        if coding == "*" {
            any = weight > 0.0;
        }
    }
    any
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn accepting(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(header::ACCEPT_ENCODING, value.parse().unwrap());
        }
        headers
    }

    async fn bytes(response: Response) -> Vec<u8> {
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec()
    }

    #[test]
    fn brotli_is_sent_only_to_browsers_that_take_it() {
        assert!(takes_brotli(&accepting(&["gzip, deflate, br, zstd"])));
        assert!(takes_brotli(&accepting(&["gzip", "BR;q=0.5"])));
        assert!(takes_brotli(&accepting(&["*"])));
        assert!(takes_brotli(&accepting(&["*;q=0, br"])));
        assert!(!takes_brotli(&accepting(&[])));
        assert!(!takes_brotli(&accepting(&["gzip, deflate"])));
        assert!(!takes_brotli(&accepting(&["br;q=0"])));
        assert!(!takes_brotli(&accepting(&["br;q=0.000, *"])));
        assert!(!takes_brotli(&accepting(&["*;q=0"])));
        assert!(!takes_brotli(&accepting(&["br;q=nonsense"])));
    }

    #[tokio::test]
    async fn a_file_is_sent_as_stored_or_decompressed() {
        let text = b"console.log('Getman');\n".repeat(64);
        let mut stored = Vec::new();
        {
            let mut packer = brotli::CompressorWriter::new(&mut stored, 4096, 11, 22);
            packer.write_all(&text).unwrap();
        }

        let packed = file(
            Cow::Owned(stored.clone()),
            true,
            "text/javascript",
            "no-cache",
        );
        assert_eq!(packed.status(), StatusCode::OK);
        assert_eq!(packed.headers()[header::CONTENT_ENCODING], "br");
        assert_eq!(packed.headers()[header::VARY], "accept-encoding");
        assert_eq!(packed.headers()[header::CONTENT_TYPE], "text/javascript");
        assert_eq!(bytes(packed).await, stored);

        let plain = file(Cow::Owned(stored), false, "text/javascript", "no-cache");
        assert_eq!(plain.status(), StatusCode::OK);
        assert!(plain.headers().get(header::CONTENT_ENCODING).is_none());
        assert_eq!(plain.headers()[header::VARY], "accept-encoding");
        assert_eq!(bytes(plain).await, text);

        let damaged = file(
            Cow::Borrowed(&b"not brotli"[..]),
            false,
            "text/javascript",
            "no-cache",
        );
        assert_eq!(damaged.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// The web build stores every file compressed. Without a built UI there
    /// is nothing to check; CI builds it before the tests.
    #[test]
    fn every_built_file_is_stored_compressed() {
        for name in Ui::iter() {
            assert!(
                name.ends_with(".br"),
                "web/dist/{name} is not compressed; rebuild it with npm run build in web/"
            );
            let stored = Ui::get(&name).unwrap().data;
            let mut plain = Vec::new();
            brotli::Decompressor::new(&*stored, 4096)
                .read_to_end(&mut plain)
                .unwrap();
        }
    }
}

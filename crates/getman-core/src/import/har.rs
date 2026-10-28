//! Importing HTTP Archives (HAR 1.2), as saved from a browser's network
//! panel: a request per entry.
//!
//! Browsers add headers of their own, such as cookies, `Host` and the
//! HTTP/2 pseudo-headers; those are left out, so the requests read like
//! ones written by hand.

use serde_json::Value;

use super::{Collection, Import, Item, name_from_url, text, text_or_empty};
use crate::error::{Error, Result};
use crate::model::{Body, Folder, FormField, KeyValue, Request};

/// Headers the browser or the HTTP layer sets on its own.
const DROPPED: &[&str] = &[
    "accept-encoding",
    "connection",
    "content-length",
    "cookie",
    "dnt",
    "host",
    "keep-alive",
    "pragma",
    "priority",
    "proxy-connection",
    "referer",
    "te",
    "transfer-encoding",
    "upgrade",
    "upgrade-insecure-requests",
    "user-agent",
];

/// Reads a HAR file, already parsed from JSON.
pub fn parse(doc: &Value) -> Result<Import> {
    let Some(entries) = doc["log"]["entries"].as_array() else {
        return Err(Error::Invalid(
            "this is not a HAR file: it has no log.entries list".into(),
        ));
    };
    let mut warnings = Vec::new();
    let mut items = Vec::new();
    let mut with_cookies = 0;
    let mut with_credentials = 0;
    for entry in entries {
        let source = &entry["request"];
        let Some(url) = source["url"]
            .as_str()
            .filter(|u| !u.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        let method = text(&source["method"])
            .unwrap_or_else(|| "GET".into())
            .to_ascii_uppercase();
        let mut request = Request {
            name: name_from_url(&method, &url),
            method,
            url,
            ..Request::default()
        };
        let mut sent_cookies = source["cookies"].as_array().is_some_and(|c| !c.is_empty());
        for header in source["headers"].as_array().into_iter().flatten() {
            let Some(name) = text(&header["name"]) else {
                continue;
            };
            let lower = name.to_ascii_lowercase();
            if lower == "cookie" {
                sent_cookies = true;
            }
            if lower.starts_with(':')
                || lower.starts_with("sec-")
                || DROPPED.contains(&lower.as_str())
            {
                continue;
            }
            if lower == "authorization" || lower.contains("api-key") || lower.contains("token") {
                with_credentials += 1;
            }
            // HTTP/2 lowercases names; keep what was captured.
            request
                .headers
                .push(KeyValue::new(name, text_or_empty(&header["value"])));
        }
        if sent_cookies {
            with_cookies += 1;
        }
        request.body = body(&source["postData"], &request.name, &mut warnings);
        super::drop_implied_content_type(&mut request);
        items.push(Item::Request(request));
    }
    if with_cookies > 0 {
        warnings.push(format!(
            "cookies sent with {} were left out; sign in again from Getman, or add a Cookie header",
            super::count(with_cookies, "request", "requests")
        ));
    }
    if with_credentials > 0 {
        warnings.push(format!(
            "captured credentials were kept in {}; move them into secret variables before you commit",
            super::count(with_credentials, "header", "headers")
        ));
    }
    let name = doc["log"]["pages"]
        .as_array()
        .and_then(|pages| pages.first())
        .and_then(|page| text(&page["title"]))
        .filter(|t| !t.trim().is_empty())
        .or_else(|| first_host(entries))
        .unwrap_or_else(|| "Recorded requests".into());
    Ok(Import::Collection(Box::new(Collection {
        folder: Folder {
            name,
            ..Folder::default()
        },
        items,
        environments: Vec::new(),
        warnings,
        format: format!(
            "HAR {}",
            text(&doc["log"]["version"]).unwrap_or_else(|| "1.2".into())
        ),
    })))
}

fn first_host(entries: &[Value]) -> Option<String> {
    let url = text(&entries.first()?["request"]["url"])?;
    let rest = url.split_once("://").map_or(url.as_str(), |(_, r)| r);
    rest.split(['/', '?', '#'])
        .next()
        .filter(|h| !h.is_empty())
        .map(str::to_owned)
}

fn body(post: &Value, name: &str, warnings: &mut Vec<String>) -> Body {
    if !post.is_object() {
        return Body::None;
    }
    let mime = text_or_empty(&post["mimeType"]);
    let essence = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let content = text_or_empty(&post["text"]);
    let params: Vec<&Value> = post["params"].as_array().into_iter().flatten().collect();
    if essence == "application/x-www-form-urlencoded" {
        let fields = if params.is_empty() {
            url::form_urlencoded::parse(content.as_bytes())
                .map(|(k, v)| KeyValue::new(k, v))
                .collect()
        } else {
            params
                .iter()
                .map(|p| KeyValue::new(text_or_empty(&p["name"]), text_or_empty(&p["value"])))
                .collect()
        };
        return Body::Form { fields };
    }
    if essence.starts_with("multipart/form-data") && !params.is_empty() {
        let mut files = false;
        let fields = params
            .iter()
            .map(|p| {
                let file_name = text(&p["fileName"]).filter(|f| !f.is_empty());
                files |= file_name.is_some();
                FormField {
                    name: text_or_empty(&p["name"]),
                    value: if file_name.is_some() {
                        String::new()
                    } else {
                        text_or_empty(&p["value"])
                    },
                    file: file_name,
                    content_type: text(&p["contentType"]).filter(|t| !t.is_empty()),
                    enabled: true,
                }
            })
            .collect();
        if files {
            warnings.push(format!(
                "{name}: copy the uploaded files into the workspace under the names its multipart body lists"
            ));
        }
        return Body::Multipart { fields };
    }
    if content.is_empty() {
        return Body::None;
    }
    if essence.is_empty() {
        return Body::Text {
            content,
            content_type: None,
        };
    }
    // A multipart body without its fields keeps its boundary, so it can be
    // sent as it was captured.
    super::text_body(&mime, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::parse_document;

    const HAR: &str = include_str!("../../tests/fixtures/import/browser.har");

    fn requests(collection: &Collection) -> Vec<&Request> {
        collection
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Request(request) => Some(request),
                Item::Folder(..) => None,
            })
            .collect()
    }

    #[test]
    fn entries_become_requests_without_browser_headers() {
        let Import::Collection(c) = parse(&parse_document(HAR).unwrap()).unwrap() else {
            panic!()
        };
        assert_eq!(c.folder.name, "Shop");
        assert_eq!(c.format, "HAR 1.2");
        let list = requests(&c);
        assert_eq!(list.len(), 4);

        let get = list[0];
        assert_eq!(get.name, "GET /api/orders");
        assert_eq!(get.url, "https://shop.example.com/api/orders?page=2");
        let names: Vec<&str> = get.headers.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["accept", "authorization", "x-trace"]);

        let post = list[1];
        assert_eq!(post.method, "POST");
        assert!(
            matches!(&post.body, Body::Json { content } if content.contains("\"sku\": \"BOOK-42\""))
        );
        assert!(post.headers.iter().all(|h| h.name != "content-type"));

        let form = list[2];
        let Body::Form { fields } = &form.body else {
            panic!("{:?}", form.body)
        };
        assert_eq!(
            (fields[0].name.as_str(), fields[0].value.as_str()),
            ("q", "a b")
        );

        let upload = list[3];
        let Body::Multipart { fields } = &upload.body else {
            panic!("{:?}", upload.body)
        };
        assert_eq!(fields[0].file.as_deref(), Some("cat.png"));
        assert_eq!(fields[1].value, "Tom");
        assert!(
            upload
                .headers
                .iter()
                .all(|h| !h.name.eq_ignore_ascii_case("content-type"))
        );

        let warnings = c.warnings.join("\n");
        assert!(
            warnings.contains("cookies sent with 1 request"),
            "{warnings}"
        );
        assert!(warnings.contains("credentials"), "{warnings}");
        assert!(
            warnings.contains("cat.png") || warnings.contains("uploaded files"),
            "{warnings}"
        );
    }

    #[test]
    fn a_har_without_entries_is_refused() {
        assert!(parse(&parse_document(r#"{"log": {}}"#).unwrap()).is_err());
    }
}

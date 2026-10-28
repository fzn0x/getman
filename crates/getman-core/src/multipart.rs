//! Building `multipart/form-data` bodies as bytes.
//!
//! Getman writes the body itself rather than streaming it, so the bytes
//! are known before the request is signed: AWS Signature, OAuth 1.0, Hawk
//! and Akamai EdgeGrid hash them. The boundary lives in the request's
//! `Content-Type` header, which is how the body is found again when the
//! request is sent.

use crate::error::{Error, Result};
use crate::http::{Header, Part, find_header, set_header};

/// Most bytes a multipart body may hold. Bodies are built in memory, as
/// are the files of a binary body.
pub const MAX_BYTES: u64 = 256 * 1024 * 1024;

/// A boundary no part is likely to contain: 128 random bits.
fn new_boundary() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|e| Error::Http(format!("Could not get random bytes from the system: {e}")))?;
    Ok(format!("----getman{}", hex::encode(bytes)))
}

/// The `boundary` parameter of a `Content-Type` value.
fn boundary_of(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("boundary") {
            return None;
        }
        let value = value.trim().trim_matches('"');
        (!value.is_empty()).then(|| value.to_owned())
    })
}

/// The boundary the headers name, if they have one.
pub(crate) fn boundary(headers: &[Header]) -> Option<String> {
    boundary_of(find_header(headers, "content-type")?)
}

/// Makes the `Content-Type` header of a multipart body carry a boundary,
/// and returns it. A header that names one keeps it, one that names a
/// multipart type without gets one, and anything else is replaced.
pub(crate) fn ensure_boundary(headers: &mut Vec<Header>) -> Result<String> {
    if let Some(boundary) = boundary(headers) {
        return Ok(boundary);
    }
    let boundary = new_boundary()?;
    let media = find_header(headers, "content-type")
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|media| media.to_ascii_lowercase().starts_with("multipart/"))
        .unwrap_or("multipart/form-data")
        .to_owned();
    set_header(
        headers,
        "Content-Type",
        format!("{media}; boundary={boundary}"),
    );
    Ok(boundary)
}

/// Writes a name or file name into a `Content-Disposition` header the way
/// browsers do: quotes and line breaks are percent-encoded.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("%22"),
            '\r' => out.push_str("%0D"),
            '\n' => out.push_str("%0A"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The body for these parts. Files are read here; their paths were
/// already checked to lie inside the workspace when the request was
/// prepared.
pub(crate) fn encode(parts: &[Part], boundary: &str) -> Result<Vec<u8>> {
    let too_large = || {
        Error::Invalid(format!(
            "the multipart body is larger than {} MiB, the most Getman builds in memory",
            MAX_BYTES / 1024 / 1024
        ))
    };
    let mut size = 0_u64;
    for part in parts {
        let own = match &part.file {
            Some(path) => std::fs::metadata(path)
                .map_err(|e| Error::io(path, e))?
                .len(),
            None => part.value.len() as u64,
        };
        size = size.saturating_add(own);
    }
    if size > MAX_BYTES {
        return Err(too_large());
    }

    let mut body = Vec::with_capacity(usize::try_from(size).unwrap_or(0) + 256 * parts.len());
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        let mut disposition = format!("Content-Disposition: form-data; name={}", quote(&part.name));
        let mut content_type = part.content_type.clone();
        let data = match &part.file {
            Some(path) => {
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                disposition.push_str(&format!("; filename={}", quote(&file_name)));
                if content_type.is_none() {
                    content_type = Some(
                        mime_guess::from_path(path)
                            .first_or_octet_stream()
                            .essence_str()
                            .to_owned(),
                    );
                }
                std::fs::read(path).map_err(|e| Error::io(path, e))?
            }
            None => part.value.clone().into_bytes(),
        };
        body.extend_from_slice(disposition.as_bytes());
        body.extend_from_slice(b"\r\n");
        if let Some(content_type) = &content_type {
            if http::HeaderValue::from_str(content_type).is_err() || content_type.trim().is_empty()
            {
                return Err(Error::Invalid(format!(
                    "{content_type:?} is not a valid content type"
                )));
            }
            body.extend_from_slice(format!("Content-Type: {}\r\n", content_type.trim()).as_bytes());
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&data);
        body.extend_from_slice(b"\r\n");
        if body.len() as u64 > MAX_BYTES + 1024 * 1024 {
            return Err(too_large());
        }
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn text(name: &str, value: &str) -> Part {
        Part {
            name: name.into(),
            value: value.into(),
            file: None,
            content_type: None,
        }
    }

    #[test]
    fn parts_are_written_with_quoted_names_and_content_types() {
        let dir = tempfile::tempdir().unwrap();
        let file: PathBuf = dir.path().join("name.txt");
        std::fs::write(&file, "from a file").unwrap();
        let parts = [
            text("plain", "value"),
            text("quo\"te\r\nd", "x"),
            Part {
                content_type: Some("application/json".into()),
                ..text("json", "{}")
            },
            Part {
                name: "upload".into(),
                value: String::new(),
                file: Some(file),
                content_type: None,
            },
        ];
        let body = String::from_utf8(encode(&parts, "BOUND").unwrap()).unwrap();
        assert_eq!(
            body,
            concat!(
                "--BOUND\r\nContent-Disposition: form-data; name=\"plain\"\r\n\r\nvalue\r\n",
                "--BOUND\r\nContent-Disposition: form-data; name=\"quo%22te%0D%0Ad\"\r\n\r\nx\r\n",
                "--BOUND\r\nContent-Disposition: form-data; name=\"json\"\r\n",
                "Content-Type: application/json\r\n\r\n{}\r\n",
                "--BOUND\r\nContent-Disposition: form-data; name=\"upload\"; ",
                "filename=\"name.txt\"\r\nContent-Type: text/plain\r\n\r\n",
                "from a file\r\n--BOUND--\r\n"
            )
        );
    }

    #[test]
    fn quotes_and_line_breaks_in_names_are_percent_encoded() {
        assert_eq!(quote("a\"b\r\nc é"), "\"a%22b%0D%0Ac é\"");
    }

    #[test]
    fn an_invalid_content_type_and_a_missing_file_are_named() {
        let bad = Part {
            content_type: Some("text/plain\r\nX-Evil: 1".into()),
            ..text("a", "b")
        };
        let error = encode(&[bad], "B").unwrap_err();
        assert!(error.to_string().contains("not a valid content type"));
        let missing = Part {
            file: Some(PathBuf::from("no-such-file.bin")),
            ..text("f", "")
        };
        assert!(encode(&[missing], "B").is_err());
    }

    #[test]
    fn the_content_type_gets_a_boundary_and_keeps_one_it_has() {
        let mut headers = Vec::new();
        let first = ensure_boundary(&mut headers).unwrap();
        assert!(first.starts_with("----getman") && first.len() == 10 + 32);
        assert_eq!(
            find_header(&headers, "content-type"),
            Some(format!("multipart/form-data; boundary={first}").as_str())
        );
        assert_eq!(ensure_boundary(&mut headers).unwrap(), first);

        let mut headers = vec![Header::new("content-type", "multipart/related")];
        let boundary = ensure_boundary(&mut headers).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(
            headers[0].value,
            format!("multipart/related; boundary={boundary}")
        );

        let mut headers = vec![Header::new(
            "Content-Type",
            "multipart/form-data; boundary=\"mine\"",
        )];
        assert_eq!(ensure_boundary(&mut headers).unwrap(), "mine");

        let mut headers = vec![Header::new("Content-Type", "text/plain")];
        ensure_boundary(&mut headers).unwrap();
        assert!(
            headers[0]
                .value
                .starts_with("multipart/form-data; boundary=")
        );
    }

    #[test]
    fn bodies_over_the_limit_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("big.bin");
        let handle = std::fs::File::create(&file).unwrap();
        handle.set_len(MAX_BYTES + 1).unwrap();
        let part = Part {
            file: Some(file),
            ..text("big", "")
        };
        let error = encode(&[part], "B").unwrap_err();
        assert!(error.to_string().contains("256 MiB"), "{error}");
    }
}

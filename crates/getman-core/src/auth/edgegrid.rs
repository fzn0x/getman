//! Akamai EdgeGrid (EG1-HMAC-SHA256): an HMAC over the request line, the
//! chosen headers and, for POST, a hash of the body.

use std::time::SystemTime;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest as _, Sha256};

use super::{body_bytes, hmac_sha256, parse_url, utc_compact};
use crate::error::{Error, Result};
use crate::http::{Outgoing, find_header, set_header};
use crate::model::EdgeGrid;

/// Body bytes a POST signature covers unless the settings say otherwise.
const DEFAULT_MAX_BODY: u64 = 128 * 1024;

/// The settings with variables replaced.
pub(super) fn render(settings: &EdgeGrid, render: &mut dyn FnMut(&str) -> String) -> EdgeGrid {
    EdgeGrid {
        client_token: render(&settings.client_token).trim().to_owned(),
        client_secret: render(&settings.client_secret).trim().to_owned(),
        access_token: render(&settings.access_token).trim().to_owned(),
        base_url: render(&settings.base_url).trim().to_owned(),
        headers_to_sign: settings
            .headers_to_sign
            .iter()
            .map(|name| render(name).trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect(),
        ..settings.clone()
    }
}

/// Sends the request to the base URL, if one is set, and signs it.
pub(super) fn apply(s: &EdgeGrid, outgoing: &mut Outgoing, now: SystemTime) -> Result<()> {
    if s.client_token.is_empty() || s.client_secret.is_empty() || s.access_token.is_empty() {
        return Err(Error::Invalid(
            "Akamai EdgeGrid needs a client token, a client secret and an access token".into(),
        ));
    }
    if !s.base_url.is_empty() {
        let base = if s.base_url.contains("://") {
            s.base_url.clone()
        } else {
            format!("https://{}", s.base_url)
        };
        let base = url::Url::parse(&base)
            .map_err(|e| Error::Invalid(format!("{base:?} is not a valid base URL: {e}")))?;
        let mut url = parse_url(outgoing)?;
        url.set_scheme(base.scheme())
            .map_err(|()| Error::Invalid(format!("{base} cannot be a base URL")))?;
        url.set_host(base.host_str())
            .map_err(|e| Error::Invalid(format!("{base} cannot be a base URL: {e}")))?;
        url.set_port(base.port())
            .map_err(|()| Error::Invalid(format!("{base} cannot be a base URL")))?;
        outgoing.url = url.to_string();
    }
    let (date, clock) = utc_compact(now);
    let timestamp = format!(
        "{date}T{}:{}:{}+0000",
        &clock[..2],
        &clock[2..4],
        &clock[4..]
    );
    let header = authorization(s, outgoing, &timestamp, &crate::dynamic::uuid_v4())?;
    set_header(&mut outgoing.headers, "Authorization", header);
    Ok(())
}

/// The header for a given timestamp and nonce.
fn authorization(
    s: &EdgeGrid,
    outgoing: &Outgoing,
    timestamp: &str,
    nonce: &str,
) -> Result<String> {
    let url = parse_url(outgoing)?;
    let host = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_owned(),
    };
    let path = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    };
    let headers: Vec<String> = s
        .headers_to_sign
        .iter()
        .filter_map(|name| {
            let value = find_header(&outgoing.headers, name)?;
            Some(format!(
                "{}:{}",
                name.to_ascii_lowercase(),
                value.split_whitespace().collect::<Vec<_>>().join(" ")
            ))
        })
        .collect();
    let content_hash = if outgoing.method.eq_ignore_ascii_case("POST") {
        let Some(body) = body_bytes(&outgoing.body)? else {
            return Err(Error::Invalid(
                "Akamai EdgeGrid cannot hash a multipart body before it is sent".into(),
            ));
        };
        let max = usize::try_from(s.max_body.unwrap_or(DEFAULT_MAX_BODY)).unwrap_or(usize::MAX);
        if body.is_empty() {
            String::new()
        } else {
            BASE64.encode(Sha256::digest(&body[..body.len().min(max)]))
        }
    } else {
        String::new()
    };

    let unsigned = format!(
        "EG1-HMAC-SHA256 client_token={};access_token={};timestamp={timestamp};nonce={nonce};",
        s.client_token, s.access_token
    );
    let data = [
        outgoing.method.to_ascii_uppercase(),
        url.scheme().to_owned(),
        host,
        path,
        headers.join("\t"),
        content_hash,
        unsigned.clone(),
    ]
    .join("\t");
    let signing_key = BASE64.encode(hmac_sha256(
        s.client_secret.as_bytes(),
        timestamp.as_bytes(),
    ));
    let signature = BASE64.encode(hmac_sha256(signing_key.as_bytes(), data.as_bytes()));
    Ok(format!("{unsigned}signature={signature}"))
}

#[cfg(test)]
mod tests {
    //! Cases from Akamai's EdgeGrid test suite.

    use super::*;
    use crate::http::{Header, OutgoingBody, Payload};

    const HOST: &str = "https://akaa-baseurl-xxxxxxxxxxx-xxxxxxxxxxxxx.luna.akamaiapis.net";
    const TIMESTAMP: &str = "20140321T19:34:21+0000";
    const NONCE: &str = "nonce-xx-xxxx-xxxx-xxxx-xxxxxxxxxxxx";

    fn suite() -> EdgeGrid {
        EdgeGrid {
            client_token: "akab-client-token-xxx-xxxxxxxxxxxxxxxx".into(),
            client_secret: "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx=".into(),
            access_token: "akab-access-token-xxx-xxxxxxxxxxxxxxxx".into(),
            headers_to_sign: vec!["X-Test1".into(), "X-Test2".into(), "X-Test3".into()],
            max_body: Some(2048),
            ..EdgeGrid::default()
        }
    }

    fn signature(method: &str, path: &str, headers: &[(&str, &str)], data: &str) -> String {
        let outgoing = Outgoing {
            method: method.into(),
            url: format!("{HOST}{path}"),
            headers: headers
                .iter()
                .map(|(name, value)| Header::new(*name, *value))
                .collect(),
            body: if data.is_empty() {
                OutgoingBody::None
            } else {
                OutgoingBody::Bytes {
                    data: Payload(data.as_bytes().to_vec()),
                }
            },
        };
        let header = authorization(&suite(), &outgoing, TIMESTAMP, NONCE).unwrap();
        let prefix = format!(
            "EG1-HMAC-SHA256 client_token=akab-client-token-xxx-xxxxxxxxxxxxxxxx;access_token=akab-access-token-xxx-xxxxxxxxxxxxxxxx;timestamp={TIMESTAMP};nonce={NONCE};signature="
        );
        header.strip_prefix(&prefix).unwrap().to_owned()
    }

    #[test]
    fn requests_without_a_body() {
        assert_eq!(
            signature("GET", "/", &[], ""),
            "tL+y4hxyHxgWVD30X3pWnGKHcPzmrIF+LThiAOhMxYU="
        );
        assert_eq!(
            signature("GET", "/testapi/v1/t1?p1=1&p2=2", &[], ""),
            "hKDH1UlnQySSHjvIcZpDMbQHihTQ0XyVAKZaApabdeA="
        );
        assert_eq!(
            signature("POST", "/testapi/v1/t6", &[], ""),
            "1gEDxeQGD5GovIkJJGcBaKnZ+VaPtrc4qBUHixjsPCQ="
        );
        assert_eq!(
            signature(
                "GET",
                "/testapi/v1/configs/111;222;333?from=12345&limit=200000",
                &[],
                ""
            ),
            "pmQF7Is2+O4r/mMojPR4yeF58BrempNNoBX5/DT0Fxs="
        );
    }

    #[test]
    fn post_bodies_are_hashed_up_to_the_limit() {
        assert_eq!(
            signature(
                "POST",
                "/testapi/v1/t3",
                &[],
                "datadatadatadatadatadatadatadata"
            ),
            "hXm4iCxtpN22m4cbZb4lVLW5rhX8Ca82vCFqXzSTPe4="
        );
        let long = "d".repeat(2049);
        let exact = "d".repeat(2048);
        assert_eq!(
            signature("POST", "/testapi/v1/t3", &[], &long),
            "6Q6PiTipLae6n4GsSIDTCJ54bEbHUBp+4MUXrbQCBoY="
        );
        assert_eq!(
            signature("POST", "/testapi/v1/t3", &[], &exact),
            "6Q6PiTipLae6n4GsSIDTCJ54bEbHUBp+4MUXrbQCBoY="
        );
        assert_eq!(
            signature("PUT", "/testapi/v1/t6", &[], &"P".repeat(31)),
            "GNBWEYSEWOLtu+7dD52da2C39aX/Jchpon3K/AmBqBU="
        );
    }

    #[test]
    fn chosen_headers_are_signed_in_order() {
        assert_eq!(
            signature(
                "GET",
                "/testapi/v1/t4",
                &[("X-Test1", "test-simple-header")],
                ""
            ),
            "8F9AybcRw+PLxnvT+H0JRkjROrrUgsxJTnRXMzqvcwY="
        );
        assert_eq!(
            signature(
                "GET",
                "/testapi/v1/t4",
                &[("X-Test1", "\"     test-header-with-spaces     \"")],
                ""
            ),
            "ucq2AbjCNtobHfCTuS38fdkl5UDdWHZhQX46fYR8CqI="
        );
        assert_eq!(
            signature(
                "GET",
                "/testapi/v1/t4",
                &[("X-Test2", "t2"), ("X-Test1", "t1"), ("X-Test3", "t3")],
                ""
            ),
            "Wus73Nx8jOYM+kkBFF2q8D1EATRIMr0WLWwpLBgkBqY="
        );
        assert_eq!(
            signature(
                "GET",
                "/testapi/v1/t5",
                &[
                    ("X-Test2", "t2"),
                    ("X-Test1", "t1"),
                    ("X-Test3", "t3"),
                    ("X-Extra", "this won't be included")
                ],
                ""
            ),
            "Knd/jc0A5Ghhizjayr0AUUvl2MZjBpS3FDSzvtq4Ixc="
        );
    }

    #[test]
    fn the_base_url_replaces_the_host() {
        let settings = EdgeGrid {
            base_url: "akab-host.luna.akamaiapis.net".into(),
            ..suite()
        };
        let mut outgoing = Outgoing {
            method: "GET".into(),
            url: "http://localhost/papi/v1/groups?x=1".into(),
            headers: vec![],
            body: OutgoingBody::None,
        };
        apply(&settings, &mut outgoing, SystemTime::now()).unwrap();
        assert_eq!(
            outgoing.url,
            "https://akab-host.luna.akamaiapis.net/papi/v1/groups?x=1"
        );
        assert!(
            find_header(&outgoing.headers, "authorization")
                .unwrap()
                .starts_with("EG1-HMAC-SHA256 client_token=akab-client-token")
        );
        let error = apply(&EdgeGrid::default(), &mut outgoing, SystemTime::now()).unwrap_err();
        assert!(error.to_string().contains("client token"), "{error}");
    }
}

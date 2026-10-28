//! AWS Signature Version 4: an HMAC-SHA256 signature over a canonical form
//! of the request, sent in the `Authorization` header or, for a presigned
//! URL, in the query string.

use std::time::SystemTime;

use percent_encoding::percent_decode_str;
use sha2::{Digest as _, Sha256};

use super::{body_bytes, hmac_sha256, host_header, parse_url, rfc3986, utc_compact};
use crate::error::{Error, Result};
use crate::http::{Outgoing, set_header};
use crate::model::{ApiKeyLocation, AwsSigV4};

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";
/// Seconds a presigned URL stays valid unless the settings say otherwise.
const DEFAULT_EXPIRES: u64 = 3600;
/// Headers left out of the signature, because proxies and clients may add,
/// drop or change them on the way.
const UNSIGNED_HEADERS: &[&str] = &[
    "authorization",
    "connection",
    "expect",
    "keep-alive",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "user-agent",
    "x-amzn-trace-id",
];
/// Query parameters a presigned URL carries; earlier ones are replaced.
const PRESIGN_PARAMS: &[&str] = &[
    "x-amz-algorithm",
    "x-amz-credential",
    "x-amz-date",
    "x-amz-expires",
    "x-amz-signedheaders",
    "x-amz-security-token",
    "x-amz-signature",
];

/// The settings with variables replaced.
pub(super) fn render(settings: &AwsSigV4, render: &mut dyn FnMut(&str) -> String) -> AwsSigV4 {
    AwsSigV4 {
        access_key: render(&settings.access_key).trim().to_owned(),
        secret_key: render(&settings.secret_key),
        session_token: render(&settings.session_token).trim().to_owned(),
        region: render(&settings.region).trim().to_owned(),
        service: render(&settings.service).trim().to_owned(),
        ..settings.clone()
    }
}

/// Signs `outgoing` in place, as of `now`.
pub(super) fn sign(s: &AwsSigV4, outgoing: &mut Outgoing, now: SystemTime) -> Result<()> {
    if s.access_key.is_empty()
        || s.secret_key.is_empty()
        || s.region.is_empty()
        || s.service.is_empty()
    {
        return Err(Error::Invalid(
            "AWS Signature needs an access key, a secret key, a region and a service".into(),
        ));
    }
    let mut url = parse_url(outgoing)?;
    let (date, clock) = utc_compact(now);
    let amz_date = format!("{date}T{clock}Z");
    let scope = format!("{date}/{}/{}/aws4_request", s.region, s.service);
    let credential = format!("{}/{scope}", s.access_key);
    let s3 = s.service.eq_ignore_ascii_case("s3");
    let host = host_header(outgoing, &url);
    let mut query = query_pairs(&url);

    match s.location {
        ApiKeyLocation::Header => {
            let payload = payload_hash(s, outgoing)?;
            set_header(&mut outgoing.headers, "X-Amz-Date", &amz_date);
            if !s.session_token.is_empty() {
                set_header(
                    &mut outgoing.headers,
                    "X-Amz-Security-Token",
                    &s.session_token,
                );
            }
            if s3 || s.unsigned_payload {
                set_header(&mut outgoing.headers, "X-Amz-Content-Sha256", &payload);
            }
            let mut headers = vec![("host".to_owned(), host)];
            headers.extend(
                outgoing
                    .headers
                    .iter()
                    .map(|h| (h.name.trim().to_ascii_lowercase(), h.value.as_str()))
                    .filter(|(name, _)| {
                        name != "host" && !UNSIGNED_HEADERS.contains(&name.as_str())
                    })
                    .map(|(name, value)| (name, normalize_value(value))),
            );
            let canonical =
                canonical_request(&outgoing.method, url.path(), &query, &headers, &payload, s3);
            let signature = signature(
                &s.secret_key,
                &date,
                &s.region,
                &s.service,
                &amz_date,
                &canonical.text,
            );
            set_header(
                &mut outgoing.headers,
                "Authorization",
                format!(
                    "{ALGORITHM} Credential={credential}, SignedHeaders={}, Signature={signature}",
                    canonical.signed_headers
                ),
            );
        }
        ApiKeyLocation::Query => {
            // A presigned URL may be opened by anything, so it signs only the
            // host, and a body only when the service needs it.
            let payload = if s3 || s.unsigned_payload {
                UNSIGNED_PAYLOAD.to_owned()
            } else {
                payload_hash(s, outgoing)?
            };
            query.retain(|(name, _)| !PRESIGN_PARAMS.contains(&name.to_ascii_lowercase().as_str()));
            query.push(("X-Amz-Algorithm".into(), ALGORITHM.into()));
            query.push(("X-Amz-Credential".into(), credential));
            query.push(("X-Amz-Date".into(), amz_date.clone()));
            query.push((
                "X-Amz-Expires".into(),
                s.expires_in.unwrap_or(DEFAULT_EXPIRES).to_string(),
            ));
            if !s.session_token.is_empty() {
                query.push(("X-Amz-Security-Token".into(), s.session_token.clone()));
            }
            query.push(("X-Amz-SignedHeaders".into(), "host".into()));
            let headers = [("host".to_owned(), host)];
            let canonical =
                canonical_request(&outgoing.method, url.path(), &query, &headers, &payload, s3);
            let signature = signature(
                &s.secret_key,
                &date,
                &s.region,
                &s.service,
                &amz_date,
                &canonical.text,
            );
            query.push(("X-Amz-Signature".into(), signature));
        }
    }
    // Write the query the way it was signed, so a `+` or a space means the
    // same thing to the server as it did here.
    if query.is_empty() {
        url.set_query(None);
    } else {
        let text: Vec<String> = query
            .iter()
            .map(|(name, value)| format!("{}={}", rfc3986(name), rfc3986(value)))
            .collect();
        url.set_query(Some(&text.join("&")));
    }
    outgoing.url = url.to_string();
    Ok(())
}

/// The hex SHA-256 of the body, or `UNSIGNED-PAYLOAD` when asked.
fn payload_hash(s: &AwsSigV4, outgoing: &Outgoing) -> Result<String> {
    if s.unsigned_payload {
        return Ok(UNSIGNED_PAYLOAD.into());
    }
    Ok(hex::encode(Sha256::digest(body_bytes(outgoing)?)))
}

/// The query's names and values, decoded, in order.
fn query_pairs(url: &url::Url) -> Vec<(String, String)> {
    url.query()
        .map(|query| {
            url::form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default()
}

/// Trims a header value and folds runs of whitespace into one space.
fn normalize_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A canonical request and the names of the headers it signs.
#[derive(Debug)]
struct Canonical {
    text: String,
    signed_headers: String,
}

/// The canonical request. `path` is the path as sent; `query` holds
/// decoded names and values; `headers` lowercase names with normalized
/// values.
fn canonical_request(
    method: &str,
    path: &str,
    query: &[(String, String)],
    headers: &[(String, String)],
    payload_hash: &str,
    s3: bool,
) -> Canonical {
    let mut encoded: Vec<(String, String)> = query
        .iter()
        .map(|(name, value)| (rfc3986(name), rfc3986(value)))
        .collect();
    encoded.sort();
    let canonical_query: Vec<String> = encoded
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();

    let mut sorted: Vec<&(String, String)> = headers.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut merged: Vec<(String, String)> = Vec::new();
    for (name, value) in sorted {
        match merged.last_mut() {
            Some((last, values)) if last == name => {
                values.push(',');
                values.push_str(value);
            }
            _ => merged.push((name.clone(), value.clone())),
        }
    }
    let canonical_headers: String = merged
        .iter()
        .map(|(name, value)| format!("{name}:{value}\n"))
        .collect();
    let signed_headers = merged
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let text = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        method.to_ascii_uppercase(),
        canonical_uri(path, s3),
        canonical_query.join("&"),
    );
    Canonical {
        text,
        signed_headers,
    }
}

/// The path as AWS signs it. S3 signs each segment of the path encoded
/// once. Other services remove `.`, `..` and empty segments and encode the
/// path as sent once more, so `%20` becomes `%2520`.
fn canonical_uri(path: &str, s3: bool) -> String {
    if s3 {
        let segments: Vec<String> = path
            .split('/')
            .map(|segment| {
                let bytes: Vec<u8> = percent_decode_str(segment).collect();
                percent_encoding::percent_encode(&bytes, super::RFC3986).to_string()
            })
            .collect();
        let joined = segments.join("/");
        return if joined.starts_with('/') {
            joined
        } else {
            format!("/{joined}")
        };
    }
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    if segments.is_empty() {
        return "/".into();
    }
    let mut uri = String::new();
    for segment in &segments {
        uri.push('/');
        uri.push_str(&rfc3986(segment));
    }
    if path.ends_with('/') {
        uri.push('/');
    }
    uri
}

/// The hex signature of a canonical request.
fn signature(
    secret: &str,
    date: &str,
    region: &str,
    service: &str,
    amz_date: &str,
    canonical: &str,
) -> String {
    let scope = format!("{date}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "{ALGORITHM}\n{amz_date}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    );
    let key = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let key = hmac_sha256(&key, region.as_bytes());
    let key = hmac_sha256(&key, service.as_bytes());
    let key = hmac_sha256(&key, b"aws4_request");
    hex::encode(hmac_sha256(&key, string_to_sign.as_bytes()))
}

#[cfg(test)]
mod tests {
    //! Vectors from AWS's published SigV4 test suite: credentials
    //! `AKIDEXAMPLE`, region `us-east-1`, service `service`, signed at
    //! 2015-08-30T12:36:00Z.

    use std::time::{Duration, UNIX_EPOCH};

    use super::*;
    use crate::http::{Header, OutgoingBody, Payload, find_header};

    const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn suite(location: ApiKeyLocation) -> AwsSigV4 {
        AwsSigV4 {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: SECRET.into(),
            region: "us-east-1".into(),
            service: "service".into(),
            location,
            ..AwsSigV4::default()
        }
    }

    fn at() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_440_938_160)
    }

    fn get(url: &str) -> Outgoing {
        Outgoing {
            method: "GET".into(),
            url: url.into(),
            headers: vec![],
            body: OutgoingBody::None,
        }
    }

    fn signed(settings: &AwsSigV4, mut outgoing: Outgoing) -> Outgoing {
        sign(settings, &mut outgoing, at()).unwrap();
        outgoing
    }

    fn signature_of(outgoing: &Outgoing) -> String {
        if let Some(header) = find_header(&outgoing.headers, "authorization") {
            return header.rsplit("Signature=").next().unwrap().to_owned();
        }
        let url = url::Url::parse(&outgoing.url).unwrap();
        url.query_pairs()
            .find(|(name, _)| name == "X-Amz-Signature")
            .unwrap()
            .1
            .into_owned()
    }

    #[test]
    fn the_time_is_formatted_the_way_aws_wants() {
        assert_eq!(
            utc_compact(at()),
            ("20150830".to_owned(), "123600".to_owned())
        );
    }

    #[test]
    fn get_vanilla() {
        let outgoing = signed(
            &suite(ApiKeyLocation::Header),
            get("https://example.amazonaws.com/"),
        );
        assert_eq!(
            find_header(&outgoing.headers, "authorization"),
            Some(
                "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, SignedHeaders=host;x-amz-date, Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
            )
        );
        assert_eq!(
            find_header(&outgoing.headers, "x-amz-date"),
            Some("20150830T123600Z")
        );

        let presigned = signed(
            &suite(ApiKeyLocation::Query),
            get("https://example.amazonaws.com/"),
        );
        assert_eq!(
            signature_of(&presigned),
            "e93c787ed7f371d5c6b165c1b38ede9550f4dce4144713e844b25b7192d3865d"
        );
        assert!(
            presigned.url.contains("X-Amz-Expires=3600"),
            "{}",
            presigned.url
        );
        assert!(find_header(&presigned.headers, "authorization").is_none());
    }

    #[test]
    fn get_vanilla_query_order_key_case() {
        let url = "https://example.amazonaws.com/?Param2=value2&Param1=value1";
        let outgoing = signed(&suite(ApiKeyLocation::Header), get(url));
        assert_eq!(
            signature_of(&outgoing),
            "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500"
        );
        let presigned = signed(&suite(ApiKeyLocation::Query), get(url));
        assert_eq!(
            signature_of(&presigned),
            "86012e2c9ad4d77369f5d81c11f75158aae4f895a085212cc6d3f923d300bed5"
        );
    }

    #[test]
    fn get_vanilla_query_unreserved() {
        let chars = "-._~0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let url = format!("https://example.amazonaws.com/?{chars}={chars}");
        let outgoing = signed(&suite(ApiKeyLocation::Header), get(&url));
        assert_eq!(
            signature_of(&outgoing),
            "9c3e54bfcdf0b19771a7f523ee5669cdf59bc7cc0884027167c21bb143a40197"
        );
    }

    #[test]
    fn get_vanilla_with_session_token() {
        let settings = AwsSigV4 {
            session_token: "6e86291e8372ff2a2260956d9b8aae1d763fbf315fa00fa31553b73ebf194267"
                .into(),
            ..suite(ApiKeyLocation::Header)
        };
        let outgoing = signed(&settings, get("https://example.amazonaws.com/"));
        assert_eq!(
            signature_of(&outgoing),
            "07ec1639c89043aa0e3e2de82b96708f198cceab042d4a97044c66dd9f74e7f8"
        );
        assert!(
            find_header(&outgoing.headers, "authorization")
                .unwrap()
                .contains("SignedHeaders=host;x-amz-date;x-amz-security-token")
        );

        let presigned = signed(
            &AwsSigV4 {
                location: ApiKeyLocation::Query,
                ..settings
            },
            get("https://example.amazonaws.com/"),
        );
        assert_eq!(
            signature_of(&presigned),
            "7ff2b50b376cb4d151970630573d6291dc128cc5c2a12ffb237f73cc53f67b6c"
        );
    }

    #[test]
    fn get_header_value_trim() {
        let mut outgoing = get("https://example.amazonaws.com/");
        outgoing.headers = vec![
            Header::new("My-Header1", " value1"),
            Header::new("My-Header2", r#" "a   b   c""#),
            Header::new("User-Agent", "getman"),
        ];
        let outgoing = signed(&suite(ApiKeyLocation::Header), outgoing);
        assert_eq!(
            signature_of(&outgoing),
            "acc3ed3afb60bb290fc8d2dd0098b9911fcaa05412b367055dee359757a9c736"
        );
    }

    #[test]
    fn post_x_www_form_urlencoded() {
        let outgoing = Outgoing {
            method: "POST".into(),
            url: "https://example.amazonaws.com/".into(),
            headers: vec![
                Header::new("Content-Type", "application/x-www-form-urlencoded"),
                Header::new("Content-Length", "13"),
                Header::new(
                    "X-Amz-Content-Sha256",
                    "9095672bbd1f56dfc5b65f3e153adc8731a4a654192329106275f4c7b24d0b6e",
                ),
            ],
            body: OutgoingBody::Bytes {
                data: Payload(b"Param1=value1".to_vec()),
            },
        };
        let outgoing = signed(&suite(ApiKeyLocation::Header), outgoing);
        assert_eq!(
            signature_of(&outgoing),
            "d3875051da38690788ef43de4db0d8f280229d82040bfac253562e56c3f20e0b"
        );
    }

    #[test]
    fn post_vanilla() {
        let mut outgoing = get("https://example.amazonaws.com/");
        outgoing.method = "POST".into();
        let outgoing = signed(&suite(ApiKeyLocation::Header), outgoing);
        assert_eq!(
            signature_of(&outgoing),
            "5da7c1a2acd57cee7505fc6676e4e544621c30862966e37dddb68e92efbe5d6b"
        );
    }

    #[test]
    fn paths_as_the_suite_sends_them() {
        // The suite's request lines carry raw paths, which a real request
        // cannot; the canonical form of each matches the suite's.
        let headers = [
            ("host".to_owned(), "example.amazonaws.com".to_owned()),
            ("x-amz-date".to_owned(), "20150830T123600Z".to_owned()),
        ];
        let cases = [
            (
                "/ሴ",
                "/%E1%88%B4",
                "8318018e0b0f223aa2bbf98705b62bb787dc9c0e678f255a891fd03141be5d85",
            ),
            (
                "/example space/",
                "/example%20space/",
                "652487583200325589f1fba4c7e578f72c47cb61beeca81406b39ddec1366741",
            ),
            (
                "/./",
                "/",
                "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31",
            ),
            (
                "/example1/example2/../..",
                "/",
                "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31",
            ),
            (
                "//example//",
                "/example/",
                "9a624bd73a37c9a373b5312afbebe7a714a789de108f0bdfe846570885f57e84",
            ),
        ];
        for (path, uri, expected) in cases {
            let canonical = canonical_request("GET", path, &[], &headers, EMPTY, false);
            assert!(canonical.text.contains(&format!("\n{uri}\n")), "{path}");
            assert_eq!(
                signature(
                    SECRET,
                    "20150830",
                    "us-east-1",
                    "service",
                    "20150830T123600Z",
                    &canonical.text
                ),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn paths_are_encoded_twice_except_for_s3() {
        assert_eq!(canonical_uri("/a%20b/c", false), "/a%2520b/c");
        assert_eq!(canonical_uri("/a%20b//c+d", true), "/a%20b//c%2Bd");
        assert_eq!(canonical_uri("", false), "/");
    }

    #[test]
    fn s3_presigned_url_example() {
        // The presigned GET in the S3 documentation.
        let settings = AwsSigV4 {
            access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            location: ApiKeyLocation::Query,
            expires_in: Some(86_400),
            ..AwsSigV4::default()
        };
        let mut outgoing = get("https://examplebucket.s3.amazonaws.com/test.txt");
        sign(
            &settings,
            &mut outgoing,
            UNIX_EPOCH + Duration::from_secs(1_369_353_600),
        )
        .unwrap();
        assert_eq!(
            signature_of(&outgoing),
            "aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
        );
    }

    #[test]
    fn s3_sends_the_payload_hash_and_can_skip_it() {
        let settings = AwsSigV4 {
            service: "s3".into(),
            ..suite(ApiKeyLocation::Header)
        };
        let outgoing = signed(&settings, get("https://bucket.s3.amazonaws.com/a.txt"));
        assert_eq!(
            find_header(&outgoing.headers, "x-amz-content-sha256"),
            Some(EMPTY)
        );
        let unsigned = signed(
            &AwsSigV4 {
                unsigned_payload: true,
                ..settings
            },
            Outgoing {
                body: OutgoingBody::Multipart { parts: vec![] },
                ..get("https://bucket.s3.amazonaws.com/a.txt")
            },
        );
        assert_eq!(
            find_header(&unsigned.headers, "x-amz-content-sha256"),
            Some("UNSIGNED-PAYLOAD")
        );
    }

    #[test]
    fn multipart_bodies_are_hashed_as_they_are_sent() {
        let parts = vec![crate::http::Part {
            name: "field".into(),
            value: "value".into(),
            file: None,
            content_type: None,
        }];
        let mut multipart = Outgoing {
            method: "POST".into(),
            headers: vec![crate::http::Header::new(
                "Content-Type",
                "multipart/form-data; boundary=B",
            )],
            body: OutgoingBody::Multipart {
                parts: parts.clone(),
            },
            ..get("https://bucket.s3.amazonaws.com/")
        };
        let settings = AwsSigV4 {
            service: "s3".into(),
            ..suite(ApiKeyLocation::Header)
        };
        sign(&settings, &mut multipart, at()).unwrap();
        let body = crate::multipart::encode(&parts, "B").unwrap();
        assert_eq!(
            find_header(&multipart.headers, "x-amz-content-sha256"),
            Some(hex::encode(Sha256::digest(&body)).as_str())
        );
    }

    #[test]
    fn missing_settings_are_reported() {
        let mut outgoing = get("https://example.amazonaws.com/");
        let error = sign(&AwsSigV4::default(), &mut outgoing, at()).unwrap_err();
        assert!(error.to_string().contains("region"), "{error}");
    }

    #[test]
    fn queries_are_rewritten_as_signed() {
        let outgoing = signed(
            &suite(ApiKeyLocation::Header),
            get("https://example.amazonaws.com/?q=a+b&x"),
        );
        assert!(outgoing.url.ends_with("?q=a%20b&x="), "{}", outgoing.url);
    }
}

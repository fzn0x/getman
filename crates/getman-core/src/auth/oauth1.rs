//! OAuth 1.0a (RFC 5849): the request's method, URL and parameters signed
//! with the client's and the token's secrets, or with a private key.

use std::path::Path;
use std::time::SystemTime;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::Hmac;
use sha1::Sha1;
use sha2::{Digest as _, Sha256, Sha512};

use super::keys::{self, Scheme};
use super::{body_bytes, hmac, parse_url, random_bytes, rfc3986, unix_seconds};
use crate::error::{Error, Result};
use crate::http::{Header, Outgoing, OutgoingBody, Payload, find_header, set_header};
use crate::model::{OAuth1, OAuth1Location, OAuth1Method};

const FORM: &str = "application/x-www-form-urlencoded";

/// The settings with variables replaced.
pub(super) fn render(settings: &OAuth1, render: &mut dyn FnMut(&str) -> String) -> OAuth1 {
    OAuth1 {
        consumer_key: render(&settings.consumer_key).trim().to_owned(),
        consumer_secret: render(&settings.consumer_secret),
        token: render(&settings.token).trim().to_owned(),
        token_secret: render(&settings.token_secret),
        private_key: render(&settings.private_key),
        private_key_file: render(settings.private_key_file.trim()),
        realm: render(&settings.realm),
        callback: render(settings.callback.trim()),
        verifier: render(settings.verifier.trim()),
        version: render(settings.version.trim()),
        timestamp: render(settings.timestamp.trim()),
        nonce: render(settings.nonce.trim()),
        ..settings.clone()
    }
}

/// Signs the request and adds the parameters where the settings say.
pub(super) fn apply(
    s: &OAuth1,
    outgoing: &mut Outgoing,
    root: &Path,
    now: SystemTime,
) -> Result<()> {
    if s.consumer_key.is_empty() {
        return Err(Error::Invalid("OAuth 1.0 needs a consumer key".into()));
    }
    let form_body = find_header(&outgoing.headers, "content-type")
        .is_some_and(|t| t.trim().to_ascii_lowercase().starts_with(FORM));
    if s.location == OAuth1Location::Body {
        match &outgoing.body {
            OutgoingBody::None => {
                set_header(&mut outgoing.headers, "Content-Type", FORM);
            }
            OutgoingBody::Bytes { .. } if form_body => {}
            _ => {
                return Err(Error::Invalid(
                    "OAuth 1.0 parameters can go in the body only when it is a URL-encoded form"
                        .into(),
                ));
            }
        }
    }
    let form_body = form_body || s.location == OAuth1Location::Body;

    let mut oauth: Vec<(String, String)> = Vec::new();
    let mut add = |name: &str, value: &str| oauth.push((name.to_owned(), value.to_owned()));
    add("oauth_consumer_key", &s.consumer_key);
    if !s.token.is_empty() {
        add("oauth_token", &s.token);
    }
    add("oauth_signature_method", method_name(s.signature_method));
    let timestamp = if s.timestamp.is_empty() {
        unix_seconds(now).to_string()
    } else {
        s.timestamp.clone()
    };
    add("oauth_timestamp", &timestamp);
    let nonce = if s.nonce.is_empty() {
        hex::encode(random_bytes::<16>()?)
    } else {
        s.nonce.clone()
    };
    add("oauth_nonce", &nonce);
    add(
        "oauth_version",
        if s.version.is_empty() {
            "1.0"
        } else {
            &s.version
        },
    );
    if !s.callback.is_empty() {
        add("oauth_callback", &s.callback);
    }
    if !s.verifier.is_empty() {
        add("oauth_verifier", &s.verifier);
    }
    // The body hash extension covers bodies the signature does not.
    if s.body_hash && !form_body {
        let Some(body) = body_bytes(&outgoing.body)? else {
            return Err(Error::Invalid(
                "OAuth 1.0 cannot hash a multipart body before it is sent; turn off the body hash"
                    .into(),
            ));
        };
        let hash = match s.signature_method {
            OAuth1Method::HmacSha256 | OAuth1Method::RsaSha256 => Sha256::digest(&body).to_vec(),
            OAuth1Method::HmacSha512 | OAuth1Method::RsaSha512 => Sha512::digest(&body).to_vec(),
            OAuth1Method::HmacSha1 | OAuth1Method::RsaSha1 | OAuth1Method::Plaintext => {
                Sha1::digest(&body).to_vec()
            }
        };
        add("oauth_body_hash", &BASE64.encode(hash));
    }

    let mut url = parse_url(outgoing)?;
    let mut params = oauth.clone();
    if let Some(query) = url.query() {
        params.extend(url::form_urlencoded::parse(query.as_bytes()).into_owned());
    }
    if form_body && let OutgoingBody::Bytes { data } = &outgoing.body {
        params.extend(url::form_urlencoded::parse(&data.0).into_owned());
    }
    let base = base_string(&outgoing.method, &url, &params);
    let signature = signature(s, &base, root)?;
    oauth.push(("oauth_signature".into(), signature));

    match s.location {
        OAuth1Location::Header => {
            let mut fields = Vec::new();
            if !s.realm.is_empty() {
                fields.push(format!(
                    r#"realm="{}""#,
                    s.realm.replace('\\', "\\\\").replace('"', "\\\"")
                ));
            }
            fields.extend(
                oauth
                    .iter()
                    .map(|(name, value)| format!(r#"{}="{}""#, rfc3986(name), rfc3986(value))),
            );
            set_header(
                &mut outgoing.headers,
                "Authorization",
                format!("OAuth {}", fields.join(", ")),
            );
        }
        OAuth1Location::Query => {
            let added = encode_pairs(&oauth);
            let query = match url.query() {
                Some(existing) if !existing.is_empty() => format!("{existing}&{added}"),
                _ => added,
            };
            url.set_query(Some(&query));
            outgoing.url = url.to_string();
        }
        OAuth1Location::Body => {
            let added = encode_pairs(&oauth);
            let mut data = match &outgoing.body {
                OutgoingBody::Bytes { data } => data.0.clone(),
                _ => Vec::new(),
            };
            if !data.is_empty() {
                data.push(b'&');
            }
            data.extend_from_slice(added.as_bytes());
            outgoing.body = OutgoingBody::Bytes {
                data: Payload(data),
            };
            if find_header(&outgoing.headers, "content-type").is_none() {
                outgoing.headers.push(Header::new("Content-Type", FORM));
            }
        }
    }
    Ok(())
}

fn encode_pairs(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{}={}", rfc3986(name), rfc3986(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The signature base string (RFC 5849 section 3.4.1).
fn base_string(method: &str, url: &url::Url, params: &[(String, String)]) -> String {
    let mut base_uri = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
    if let Some(port) = url.port() {
        base_uri.push_str(&format!(":{port}"));
    }
    base_uri.push_str(url.path());

    let mut encoded: Vec<(String, String)> = params
        .iter()
        .map(|(name, value)| (rfc3986(name), rfc3986(value)))
        .collect();
    encoded.sort();
    let normalized = encoded
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    format!(
        "{}&{}&{}",
        method.to_ascii_uppercase(),
        rfc3986(&base_uri),
        rfc3986(&normalized)
    )
}

/// The base64 signature, or the plain key for PLAINTEXT.
fn signature(s: &OAuth1, base: &str, root: &Path) -> Result<String> {
    let key = format!(
        "{}&{}",
        rfc3986(&s.consumer_secret),
        rfc3986(&s.token_secret)
    );
    let rsa = |scheme: Scheme| -> Result<String> {
        let private = keys::load(&s.private_key, &s.private_key_file, root, "OAuth 1.0")?;
        Ok(BASE64.encode(keys::sign(&private, scheme, base.as_bytes())?))
    };
    match s.signature_method {
        OAuth1Method::HmacSha1 => {
            Ok(BASE64.encode(hmac::<Hmac<Sha1>>(key.as_bytes(), base.as_bytes())))
        }
        OAuth1Method::HmacSha256 => {
            Ok(BASE64.encode(hmac::<Hmac<Sha256>>(key.as_bytes(), base.as_bytes())))
        }
        OAuth1Method::HmacSha512 => {
            Ok(BASE64.encode(hmac::<Hmac<Sha512>>(key.as_bytes(), base.as_bytes())))
        }
        OAuth1Method::Plaintext => Ok(key),
        OAuth1Method::RsaSha1 => rsa(Scheme::RsaSha1),
        OAuth1Method::RsaSha256 => rsa(Scheme::RsaSha256),
        OAuth1Method::RsaSha512 => rsa(Scheme::RsaSha512),
    }
}

/// The method's name, as `oauth_signature_method` writes it.
fn method_name(method: OAuth1Method) -> &'static str {
    match method {
        OAuth1Method::HmacSha1 => "HMAC-SHA1",
        OAuth1Method::HmacSha256 => "HMAC-SHA256",
        OAuth1Method::HmacSha512 => "HMAC-SHA512",
        OAuth1Method::Plaintext => "PLAINTEXT",
        OAuth1Method::RsaSha1 => "RSA-SHA1",
        OAuth1Method::RsaSha256 => "RSA-SHA256",
        OAuth1Method::RsaSha512 => "RSA-SHA512",
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::super::keys::tests::{RSA_PKCS1, verify};
    use super::*;

    /// The photo-sharing example from the OAuth Core 1.0 specification.
    fn photos() -> OAuth1 {
        OAuth1 {
            consumer_key: "dpf43f3p2l4k3l03".into(),
            consumer_secret: "kd94hf93k423kf44".into(),
            token: "nnch734d00sl2jdk".into(),
            token_secret: "pfkkdhi9sl3r4s00".into(),
            timestamp: "1191242096".into(),
            nonce: "kllo9940pd9333jh".into(),
            ..OAuth1::default()
        }
    }

    fn get(url: &str) -> Outgoing {
        Outgoing {
            method: "GET".into(),
            url: url.into(),
            headers: vec![],
            body: OutgoingBody::None,
        }
    }

    fn signed(settings: &OAuth1, mut outgoing: Outgoing) -> Outgoing {
        apply(settings, &mut outgoing, Path::new("."), UNIX_EPOCH).unwrap();
        outgoing
    }

    fn header_param(outgoing: &Outgoing, name: &str) -> String {
        let header = find_header(&outgoing.headers, "authorization").unwrap();
        let start = header.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
        let rest = &header[start..];
        let value = &rest[..rest.find('"').unwrap()];
        percent_encoding::percent_decode_str(value)
            .decode_utf8()
            .unwrap()
            .into_owned()
    }

    #[test]
    fn hmac_sha1_matches_the_specification_example() {
        let outgoing = signed(
            &OAuth1 {
                realm: "Photos".into(),
                ..photos()
            },
            get("http://photos.example.net/photos?file=vacation.jpg&size=original"),
        );
        assert_eq!(
            header_param(&outgoing, "oauth_signature"),
            "tR3+Ty81lMeYAr/Fid0kMTYa/WM="
        );
        let header = find_header(&outgoing.headers, "authorization").unwrap();
        assert!(
            header.starts_with(r#"OAuth realm="Photos", oauth_consumer_key="dpf43f3p2l4k3l03""#),
            "{header}"
        );
        assert!(header.contains(r#"oauth_signature="tR3%2BTy81lMeYAr%2FFid0kMTYa%2FWM%3D""#));
    }

    #[test]
    fn the_base_string_follows_rfc_5849() {
        // Section 3.4.1.1, with the oauth_* parameters as listed there.
        let url =
            url::Url::parse("http://example.com/request?b5=%3D%253D&a3=a&c%40=&a2=r%20b").unwrap();
        let mut params: Vec<(String, String)> = [
            ("oauth_consumer_key", "9djdj82h48djs9d2"),
            ("oauth_token", "kkk9d7dh3k39sjv7"),
            ("oauth_signature_method", "HMAC-SHA1"),
            ("oauth_timestamp", "137131201"),
            ("oauth_nonce", "7d8f3e4a"),
        ]
        .into_iter()
        .map(|(n, v)| (n.to_owned(), v.to_owned()))
        .collect();
        params.extend(url::form_urlencoded::parse(url.query().unwrap().as_bytes()).into_owned());
        params.extend(url::form_urlencoded::parse(b"c2&a3=2+q").into_owned());
        assert_eq!(
            base_string("POST", &url, &params),
            "POST&http%3A%2F%2Fexample.com%2Frequest&a2%3Dr%2520b%26a3%3D2%2520q%26a3%3Da%26b5%3D%253D%25253D%26c%2540%3D%26c2%3D%26oauth_consumer_key%3D9djdj82h48djs9d2%26oauth_nonce%3D7d8f3e4a%26oauth_signature_method%3DHMAC-SHA1%26oauth_timestamp%3D137131201%26oauth_token%3Dkkk9d7dh3k39sjv7"
        );
    }

    #[test]
    fn plaintext_sends_the_secrets() {
        let outgoing = signed(
            &OAuth1 {
                signature_method: OAuth1Method::Plaintext,
                ..photos()
            },
            get("https://photos.example.net/photos"),
        );
        assert_eq!(
            header_param(&outgoing, "oauth_signature"),
            "kd94hf93k423kf44&pfkkdhi9sl3r4s00"
        );
    }

    #[test]
    fn rsa_signatures_verify() {
        let settings = OAuth1 {
            signature_method: OAuth1Method::RsaSha1,
            private_key: RSA_PKCS1.into(),
            ..photos()
        };
        let outgoing = signed(
            &settings,
            get("http://photos.example.net/photos?size=original"),
        );
        let signature = BASE64
            .decode(header_param(&outgoing, "oauth_signature"))
            .unwrap();
        let url = url::Url::parse(&outgoing.url).unwrap();
        let mut params: Vec<(String, String)> = [
            ("oauth_consumer_key", "dpf43f3p2l4k3l03"),
            ("oauth_token", "nnch734d00sl2jdk"),
            ("oauth_signature_method", "RSA-SHA1"),
            ("oauth_timestamp", "1191242096"),
            ("oauth_nonce", "kllo9940pd9333jh"),
            ("oauth_version", "1.0"),
            ("size", "original"),
        ]
        .into_iter()
        .map(|(n, v)| (n.to_owned(), v.to_owned()))
        .collect();
        params.sort();
        let base = base_string("GET", &url, &params);
        let key = keys::parse(RSA_PKCS1).unwrap();
        verify(&key, Scheme::RsaSha1, base.as_bytes(), &signature);
    }

    #[test]
    fn parameters_can_go_in_the_query_or_the_body() {
        let query = signed(
            &OAuth1 {
                location: OAuth1Location::Query,
                ..photos()
            },
            get("http://photos.example.net/photos?size=original"),
        );
        assert!(
            query
                .url
                .contains("?size=original&oauth_consumer_key=dpf43f3p2l4k3l03"),
            "{}",
            query.url
        );
        assert!(
            query.url.contains("&oauth_version=1.0&oauth_signature="),
            "{}",
            query.url
        );
        assert!(find_header(&query.headers, "authorization").is_none());

        let mut post = get("http://photos.example.net/photos?size=original");
        post.method = "POST".into();
        post.headers.push(Header::new("Content-Type", FORM));
        post.body = OutgoingBody::Bytes {
            data: Payload(b"file=vacation.jpg".to_vec()),
        };
        let body = signed(
            &OAuth1 {
                location: OAuth1Location::Body,
                ..photos()
            },
            post,
        );
        let OutgoingBody::Bytes { data } = &body.body else {
            panic!()
        };
        let text = String::from_utf8(data.0.clone()).unwrap();
        assert!(
            text.starts_with("file=vacation.jpg&oauth_consumer_key="),
            "{text}"
        );

        let mut json = get("http://photos.example.net/photos");
        json.body = OutgoingBody::Bytes {
            data: Payload(b"{}".to_vec()),
        };
        let error = apply(
            &OAuth1 {
                location: OAuth1Location::Body,
                ..photos()
            },
            &mut json,
            Path::new("."),
            UNIX_EPOCH,
        )
        .unwrap_err();
        assert!(error.to_string().contains("URL-encoded form"), "{error}");
    }

    #[test]
    fn body_hashes_and_fresh_nonces() {
        let mut post = get("https://api.example.com/notes");
        post.method = "POST".into();
        post.body = OutgoingBody::Bytes {
            data: Payload(b"Hello World!".to_vec()),
        };
        let settings = OAuth1 {
            body_hash: true,
            nonce: String::new(),
            ..photos()
        };
        let outgoing = signed(&settings, post);
        assert_eq!(
            header_param(&outgoing, "oauth_body_hash"),
            "Lve95gjOVATpfV8EL5X4nxwjKHE="
        );
        let again = signed(&settings, outgoing.clone());
        assert_ne!(
            header_param(&outgoing, "oauth_nonce"),
            header_param(&again, "oauth_nonce")
        );
        assert_eq!(header_param(&outgoing, "oauth_nonce").len(), 32);
    }
}

//! Hawk: an HMAC over the request's method, resource, host and port, and
//! optionally a hash of its body.

use std::time::SystemTime;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::Hmac;
use sha1::Sha1;
use sha2::{Digest as _, Sha256};

use super::{body_bytes, hmac, parse_url, random_bytes, unix_seconds};
use crate::error::{Error, Result};
use crate::http::{Outgoing, find_header, set_header};
use crate::model::{Hawk, HawkAlgorithm};

/// The settings with variables replaced.
pub(super) fn render(settings: &Hawk, render: &mut dyn FnMut(&str) -> String) -> Hawk {
    Hawk {
        id: render(&settings.id).trim().to_owned(),
        key: render(&settings.key),
        ext: render(&settings.ext),
        app: render(settings.app.trim()),
        dlg: render(settings.dlg.trim()),
        ..settings.clone()
    }
}

/// Adds the `Authorization: Hawk ...` header.
pub(super) fn apply(s: &Hawk, outgoing: &mut Outgoing, now: SystemTime) -> Result<()> {
    let nonce = hex::encode(random_bytes::<8>()?);
    let header = authorization(s, outgoing, unix_seconds(now), &nonce)?;
    set_header(&mut outgoing.headers, "Authorization", header);
    Ok(())
}

/// The header for a given time and nonce.
pub(super) fn authorization(s: &Hawk, outgoing: &Outgoing, ts: u64, nonce: &str) -> Result<String> {
    if s.id.is_empty() || s.key.is_empty() {
        return Err(Error::Invalid("Hawk needs an ID and a key".into()));
    }
    let url = parse_url(outgoing)?;
    let resource = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    };
    let host = url.host_str().unwrap_or_default();
    let port = url.port_or_known_default().unwrap_or_default();

    let hash = if s.payload_hash {
        let Some(body) = body_bytes(&outgoing.body)? else {
            return Err(Error::Invalid(
                "Hawk cannot hash a multipart body before it is sent; turn off the payload hash"
                    .into(),
            ));
        };
        let content_type = find_header(&outgoing.headers, "content-type")
            .and_then(|t| t.split(';').next())
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let mut normalized = format!("hawk.1.payload\n{content_type}\n").into_bytes();
        normalized.extend_from_slice(&body);
        normalized.push(b'\n');
        Some(BASE64.encode(match s.algorithm {
            HawkAlgorithm::Sha256 => Sha256::digest(&normalized).to_vec(),
            HawkAlgorithm::Sha1 => Sha1::digest(&normalized).to_vec(),
        }))
    } else {
        None
    };

    let mut normalized = format!(
        "hawk.1.header\n{ts}\n{nonce}\n{}\n{resource}\n{}\n{port}\n{}\n{}\n",
        outgoing.method.to_ascii_uppercase(),
        host.to_ascii_lowercase(),
        hash.as_deref().unwrap_or_default(),
        s.ext.replace('\\', "\\\\").replace('\n', "\\n"),
    );
    if !s.app.is_empty() {
        normalized.push_str(&format!("{}\n{}\n", s.app, s.dlg));
    }
    let mac = BASE64.encode(match s.algorithm {
        HawkAlgorithm::Sha256 => hmac::<Hmac<Sha256>>(s.key.as_bytes(), normalized.as_bytes()),
        HawkAlgorithm::Sha1 => hmac::<Hmac<Sha1>>(s.key.as_bytes(), normalized.as_bytes()),
    });

    let quote = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    let mut header = format!(r#"Hawk id="{}", ts="{ts}", nonce="{nonce}""#, quote(&s.id));
    if let Some(hash) = &hash {
        header.push_str(&format!(r#", hash="{hash}""#));
    }
    if !s.ext.is_empty() {
        header.push_str(&format!(r#", ext="{}""#, quote(&s.ext)));
    }
    header.push_str(&format!(r#", mac="{mac}""#));
    if !s.app.is_empty() {
        header.push_str(&format!(r#", app="{}""#, quote(&s.app)));
        if !s.dlg.is_empty() {
            header.push_str(&format!(r#", dlg="{}""#, quote(&s.dlg)));
        }
    }
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Header, OutgoingBody, Payload};

    /// The credentials from the Hawk protocol's README.
    fn readme() -> Hawk {
        Hawk {
            id: "dh37fgj492je".into(),
            key: "werxhqb98rpaxn39848xrunpaw3489ruxnpa98w4rxn".into(),
            ext: "some-app-ext-data".into(),
            ..Hawk::default()
        }
    }

    fn request(method: &str) -> Outgoing {
        Outgoing {
            method: method.into(),
            url: "http://example.com:8000/resource/1?b=1&a=2".into(),
            headers: vec![],
            body: OutgoingBody::None,
        }
    }

    #[test]
    fn matches_the_readme_example() {
        let header = authorization(&readme(), &request("GET"), 1_353_832_234, "j4h3g2").unwrap();
        assert_eq!(
            header,
            r#"Hawk id="dh37fgj492je", ts="1353832234", nonce="j4h3g2", ext="some-app-ext-data", mac="6R4rV5iE+NPoym+WwjeHzjAGXUtLNIxmo1vpMofpLAE=""#
        );
    }

    #[test]
    fn matches_the_readme_example_with_a_payload() {
        let mut post = request("POST");
        post.headers
            .push(Header::new("Content-Type", "text/plain; charset=utf-8"));
        post.body = OutgoingBody::Bytes {
            data: Payload(b"Thank you for flying Hawk".to_vec()),
        };
        let settings = Hawk {
            payload_hash: true,
            ..readme()
        };
        let header = authorization(&settings, &post, 1_353_832_234, "j4h3g2").unwrap();
        assert_eq!(
            header,
            r#"Hawk id="dh37fgj492je", ts="1353832234", nonce="j4h3g2", hash="Yi9LfIIFRtBEPt74PVmbTF/xVAwPn7ub15ePICfgnuY=", ext="some-app-ext-data", mac="aSe1DERmZuRl3pI36/9BdZmnErTw3sNzOOAUlfeKjVw=""#
        );
    }

    #[test]
    fn delegation_and_sha1_and_fresh_nonces() {
        let settings = Hawk {
            algorithm: HawkAlgorithm::Sha1,
            app: "app1".into(),
            dlg: "app0".into(),
            ..readme()
        };
        let header = authorization(&settings, &request("GET"), 1, "n").unwrap();
        assert!(header.ends_with(r#", app="app1", dlg="app0""#), "{header}");
        let mut outgoing = request("GET");
        apply(&settings, &mut outgoing, SystemTime::now()).unwrap();
        let first = find_header(&outgoing.headers, "authorization")
            .unwrap()
            .to_owned();
        apply(&settings, &mut outgoing, SystemTime::now()).unwrap();
        assert_ne!(
            find_header(&outgoing.headers, "authorization").unwrap(),
            first
        );
        let error = apply(&Hawk::default(), &mut request("GET"), SystemTime::now()).unwrap_err();
        assert!(error.to_string().contains("ID and a key"), "{error}");
    }
}

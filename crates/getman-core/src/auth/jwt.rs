//! JSON Web Tokens signed when the request is sent, and Atlassian ASAP,
//! which is a JWT with fixed claims.

use std::path::Path;
use std::time::SystemTime;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use hmac::Hmac;
use serde_json::{Map, Value};
use sha2::{Sha256, Sha384, Sha512};

use super::keys::{self, Scheme};
use super::{hmac, unix_seconds};
use crate::error::{Error, Result};
use crate::http::{Outgoing, set_header};
use crate::model::{ApiKeyLocation, Asap, Jwt, JwtAlgorithm};

/// How long an ASAP token lives unless the settings say otherwise; the
/// specification allows at most an hour.
const ASAP_LIFETIME: u64 = 3600;

/// The JWT settings with variables replaced.
pub(super) fn render(settings: &Jwt, render: &mut dyn FnMut(&str) -> String) -> Jwt {
    Jwt {
        secret: render(&settings.secret),
        private_key: render(&settings.private_key),
        private_key_file: render(settings.private_key_file.trim()),
        header: render(&settings.header),
        payload: render(&settings.payload),
        prefix: render(settings.prefix.trim()),
        param: render(settings.param.trim()),
        ..settings.clone()
    }
}

/// The ASAP settings with variables replaced.
pub(super) fn render_asap(settings: &Asap, render: &mut dyn FnMut(&str) -> String) -> Asap {
    Asap {
        issuer: render(&settings.issuer).trim().to_owned(),
        subject: render(&settings.subject).trim().to_owned(),
        audience: render(&settings.audience).trim().to_owned(),
        key_id: render(&settings.key_id).trim().to_owned(),
        private_key: render(&settings.private_key),
        private_key_file: render(settings.private_key_file.trim()),
        claims: render(&settings.claims),
        ..settings.clone()
    }
}

/// Signs a JWT and adds it to the request.
pub(super) fn apply(s: &Jwt, outgoing: &mut Outgoing, root: &Path, now: SystemTime) -> Result<()> {
    let mut header = json_object(&s.header, "the JWT header")?;
    header.remove("alg");
    let typ = header.remove("typ").unwrap_or_else(|| "JWT".into());
    let mut fields = Map::new();
    fields.insert("alg".into(), algorithm_name(s.algorithm).into());
    fields.insert("typ".into(), typ);
    fields.extend(header);

    let mut claims = json_object(&s.payload, "the JWT payload")?;
    let now = unix_seconds(now);
    if s.issued_at {
        claims.insert("iat".into(), now.into());
    }
    if let Some(seconds) = s.expires_in {
        claims.insert("exp".into(), now.saturating_add(seconds).into());
    }

    let token = encode(&fields, &claims, s.algorithm, |algorithm, input| {
        sign_input(algorithm, input, s, root)
    })?;
    match s.location {
        ApiKeyLocation::Header => {
            let prefix = if s.prefix.is_empty() {
                "Bearer"
            } else {
                &s.prefix
            };
            set_header(
                &mut outgoing.headers,
                "Authorization",
                format!("{prefix} {token}"),
            );
        }
        ApiKeyLocation::Query => {
            let mut url = super::parse_url(outgoing)?;
            let name = if s.param.is_empty() {
                "token"
            } else {
                &s.param
            };
            url.query_pairs_mut().append_pair(name, &token);
            outgoing.url = url.to_string();
        }
    }
    Ok(())
}

/// Signs an ASAP token and sends it as a Bearer token.
pub(super) fn apply_asap(
    s: &Asap,
    outgoing: &mut Outgoing,
    root: &Path,
    now: SystemTime,
) -> Result<()> {
    if s.issuer.is_empty() || s.audience.is_empty() || s.key_id.is_empty() {
        return Err(Error::Invalid(
            "Atlassian ASAP needs an issuer, an audience and a key ID".into(),
        ));
    }
    let algorithm = s.algorithm.unwrap_or(JwtAlgorithm::Rs256);
    if matches!(
        algorithm,
        JwtAlgorithm::Hs256 | JwtAlgorithm::Hs384 | JwtAlgorithm::Hs512
    ) {
        return Err(Error::Invalid(
            "Atlassian ASAP signs with a private key; choose an RS, PS or ES algorithm".into(),
        ));
    }
    let mut header = Map::new();
    header.insert("alg".into(), algorithm_name(algorithm).into());
    header.insert("typ".into(), "JWT".into());
    header.insert("kid".into(), s.key_id.clone().into());

    let mut claims = json_object(&s.claims, "the ASAP claims")?;
    let now = unix_seconds(now);
    claims.insert("iss".into(), s.issuer.clone().into());
    if !s.subject.is_empty() {
        claims.insert("sub".into(), s.subject.clone().into());
    }
    let audiences: Vec<&str> = s
        .audience
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .collect();
    claims.insert(
        "aud".into(),
        match audiences.as_slice() {
            [one] => (*one).into(),
            many => many.to_vec().into(),
        },
    );
    claims.insert("jti".into(), crate::dynamic::uuid_v4().into());
    claims.insert("iat".into(), now.into());
    claims.insert(
        "exp".into(),
        now.saturating_add(s.expires_in.unwrap_or(ASAP_LIFETIME))
            .into(),
    );

    let token = encode(&header, &claims, algorithm, |algorithm, input| {
        let key = keys::load(&s.private_key, &s.private_key_file, root, "Atlassian ASAP")?;
        keys::sign(&key, asymmetric(algorithm), input)
    })?;
    set_header(
        &mut outgoing.headers,
        "Authorization",
        format!("Bearer {token}"),
    );
    Ok(())
}

/// Encodes and signs a token: `header.claims.signature`, each in base64url.
fn encode(
    header: &Map<String, Value>,
    claims: &Map<String, Value>,
    algorithm: JwtAlgorithm,
    sign: impl FnOnce(JwtAlgorithm, &[u8]) -> Result<Vec<u8>>,
) -> Result<String> {
    let input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(Value::Object(header.clone()).to_string()),
        URL_SAFE_NO_PAD.encode(Value::Object(claims.clone()).to_string()),
    );
    let signature = sign(algorithm, input.as_bytes())?;
    Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

/// The signature of a JWT's signing input.
fn sign_input(algorithm: JwtAlgorithm, input: &[u8], s: &Jwt, root: &Path) -> Result<Vec<u8>> {
    let secret = || -> Result<Vec<u8>> {
        if s.secret.is_empty() {
            return Err(Error::Invalid(format!(
                "JWT with {} needs a secret",
                algorithm_name(algorithm)
            )));
        }
        if !s.secret_base64 {
            return Ok(s.secret.as_bytes().to_vec());
        }
        let text = s.secret.trim();
        STANDARD
            .decode(text)
            .or_else(|_| URL_SAFE.decode(text))
            .or_else(|_| URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')))
            .map_err(|_| Error::Invalid("the JWT secret is not valid base64".into()))
    };
    match algorithm {
        JwtAlgorithm::Hs256 => Ok(hmac::<Hmac<Sha256>>(&secret()?, input)),
        JwtAlgorithm::Hs384 => Ok(hmac::<Hmac<Sha384>>(&secret()?, input)),
        JwtAlgorithm::Hs512 => Ok(hmac::<Hmac<Sha512>>(&secret()?, input)),
        other => {
            let key = keys::load(&s.private_key, &s.private_key_file, root, "JWT")?;
            keys::sign(&key, asymmetric(other), input)
        }
    }
}

fn asymmetric(algorithm: JwtAlgorithm) -> Scheme {
    match algorithm {
        JwtAlgorithm::Rs384 => Scheme::RsaSha384,
        JwtAlgorithm::Rs512 => Scheme::RsaSha512,
        JwtAlgorithm::Ps256 => Scheme::PssSha256,
        JwtAlgorithm::Ps384 => Scheme::PssSha384,
        JwtAlgorithm::Ps512 => Scheme::PssSha512,
        JwtAlgorithm::Es256 => Scheme::EcdsaP256,
        JwtAlgorithm::Es384 => Scheme::EcdsaP384,
        JwtAlgorithm::Rs256 | JwtAlgorithm::Hs256 | JwtAlgorithm::Hs384 | JwtAlgorithm::Hs512 => {
            Scheme::RsaSha256
        }
    }
}

/// The algorithm's name, as the `alg` header writes it.
pub(super) fn algorithm_name(algorithm: JwtAlgorithm) -> &'static str {
    match algorithm {
        JwtAlgorithm::Hs256 => "HS256",
        JwtAlgorithm::Hs384 => "HS384",
        JwtAlgorithm::Hs512 => "HS512",
        JwtAlgorithm::Rs256 => "RS256",
        JwtAlgorithm::Rs384 => "RS384",
        JwtAlgorithm::Rs512 => "RS512",
        JwtAlgorithm::Ps256 => "PS256",
        JwtAlgorithm::Ps384 => "PS384",
        JwtAlgorithm::Ps512 => "PS512",
        JwtAlgorithm::Es256 => "ES256",
        JwtAlgorithm::Es384 => "ES384",
    }
}

/// Parses JSON text that must hold an object; empty text is an empty one.
fn json_object(text: &str, what: &str) -> Result<Map<String, Value>> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(Error::Invalid(format!("{what} must be a JSON object"))),
        Err(e) => Err(Error::Invalid(format!("{what} is not valid JSON: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::super::keys::tests::{P256_PKCS8, P384_SEC1, RSA_PKCS8, verify};
    use super::*;
    use crate::http::{OutgoingBody, find_header};

    fn get() -> Outgoing {
        Outgoing {
            method: "GET".into(),
            url: "https://api.example.com/me?a=1".into(),
            headers: vec![],
            body: OutgoingBody::None,
        }
    }

    fn at() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }

    fn token_of(outgoing: &Outgoing) -> String {
        find_header(&outgoing.headers, "authorization")
            .unwrap()
            .rsplit(' ')
            .next()
            .unwrap()
            .to_owned()
    }

    fn part(token: &str, index: usize) -> Value {
        let text = token.split('.').nth(index).unwrap();
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(text).unwrap()).unwrap()
    }

    #[test]
    fn hs256_matches_the_well_known_example() {
        let settings = Jwt {
            secret: "your-256-bit-secret".into(),
            payload: r#"{"sub":"1234567890","name":"John Doe","iat":1516239022}"#.into(),
            ..Jwt::default()
        };
        let mut outgoing = get();
        apply(&settings, &mut outgoing, Path::new("."), at()).unwrap();
        assert_eq!(
            find_header(&outgoing.headers, "authorization"),
            Some(
                "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
            )
        );

        let base64 = Jwt {
            secret: STANDARD.encode("your-256-bit-secret"),
            secret_base64: true,
            ..settings
        };
        let mut again = get();
        apply(&base64, &mut again, Path::new("."), at()).unwrap();
        assert_eq!(again.headers, outgoing.headers);
    }

    #[test]
    fn helpers_add_iat_and_exp_and_the_header_merges() {
        let settings = Jwt {
            algorithm: JwtAlgorithm::Hs512,
            secret: "s".into(),
            header: r#"{"kid": "k1", "alg": "none"}"#.into(),
            payload: r#"{"sub": "ada"}"#.into(),
            issued_at: true,
            expires_in: Some(300),
            location: ApiKeyLocation::Query,
            ..Jwt::default()
        };
        let mut outgoing = get();
        apply(&settings, &mut outgoing, Path::new("."), at()).unwrap();
        let url = url::Url::parse(&outgoing.url).unwrap();
        let token = url
            .query_pairs()
            .find(|(name, _)| name == "token")
            .unwrap()
            .1
            .into_owned();
        assert_eq!(
            part(&token, 0),
            serde_json::json!({"alg": "HS512", "typ": "JWT", "kid": "k1"})
        );
        assert_eq!(
            part(&token, 1),
            serde_json::json!({"sub": "ada", "iat": 1_700_000_000, "exp": 1_700_000_300})
        );
        assert!(
            outgoing
                .url
                .starts_with("https://api.example.com/me?a=1&token=")
        );
    }

    #[test]
    fn asymmetric_algorithms_sign_with_the_private_key() {
        for (algorithm, pem, scheme) in [
            (JwtAlgorithm::Rs256, RSA_PKCS8, Scheme::RsaSha256),
            (JwtAlgorithm::Ps256, RSA_PKCS8, Scheme::PssSha256),
            (JwtAlgorithm::Es256, P256_PKCS8, Scheme::EcdsaP256),
            (JwtAlgorithm::Es384, P384_SEC1, Scheme::EcdsaP384),
        ] {
            let settings = Jwt {
                algorithm,
                private_key: pem.into(),
                payload: r#"{"sub": "ada"}"#.into(),
                prefix: "JWT".into(),
                ..Jwt::default()
            };
            let mut outgoing = get();
            apply(&settings, &mut outgoing, Path::new("."), at()).unwrap();
            assert!(
                find_header(&outgoing.headers, "authorization")
                    .unwrap()
                    .starts_with("JWT ey")
            );
            let token = token_of(&outgoing);
            let (input, signature) = token.rsplit_once('.').unwrap();
            let key = keys::parse(pem).unwrap();
            verify(
                &key,
                scheme,
                input.as_bytes(),
                &URL_SAFE_NO_PAD.decode(signature).unwrap(),
            );
            assert_eq!(part(&token, 0)["alg"], algorithm_name(algorithm));
        }
    }

    #[test]
    fn keys_can_come_from_a_workspace_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("signing.pem"), RSA_PKCS8).unwrap();
        let settings = Jwt {
            algorithm: JwtAlgorithm::Rs512,
            private_key_file: "signing.pem".into(),
            ..Jwt::default()
        };
        let mut outgoing = get();
        apply(&settings, &mut outgoing, dir.path(), at()).unwrap();
        let outside = Jwt {
            private_key_file: "../signing.pem".into(),
            ..settings
        };
        let error = apply(&outside, &mut get(), dir.path(), at()).unwrap_err();
        assert!(
            error.to_string().contains("outside the workspace"),
            "{error}"
        );
    }

    #[test]
    fn mistakes_are_explained() {
        let no_secret = Jwt::default();
        let error = apply(&no_secret, &mut get(), Path::new("."), at()).unwrap_err();
        assert!(error.to_string().contains("needs a secret"), "{error}");
        let bad_json = Jwt {
            secret: "s".into(),
            payload: "{oops".into(),
            ..Jwt::default()
        };
        let error = apply(&bad_json, &mut get(), Path::new("."), at()).unwrap_err();
        assert!(
            error.to_string().contains("payload is not valid JSON"),
            "{error}"
        );
        let wrong_key = Jwt {
            algorithm: JwtAlgorithm::Es256,
            private_key: RSA_PKCS8.into(),
            ..Jwt::default()
        };
        let error = apply(&wrong_key, &mut get(), Path::new("."), at()).unwrap_err();
        assert!(error.to_string().contains("P-256"), "{error}");
    }

    #[test]
    fn asap_tokens_carry_the_required_claims() {
        let settings = Asap {
            issuer: "billing".into(),
            audience: "ledger, audit".into(),
            key_id: "billing/key1".into(),
            private_key: RSA_PKCS8.into(),
            claims: r#"{"scope": "read"}"#.into(),
            expires_in: Some(60),
            ..Asap::default()
        };
        let mut outgoing = get();
        apply_asap(&settings, &mut outgoing, Path::new("."), at()).unwrap();
        let token = token_of(&outgoing);
        assert_eq!(
            part(&token, 0),
            serde_json::json!({"alg": "RS256", "typ": "JWT", "kid": "billing/key1"})
        );
        let claims = part(&token, 1);
        assert_eq!(claims["iss"], "billing");
        assert_eq!(claims["aud"], serde_json::json!(["ledger", "audit"]));
        assert_eq!(claims["scope"], "read");
        assert_eq!(claims["exp"], 1_700_000_060);
        assert_eq!(claims["jti"].as_str().unwrap().len(), 36);
        assert!(claims.get("sub").is_none());

        let error = apply_asap(&Asap::default(), &mut get(), Path::new("."), at()).unwrap_err();
        assert!(error.to_string().contains("key ID"), "{error}");
        let hmac = Asap {
            algorithm: Some(JwtAlgorithm::Hs256),
            ..settings
        };
        let error = apply_asap(&hmac, &mut get(), Path::new("."), at()).unwrap_err();
        assert!(error.to_string().contains("private key"), "{error}");
    }
}

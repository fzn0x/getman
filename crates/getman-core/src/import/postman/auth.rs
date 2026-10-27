//! Postman's signed and token-based authentication types: OAuth 2.0 with
//! every grant, OAuth 1.0, AWS Signature, JWT, Hawk, Akamai EdgeGrid and
//! Atlassian ASAP.

use serde_json::{Map, Value};

use super::text;
use crate::model::{
    ApiKeyLocation, Asap, Auth, AwsSigV4, ClientAuth, EdgeGrid, Hawk, HawkAlgorithm, Jwt,
    JwtAlgorithm, KeyValue, OAuth1, OAuth1Location, OAuth1Method, OAuth2, OAuth2Grant, Pkce,
};

/// An auth type's parameters by name.
pub(super) type Params = Map<String, Value>;

fn get(p: &Params, key: &str) -> String {
    p.get(key).and_then(text).unwrap_or_default()
}

fn flag(p: &Params, key: &str) -> bool {
    matches!(p.get(key), Some(Value::Bool(true))) || get(p, key) == "true"
}

/// Postman writes defaults out; Getman leaves them unset.
fn unless(value: String, default: &str) -> String {
    if value == default {
        String::new()
    } else {
        value
    }
}

/// The OAuth 2.0 settings, or `None` for a grant Getman does not know.
pub(super) fn oauth2(p: &Params, name: &str, warnings: &mut Vec<String>) -> Option<Auth> {
    let grant_type = get(p, "grant_type");
    let grant = match grant_type.as_str() {
        "client_credentials" => OAuth2Grant::ClientCredentials,
        "password_credentials" => OAuth2Grant::Password,
        // Postman's default grant.
        "authorization_code" | "authorization_code_with_pkce" | "" => {
            OAuth2Grant::AuthorizationCode
        }
        "implicit" => OAuth2Grant::Implicit,
        _ => return None,
    };
    let pkce = match (grant_type.as_str(), get(p, "challengeAlgorithm").as_str()) {
        ("authorization_code_with_pkce", "plain") => Pkce::Plain,
        ("authorization_code_with_pkce", _) => Pkce::S256,
        _ => Pkce::Off,
    };
    let callback = get(p, "redirect_uri");
    let callback_url = if callback.is_empty() || is_loopback(&callback) {
        callback
    } else {
        if grant.uses_browser() {
            warnings.push(format!(
                "{name}: Getman receives the OAuth 2.0 callback on this machine, not at {callback}; it listens on http://127.0.0.1:<free port>/callback, or set a loopback callback URL your provider allows"
            ));
        }
        String::new()
    };
    if !get(p, "accessToken").is_empty() {
        warnings.push(format!(
            "{name}: the saved OAuth 2.0 access token was not imported; get a new one under Auth"
        ));
    }
    Some(Auth::Oauth2(OAuth2 {
        grant,
        auth_url: get(p, "authUrl"),
        token_url: get(p, "accessTokenUrl"),
        client_id: get(p, "clientId"),
        client_secret: get(p, "clientSecret"),
        callback_url,
        pkce,
        scope: get(p, "scope"),
        audience: get(p, "audience"),
        resource: get(p, "resource"),
        username: get(p, "username"),
        password: get(p, "password"),
        client_auth: if get(p, "client_authentication") == "body" {
            ClientAuth::Body
        } else {
            ClientAuth::Basic
        },
        auth_params: params(p.get("authRequestParams")),
        token_params: params(p.get("tokenRequestParams")),
        prefix: unless(get(p, "headerPrefix"), "Bearer"),
        token_in: if get(p, "addTokenTo") == "queryParams" {
            ApiKeyLocation::Query
        } else {
            ApiKeyLocation::Header
        },
    }))
}

/// The other types, or `None` for a type Getman does not know.
pub(super) fn other(
    kind: &str,
    p: &Params,
    name: &str,
    warnings: &mut Vec<String>,
) -> Option<Auth> {
    Some(match kind {
        "awsv4" => Auth::AwsSigV4(AwsSigV4 {
            access_key: get(p, "accessKey"),
            secret_key: get(p, "secretKey"),
            session_token: get(p, "sessionToken"),
            region: get(p, "region"),
            service: get(p, "service"),
            location: if flag(p, "addAuthDataToQuery") {
                ApiKeyLocation::Query
            } else {
                ApiKeyLocation::Header
            },
            ..AwsSigV4::default()
        }),
        "oauth1" => Auth::Oauth1(OAuth1 {
            signature_method: match get(p, "signatureMethod").to_ascii_uppercase().as_str() {
                "HMAC-SHA1" | "" => OAuth1Method::HmacSha1,
                "HMAC-SHA256" => OAuth1Method::HmacSha256,
                "HMAC-SHA512" => OAuth1Method::HmacSha512,
                "PLAINTEXT" => OAuth1Method::Plaintext,
                "RSA-SHA1" => OAuth1Method::RsaSha1,
                "RSA-SHA256" => OAuth1Method::RsaSha256,
                "RSA-SHA512" => OAuth1Method::RsaSha512,
                other => {
                    warnings.push(format!(
                        "{name}: OAuth 1.0 with {other} is not supported; it now signs with HMAC-SHA1"
                    ));
                    OAuth1Method::HmacSha1
                }
            },
            consumer_key: get(p, "consumerKey"),
            consumer_secret: get(p, "consumerSecret"),
            token: get(p, "token"),
            token_secret: get(p, "tokenSecret"),
            private_key: get(p, "privateKey"),
            realm: get(p, "realm"),
            callback: get(p, "callback"),
            verifier: get(p, "verifier"),
            version: unless(get(p, "version"), "1.0"),
            timestamp: get(p, "timestamp"),
            nonce: get(p, "nonce"),
            body_hash: flag(p, "includeBodyHash"),
            location: if p.contains_key("addParamsToHeader") && !flag(p, "addParamsToHeader") {
                OAuth1Location::Query
            } else {
                OAuth1Location::Header
            },
            ..OAuth1::default()
        }),
        "jwt" => Auth::Jwt(Jwt {
            algorithm: algorithm(&get(p, "algorithm"), name, warnings).unwrap_or_default(),
            secret: get(p, "secret"),
            secret_base64: flag(p, "isSecretBase64Encoded"),
            private_key: get(p, "privateKey"),
            header: unless(get(p, "header"), "{}"),
            payload: unless(get(p, "payload"), "{}"),
            location: if get(p, "addTokenTo") == "queryParam" {
                ApiKeyLocation::Query
            } else {
                ApiKeyLocation::Header
            },
            prefix: unless(get(p, "headerPrefix"), "Bearer"),
            param: unless(get(p, "queryParamKey"), "token"),
            ..Jwt::default()
        }),
        "hawk" => Auth::Hawk(Hawk {
            id: get(p, "authId"),
            key: get(p, "authKey"),
            algorithm: if get(p, "algorithm").eq_ignore_ascii_case("sha1") {
                HawkAlgorithm::Sha1
            } else {
                HawkAlgorithm::Sha256
            },
            ext: get(p, "extraData"),
            app: get(p, "app"),
            dlg: get(p, "delegation"),
            payload_hash: flag(p, "includePayloadHash"),
        }),
        "edgegrid" => Auth::Edgegrid(EdgeGrid {
            client_token: get(p, "clientToken"),
            client_secret: get(p, "clientSecret"),
            access_token: get(p, "accessToken"),
            base_url: get(p, "baseURL"),
            headers_to_sign: get(p, "headersToSign")
                .split(',')
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .map(str::to_owned)
                .collect(),
            max_body: get(p, "maxBody").parse().ok(),
        }),
        "asap" => Auth::Asap(Asap {
            algorithm: algorithm(&get(p, "alg"), name, warnings)
                .filter(|a| *a != JwtAlgorithm::Rs256),
            issuer: get(p, "iss"),
            subject: get(p, "sub"),
            audience: get(p, "aud"),
            key_id: get(p, "kid"),
            private_key: get(p, "privateKey"),
            claims: unless(get(p, "claims"), "{}"),
            expires_in: seconds(&get(p, "exp")),
            ..Asap::default()
        }),
        _ => return None,
    })
}

/// A JWT algorithm by name; `None` when empty or unsupported.
fn algorithm(name: &str, request: &str, warnings: &mut Vec<String>) -> Option<JwtAlgorithm> {
    if name.is_empty() {
        return None;
    }
    let parsed = serde_json::from_value(Value::String(name.to_ascii_uppercase())).ok();
    if parsed.is_none() {
        warnings.push(format!(
            "{request}: the {name} signing algorithm is not supported; choose another under Auth"
        ));
    }
    parsed
}

/// A lifetime such as `3600`, `90s`, `15m` or `1h`, in seconds.
fn seconds(text: &str) -> Option<u64> {
    let text = text.trim();
    let (number, unit) = match text.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((at, _)) => text.split_at(at),
        None => (text, ""),
    };
    let number: u64 = number.parse().ok()?;
    match unit.trim() {
        "" | "s" => Some(number),
        "m" => Some(number * 60),
        "h" => Some(number * 3600),
        _ => None,
    }
}

/// Extra parameters: `[{ "key", "value", "enabled" }]`.
fn params(value: Option<&Value>) -> Vec<KeyValue> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let name = text(&item["key"]).filter(|k| !k.is_empty())?;
            Some(KeyValue {
                enabled: item["enabled"].as_bool() != Some(false),
                ..KeyValue::new(name, text(&item["value"]).unwrap_or_default())
            })
        })
        .collect()
}

/// Whether a URL is `http` on this machine, where Getman can listen.
fn is_loopback(text: &str) -> bool {
    url::Url::parse(text).is_ok_and(|url| {
        url.scheme() == "http"
            && match url.host() {
                Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                None => false,
            }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetimes_and_loopback_urls() {
        assert_eq!(seconds("3600"), Some(3600));
        assert_eq!(seconds("15m"), Some(900));
        assert_eq!(seconds("1h"), Some(3600));
        assert_eq!(seconds("soon"), None);
        assert!(is_loopback("http://localhost:5000/cb"));
        assert!(is_loopback("http://127.0.0.1/cb"));
        assert!(!is_loopback("https://oauth.pstmn.io/v1/callback"));
    }
}

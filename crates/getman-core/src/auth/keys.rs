//! Private keys in PEM form, and the signatures made with them: RSA
//! (PKCS#1 v1.5 and PSS) and ECDSA on P-256 and P-384.

use std::path::Path;

use rsa::RsaPrivateKey;
use rsa::pkcs1::DecodeRsaPrivateKey as _;
use rsa::pkcs8::DecodePrivateKey as _;
use rsa::signature::{SignatureEncoding as _, Signer as _};
use sha1::Sha1;
use sha2::{Sha256, Sha384, Sha512};

use crate::error::{Error, Result};
use crate::prepare::resolve_file;

/// A private key read from PEM.
#[derive(Debug)]
pub(super) enum PrivateKey {
    /// An RSA key.
    Rsa(Box<RsaPrivateKey>),
    /// A NIST P-256 key.
    P256(p256::SecretKey),
    /// A NIST P-384 key.
    P384(p384::SecretKey),
}

impl PrivateKey {
    fn kind(&self) -> &'static str {
        match self {
            Self::Rsa(_) => "an RSA key",
            Self::P256(_) => "a P-256 key",
            Self::P384(_) => "a P-384 key",
        }
    }
}

/// A way of signing with a private key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scheme {
    /// RSA PKCS#1 v1.5 with SHA-1, for OAuth 1.0a's RSA-SHA1 only.
    RsaSha1,
    /// RSA PKCS#1 v1.5 with SHA-256.
    RsaSha256,
    /// RSA PKCS#1 v1.5 with SHA-384.
    RsaSha384,
    /// RSA PKCS#1 v1.5 with SHA-512.
    RsaSha512,
    /// RSA-PSS with SHA-256 and a salt as long as the hash.
    PssSha256,
    /// RSA-PSS with SHA-384.
    PssSha384,
    /// RSA-PSS with SHA-512.
    PssSha512,
    /// ECDSA on P-256 with SHA-256, as the fixed-size `r || s`.
    EcdsaP256,
    /// ECDSA on P-384 with SHA-384, as the fixed-size `r || s`.
    EcdsaP384,
}

/// Reads the private key a request names: the file under `root` when one is
/// given, else the PEM text. `what` names the authentication, for errors.
pub(super) fn load(text: &str, file: &str, root: &Path, what: &str) -> Result<PrivateKey> {
    let pem = if file.trim().is_empty() {
        text.to_owned()
    } else {
        let path = resolve_file(root, file)?;
        std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?
    };
    if pem.trim().is_empty() {
        return Err(Error::Invalid(format!(
            "{what} needs a private key: put the PEM text in a variable from a .local.yaml file, or name a key file in the workspace"
        )));
    }
    parse(&pem).map_err(|e| Error::Invalid(format!("{what}: {e}")))
}

/// Parses a PEM private key: PKCS#8, PKCS#1 (`RSA PRIVATE KEY`) or SEC1
/// (`EC PRIVATE KEY`).
pub(super) fn parse(pem: &str) -> Result<PrivateKey, String> {
    // Keys kept in environment variables often have `\n` for line breaks.
    let pem = if pem.contains('\n') {
        pem.trim().to_owned()
    } else {
        pem.trim().replace("\\n", "\n")
    };
    let label = pem
        .lines()
        .next()
        .and_then(|line| line.trim().strip_prefix("-----BEGIN "))
        .and_then(|rest| rest.strip_suffix("-----"))
        .unwrap_or_default();
    let unreadable = |e: &dyn std::fmt::Display| format!("the private key could not be read: {e}");
    match label {
        "RSA PRIVATE KEY" => RsaPrivateKey::from_pkcs1_pem(&pem)
            .map(|key| PrivateKey::Rsa(Box::new(key)))
            .map_err(|e| unreadable(&e)),
        "EC PRIVATE KEY" => p256::SecretKey::from_sec1_pem(&pem)
            .map(PrivateKey::P256)
            .or_else(|_| p384::SecretKey::from_sec1_pem(&pem).map(PrivateKey::P384))
            .map_err(|_| {
                "the EC private key is not on a supported curve; use P-256 or P-384".to_owned()
            }),
        "PRIVATE KEY" => RsaPrivateKey::from_pkcs8_pem(&pem)
            .map(|key| PrivateKey::Rsa(Box::new(key)))
            .or_else(|_| p256::SecretKey::from_pkcs8_pem(&pem).map(PrivateKey::P256))
            .or_else(|_| p384::SecretKey::from_pkcs8_pem(&pem).map(PrivateKey::P384))
            .map_err(|_| {
                "the private key is not an RSA, P-256 or P-384 key Getman can read".to_owned()
            }),
        "ENCRYPTED PRIVATE KEY" => Err(
            "the private key is encrypted; decrypt it first, for example with `openssl pkey -in key.pem -out plain.pem`".into(),
        ),
        _ => Err(
            "the private key is not in PEM form; it should start with -----BEGIN PRIVATE KEY-----"
                .into(),
        ),
    }
}

/// Signs `message` with `key`.
pub(super) fn sign(key: &PrivateKey, scheme: Scheme, message: &[u8]) -> Result<Vec<u8>> {
    use rsa::pkcs1v15::SigningKey as Pkcs1;
    use rsa::pss::SigningKey as Pss;

    let failed = |e: rsa::signature::Error| Error::Invalid(format!("Could not sign: {e}"));
    let wrong = |needs: &str| {
        Error::Invalid(format!(
            "this algorithm needs {needs}, but the private key is {}",
            key.kind()
        ))
    };
    match (scheme, key) {
        (Scheme::EcdsaP256, PrivateKey::P256(secret)) => {
            let signing = p256::ecdsa::SigningKey::from(secret);
            let signature: p256::ecdsa::Signature = signing.try_sign(message).map_err(failed)?;
            Ok(signature.to_vec())
        }
        (Scheme::EcdsaP384, PrivateKey::P384(secret)) => {
            let signing = p384::ecdsa::SigningKey::from(secret);
            let signature: p384::ecdsa::Signature = signing.try_sign(message).map_err(failed)?;
            Ok(signature.to_vec())
        }
        (Scheme::EcdsaP256, _) => Err(wrong("a P-256 key")),
        (Scheme::EcdsaP384, _) => Err(wrong("a P-384 key")),
        (_, PrivateKey::Rsa(rsa)) => {
            let rsa = rsa.as_ref().clone();
            let signature = match scheme {
                Scheme::RsaSha1 => Pkcs1::<Sha1>::new(rsa)
                    .try_sign(message)
                    .map(|s| s.to_vec()),
                Scheme::RsaSha256 => Pkcs1::<Sha256>::new(rsa)
                    .try_sign(message)
                    .map(|s| s.to_vec()),
                Scheme::RsaSha384 => Pkcs1::<Sha384>::new(rsa)
                    .try_sign(message)
                    .map(|s| s.to_vec()),
                Scheme::RsaSha512 => Pkcs1::<Sha512>::new(rsa)
                    .try_sign(message)
                    .map(|s| s.to_vec()),
                Scheme::PssSha256 => Pss::<Sha256>::new(rsa)
                    .try_sign(message)
                    .map(|s| s.to_vec()),
                Scheme::PssSha384 => Pss::<Sha384>::new(rsa)
                    .try_sign(message)
                    .map(|s| s.to_vec()),
                Scheme::PssSha512 => Pss::<Sha512>::new(rsa)
                    .try_sign(message)
                    .map(|s| s.to_vec()),
                Scheme::EcdsaP256 | Scheme::EcdsaP384 => unreachable!("handled above"),
            };
            signature.map_err(failed)
        }
        (_, _) => Err(wrong("an RSA key")),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use rsa::signature::Verifier as _;

    use super::*;

    pub(crate) const RSA_PKCS1: &str = include_str!("../../tests/fixtures/keys/rsa-pkcs1.pem");
    pub(crate) const RSA_PKCS8: &str = include_str!("../../tests/fixtures/keys/rsa-pkcs8.pem");
    pub(crate) const P256_SEC1: &str = include_str!("../../tests/fixtures/keys/p256-sec1.pem");
    pub(crate) const P256_PKCS8: &str = include_str!("../../tests/fixtures/keys/p256-pkcs8.pem");
    pub(crate) const P384_SEC1: &str = include_str!("../../tests/fixtures/keys/p384-sec1.pem");

    /// Checks a signature with the public half of `key`.
    pub(crate) fn verify(key: &PrivateKey, scheme: Scheme, message: &[u8], signature: &[u8]) {
        match key {
            PrivateKey::Rsa(rsa) => {
                let public = rsa.to_public_key();
                let ok = match scheme {
                    Scheme::RsaSha1 => rsa::pkcs1v15::VerifyingKey::<Sha1>::new(public)
                        .verify(message, &signature.try_into().unwrap()),
                    Scheme::RsaSha256 => rsa::pkcs1v15::VerifyingKey::<Sha256>::new(public)
                        .verify(message, &signature.try_into().unwrap()),
                    Scheme::RsaSha512 => rsa::pkcs1v15::VerifyingKey::<Sha512>::new(public)
                        .verify(message, &signature.try_into().unwrap()),
                    Scheme::PssSha256 => rsa::pss::VerifyingKey::<Sha256>::new(public)
                        .verify(message, &signature.try_into().unwrap()),
                    Scheme::PssSha384 => rsa::pss::VerifyingKey::<Sha384>::new(public)
                        .verify(message, &signature.try_into().unwrap()),
                    other => panic!("no verifier for {other:?} in the tests"),
                };
                ok.unwrap();
            }
            PrivateKey::P256(secret) => {
                let verifying =
                    p256::ecdsa::VerifyingKey::from(&p256::ecdsa::SigningKey::from(secret));
                let signature = p256::ecdsa::Signature::from_slice(signature).unwrap();
                verifying.verify(message, &signature).unwrap();
            }
            PrivateKey::P384(secret) => {
                let verifying =
                    p384::ecdsa::VerifyingKey::from(&p384::ecdsa::SigningKey::from(secret));
                let signature = p384::ecdsa::Signature::from_slice(signature).unwrap();
                verifying.verify(message, &signature).unwrap();
            }
        }
    }

    #[test]
    fn every_supported_encoding_reads() {
        assert!(matches!(parse(RSA_PKCS1), Ok(PrivateKey::Rsa(_))));
        assert!(matches!(parse(RSA_PKCS8), Ok(PrivateKey::Rsa(_))));
        assert!(matches!(parse(P256_SEC1), Ok(PrivateKey::P256(_))));
        assert!(matches!(parse(P256_PKCS8), Ok(PrivateKey::P256(_))));
        assert!(matches!(parse(P384_SEC1), Ok(PrivateKey::P384(_))));
        // As kept in an environment variable, on one line.
        let escaped = RSA_PKCS8.trim().replace('\n', "\\n");
        assert!(matches!(parse(&escaped), Ok(PrivateKey::Rsa(_))));
    }

    #[test]
    fn unreadable_keys_say_why() {
        assert!(parse("not a key").unwrap_err().contains("PEM"));
        let encrypted =
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----";
        assert!(parse(encrypted).unwrap_err().contains("encrypted"));
        let error = load("", "", Path::new("."), "JWT").unwrap_err();
        assert!(error.to_string().contains(".local.yaml"), "{error}");
    }

    #[test]
    fn signatures_verify_and_keys_must_fit() {
        let rsa = parse(RSA_PKCS1).unwrap();
        for scheme in [
            Scheme::RsaSha1,
            Scheme::RsaSha256,
            Scheme::RsaSha512,
            Scheme::PssSha256,
            Scheme::PssSha384,
        ] {
            let signature = sign(&rsa, scheme, b"message").unwrap();
            assert_eq!(signature.len(), 256);
            verify(&rsa, scheme, b"message", &signature);
        }
        let p256 = parse(P256_SEC1).unwrap();
        let signature = sign(&p256, Scheme::EcdsaP256, b"message").unwrap();
        assert_eq!(signature.len(), 64);
        verify(&p256, Scheme::EcdsaP256, b"message", &signature);
        let p384 = parse(P384_SEC1).unwrap();
        let signature = sign(&p384, Scheme::EcdsaP384, b"message").unwrap();
        assert_eq!(signature.len(), 96);
        verify(&p384, Scheme::EcdsaP384, b"message", &signature);

        let error = sign(&p256, Scheme::RsaSha256, b"m").unwrap_err();
        assert!(error.to_string().contains("needs an RSA key"), "{error}");
        let error = sign(&rsa, Scheme::EcdsaP384, b"m").unwrap_err();
        assert!(error.to_string().contains("P-384"), "{error}");
    }
}

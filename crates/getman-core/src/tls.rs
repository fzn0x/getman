//! TLS beyond the defaults: client certificates, extra certificate
//! authorities, and what a server's certificate says.

use std::fmt;
use std::net::IpAddr;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use x509_parser::extensions::GeneralName;

use crate::error::{Error, Result};

/// A client certificate with its private key, as PEM, ready to present to
/// servers that ask for one.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ClientIdentity {
    pem: Vec<u8>,
}

impl fmt::Debug for ClientIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The private key stays out of logs.
        f.write_str("ClientIdentity { .. }")
    }
}

impl ClientIdentity {
    /// From PEM text holding the certificate, any intermediates, and an
    /// unencrypted private key. `name` names the files in errors.
    pub fn from_pem(pem: Vec<u8>, name: &str) -> Result<Self> {
        let text = String::from_utf8_lossy(&pem);
        if text.contains("ENCRYPTED PRIVATE KEY") || text.contains("Proc-Type: 4,ENCRYPTED") {
            return Err(Error::Invalid(format!(
                "the private key in {name} is encrypted; use an unencrypted PEM key, or a PKCS#12 file with a passphrase"
            )));
        }
        if !text.contains("-----BEGIN CERTIFICATE-----") {
            return Err(Error::Invalid(format!(
                "{name} holds no PEM certificate (-----BEGIN CERTIFICATE-----)"
            )));
        }
        if !text.contains("PRIVATE KEY-----") {
            return Err(Error::Invalid(format!(
                "{name} holds no PEM private key (-----BEGIN PRIVATE KEY-----)"
            )));
        }
        reqwest::Identity::from_pem(&pem).map_err(|e| {
            Error::Invalid(format!(
                "{name} is not a usable certificate and key: {}",
                cause(&e)
            ))
        })?;
        Ok(Self { pem })
    }

    /// From a PKCS#12 bundle (`.p12` or `.pfx`) and its passphrase. `name`
    /// names the file in errors.
    pub fn from_pkcs12(der: &[u8], passphrase: &str, name: &str) -> Result<Self> {
        let store = p12_keystore::KeyStore::from_pkcs12(
            der,
            passphrase,
            p12_keystore::Pkcs12ImportPolicy::Relaxed,
        )
        .map_err(|e| {
            Error::Invalid(format!(
                "{name} could not be opened; is the passphrase right? ({e})"
            ))
        })?;
        let Some((_, chain)) = store.private_key_chain() else {
            return Err(Error::Invalid(format!("{name} holds no private key")));
        };
        if chain.certs().is_empty() {
            return Err(Error::Invalid(format!(
                "{name} holds a private key but no certificate"
            )));
        }
        let mut pem = String::new();
        for cert in chain.certs() {
            pem.push_str(&pem_block("CERTIFICATE", cert.as_der()));
        }
        pem.push_str(&pem_block("PRIVATE KEY", chain.key().as_der()));
        Self::from_pem(pem.into_bytes(), name)
    }

    /// The certificate and key as PEM.
    pub fn pem(&self) -> &[u8] {
        &self.pem
    }
}

/// Checks that PEM text holds at least one certificate, for trusting it
/// as a certificate authority. `name` names the file in errors.
pub fn check_ca_pem(pem: &[u8], name: &str) -> Result<()> {
    let certs = reqwest::Certificate::from_pem_bundle(pem).map_err(|e| {
        Error::Invalid(format!(
            "{name} is not a PEM certificate file: {}",
            cause(&e)
        ))
    })?;
    if certs.is_empty() {
        return Err(Error::Invalid(format!(
            "{name} holds no PEM certificate (-----BEGIN CERTIFICATE-----)"
        )));
    }
    Ok(())
}

fn pem_block(label: &str, der: &[u8]) -> String {
    let encoded = BASE64.encode(der);
    let mut block = format!("-----BEGIN {label}-----\n");
    for line in encoded.as_bytes().chunks(64) {
        block.push_str(&String::from_utf8_lossy(line));
        block.push('\n');
    }
    block.push_str(&format!("-----END {label}-----\n"));
    block
}

fn cause(error: &reqwest::Error) -> String {
    std::error::Error::source(error).map_or_else(|| error.to_string(), ToString::to_string)
}

/// The TLS connection a response came over.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsDetails {
    /// Protocol version, such as `TLSv1.3`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The certificate the server presented.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<CertificateInfo>,
}

/// What a certificate says about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificateInfo {
    /// Who it was issued to, such as `CN=api.example.com`.
    pub subject: String,
    /// Who issued it.
    pub issuer: String,
    /// Host names and addresses it is valid for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// Valid from, as milliseconds since the Unix epoch.
    pub not_before: i64,
    /// Valid until, as milliseconds since the Unix epoch.
    pub not_after: i64,
}

impl CertificateInfo {
    /// Reads a DER certificate. `None` if it cannot be parsed.
    pub fn from_der(der: &[u8]) -> Option<Self> {
        let (_, cert) = x509_parser::parse_x509_certificate(der).ok()?;
        let names = cert
            .subject_alternative_name()
            .ok()
            .flatten()
            .map(|extension| {
                extension
                    .value
                    .general_names
                    .iter()
                    .filter_map(|name| match name {
                        GeneralName::DNSName(dns) => Some((*dns).to_owned()),
                        GeneralName::IPAddress(bytes) => ip_text(bytes),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let validity = cert.validity();
        Some(Self {
            subject: cert.subject().to_string(),
            issuer: cert.issuer().to_string(),
            names,
            not_before: validity.not_before.timestamp().saturating_mul(1000),
            not_after: validity.not_after.timestamp().saturating_mul(1000),
        })
    }
}

fn ip_text(bytes: &[u8]) -> Option<String> {
    let ip = match bytes.len() {
        4 => IpAddr::from(<[u8; 4]>::try_from(bytes).ok()?),
        16 => IpAddr::from(<[u8; 16]>::try_from(bytes).ok()?),
        _ => return None,
    };
    Some(ip.to_string())
}

/// Names a TLS protocol version the way OpenSSL does, such as `TLSv1.3`.
pub(crate) fn version_name(version: reqwest::tls::Version) -> Option<&'static str> {
    use reqwest::tls::Version;
    [
        (Version::TLS_1_0, "TLSv1"),
        (Version::TLS_1_1, "TLSv1.1"),
        (Version::TLS_1_2, "TLSv1.2"),
        (Version::TLS_1_3, "TLSv1.3"),
    ]
    .into_iter()
    .find(|(known, _)| *known == version)
    .map(|(_, name)| name)
}

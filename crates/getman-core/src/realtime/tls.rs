//! TLS for the connections Getman opens itself rather than through
//! reqwest: WebSocket and Socket.IO. Certificates are checked the way the
//! HTTP client checks them, with the operating system's verifier and the
//! request's extra certificate authorities, unless a request turns
//! checking off, and the request's client certificate is presented.

use std::sync::{Arc, OnceLock};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};

use crate::http::Transport;
use crate::tls::ClientIdentity;

/// The client configuration for `transport`: certificates checked or
/// not, its extra certificate authorities trusted, and its client
/// certificate presented when `identity` is set. The common ones, with
/// neither authorities nor a certificate, are built once.
pub(crate) fn config(transport: &Transport, identity: bool) -> Result<Arc<ClientConfig>, String> {
    static CHECKED: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();
    static UNCHECKED: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();
    let identity = transport.client_identity.as_ref().filter(|_| identity);
    if !transport.ca_certs.is_empty() || identity.is_some() {
        return build(transport.verify_tls, &transport.ca_certs, identity);
    }
    let slot = if transport.verify_tls {
        &CHECKED
    } else {
        &UNCHECKED
    };
    slot.get_or_init(|| build(transport.verify_tls, &[], None))
        .clone()
}

fn build(
    verify: bool,
    ca_certs: &[Vec<u8>],
    identity: Option<&ClientIdentity>,
) -> Result<Arc<ClientConfig>, String> {
    // WebSocket upgrades an HTTP/1.1 connection.
    crate::tls::client_config(verify, ca_certs, identity, &[b"http/1.1"])
        .map(Arc::new)
        .map_err(|e| e.to_string())
}

/// Accepts any certificate, for requests that turned checking off. The
/// handshake signatures are still checked, so the connection is encrypted
/// to whoever holds the certificate's key; only who that is goes unchecked.
#[derive(Debug)]
pub(crate) struct AcceptAnyCertificate {
    algorithms: WebPkiSupportedAlgorithms,
}

impl AcceptAnyCertificate {
    pub(crate) fn new(algorithms: WebPkiSupportedAlgorithms) -> Self {
        Self { algorithms }
    }
}

impl ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

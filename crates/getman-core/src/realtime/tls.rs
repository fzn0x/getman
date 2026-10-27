//! TLS for the connections Getman opens itself rather than through
//! reqwest: WebSocket and Socket.IO. Certificates are checked the way the
//! HTTP client checks them, with the operating system's verifier, unless a
//! request turns checking off.

use std::sync::{Arc, OnceLock};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use rustls_platform_verifier::BuilderVerifierExt as _;

/// The client configuration for checked or unchecked certificates.
pub(crate) fn config(verify: bool) -> Result<Arc<ClientConfig>, String> {
    static CHECKED: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();
    static UNCHECKED: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();
    let slot = if verify { &CHECKED } else { &UNCHECKED };
    slot.get_or_init(|| build(verify)).clone()
}

fn build(verify: bool) -> Result<Arc<ClientConfig>, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("Could not set up TLS: {e}"))?;
    let mut config = if verify {
        builder
            .with_platform_verifier()
            .map_err(|e| format!("Could not load the system's certificates: {e}"))?
            .with_no_client_auth()
    } else {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate {
                algorithms: provider.signature_verification_algorithms,
            }))
            .with_no_client_auth()
    };
    // WebSocket upgrades an HTTP/1.1 connection.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Accepts any certificate, for requests that turned checking off. The
/// handshake signatures are still checked, so the connection is encrypted
/// to whoever holds the certificate's key; only who that is goes unchecked.
#[derive(Debug)]
struct AcceptAnyCertificate {
    algorithms: WebPkiSupportedAlgorithms,
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

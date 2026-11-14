//! A verifier that trusts extra certificate authorities beside the
//! system's, for Android, whose platform verifier cannot take any: it asks
//! Android's own trust manager, which reads only the system's and the
//! user's authorities.
//!
//! A chain the system turns down is checked again against the extra
//! authorities alone. Signatures are checked the system's way either way.
//! It is generic over the system's verifier so tests can run it anywhere.

use std::sync::Arc;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, RootCertStore, SignatureScheme};

#[derive(Debug)]
pub(crate) struct ExtraRoots<S> {
    system: S,
    extra: Arc<WebPkiServerVerifier>,
}

impl<S: ServerCertVerifier> ExtraRoots<S> {
    pub(crate) fn new(
        system: S,
        roots: Vec<CertificateDer<'static>>,
        provider: Arc<CryptoProvider>,
    ) -> Result<Self, rustls::Error> {
        let mut store = RootCertStore::empty();
        for root in roots {
            store.add(root)?;
        }
        let extra = WebPkiServerVerifier::builder_with_provider(Arc::new(store), provider)
            .build()
            .map_err(|e| rustls::Error::General(e.to_string()))?;
        Ok(Self { system, extra })
    }
}

impl<S: ServerCertVerifier> ServerCertVerifier for ExtraRoots<S> {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.system
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
            .or_else(|refused| {
                // The system's reason is the one worth showing when the
                // extra authorities don't know the chain either.
                self.extra
                    .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
                    .map_err(|_| refused)
            })
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.system.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.system.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.system.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests {
    use rustls::pki_types::pem::PemObject as _;

    use super::*;
    use crate::test_server::pki;

    fn provider() -> Arc<CryptoProvider> {
        Arc::new(rustls::crypto::ring::default_provider())
    }

    fn pem(text: &str) -> CertificateDer<'static> {
        CertificateDer::from_pem_slice(text.as_bytes()).unwrap()
    }

    /// Stands in for the system: it trusts one authority of its own,
    /// which signed nothing the tests present.
    fn system() -> WebPkiServerVerifier {
        let other = rcgen::generate_simple_self_signed(vec!["other.test".to_owned()]).unwrap();
        let mut store = RootCertStore::empty();
        store.add(other.cert.der().clone()).unwrap();
        let built = WebPkiServerVerifier::builder_with_provider(Arc::new(store), provider())
            .build()
            .unwrap();
        Arc::try_unwrap(built).unwrap()
    }

    fn verify(verifier: &impl ServerCertVerifier) -> Result<ServerCertVerified, rustls::Error> {
        let server = pem(&pki().server_cert_pem);
        verifier.verify_server_cert(
            &server,
            &[],
            &ServerName::try_from("localhost").unwrap(),
            &[],
            UnixTime::now(),
        )
    }

    #[test]
    fn an_extra_authority_vouches_for_a_chain_the_system_refuses() {
        let system = system();
        assert!(verify(&system).is_err());
        let ca = pem(&pki().ca_pem);
        let verifier = ExtraRoots::new(system, vec![ca], provider()).unwrap();
        assert!(verify(&verifier).is_ok());
    }

    #[test]
    fn a_chain_no_one_knows_keeps_the_systems_reason() {
        let system = system();
        let refused = verify(&system).unwrap_err();
        let unrelated = rcgen::generate_simple_self_signed(vec!["x.test".to_owned()]).unwrap();
        let verifier =
            ExtraRoots::new(system, vec![unrelated.cert.der().clone()], provider()).unwrap();
        assert_eq!(verify(&verifier).unwrap_err(), refused);
    }
}

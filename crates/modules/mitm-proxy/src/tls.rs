//! TLS termination for the MITM proxy.
//!
//! A custom [`rustls::server::ResolvesServerCert`] issues a fresh leaf
//! certificate for every requested SNI hostname (cached per host), so the
//! proxy can impersonate arbitrary targets to the connecting tool.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

use crate::ca::CertificateAuthority;

struct CertResolver {
    ca: Arc<CertificateAuthority>,
    provider: Arc<CryptoProvider>,
    cache: Mutex<HashMap<String, Arc<CertifiedKey>>>,
}

impl std::fmt::Debug for CertResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertResolver")
            .field("cache_len", &self.cache.lock().unwrap().len())
            .finish_non_exhaustive()
    }
}

impl CertResolver {
    fn new(ca: Arc<CertificateAuthority>, provider: Arc<CryptoProvider>) -> Self {
        Self {
            ca,
            provider,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn resolve_key(&self, host: &str) -> Option<Arc<CertifiedKey>> {
        if let Some(cached) = self.cache.lock().unwrap().get(host) {
            return Some(cached.clone());
        }
        let signed = self.ca.issue(host).ok()?;
        let cert_der = CertificateDer::from(signed.cert_der.clone());
        let key_der: PrivateKeyDer = PrivatePkcs8KeyDer::from(signed.key_der.clone()).into();
        let signing_key = self.provider.key_provider.load_private_key(key_der).ok()?;
        let key = Arc::new(CertifiedKey::new(vec![cert_der], signing_key));
        self.cache
            .lock()
            .unwrap()
            .insert(host.to_string(), key.clone());
        Some(key)
    }
}

impl ResolvesServerCert for CertResolver {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let host = hello.server_name()?.to_string();
        self.resolve_key(&host)
    }
}

/// Resolve the process-wide default crypto provider, installing the
/// `aws-lc-rs` provider if none has been installed yet.
pub(crate) fn default_provider() -> Result<Arc<CryptoProvider>, String> {
    if let Some(provider) = CryptoProvider::get_default() {
        return Ok(provider.clone());
    }
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    let _ = CryptoProvider::install_default(provider.clone());
    Ok(Arc::new(provider))
}

/// Build a `TlsAcceptor` that terminates client TLS using dynamically-issued
/// per-host certificates.
pub fn build_acceptor(ca: Arc<CertificateAuthority>) -> Result<TlsAcceptor, String> {
    let provider = default_provider()?;
    let mut config = ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(CertResolver::new(ca, provider)));
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(config)))
}

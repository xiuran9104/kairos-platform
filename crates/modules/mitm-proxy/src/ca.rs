//! Certificate authority for MITM TLS interception.
//!
//! Generates (or loads) a self-signed CA persisted to disk, and issues
//! per-host leaf certificates signed by that CA. Tools are configured to
//! trust the CA so their HTTPS traffic can be decrypted and inspected.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose,
};

const CA_CERT_FILE: &str = "ca-cert.pem";
const CA_KEY_FILE: &str = "ca-key.pem";

/// A signed leaf certificate ready for use by `rustls`.
///
/// `cert_der` is the DER-encoded certificate chain (leaf only), `key_der` is
/// the PKCS#8 DER-encoded private key.
#[derive(Clone)]
pub struct SignedCert {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

/// A loaded/generated certificate authority.
pub struct CertificateAuthority {
    issuer_cert: Certificate,
    key: KeyPair,
    cert_pem: String,
}

impl std::fmt::Debug for CertificateAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertificateAuthority")
            .field("issuer", &"<CertificateParams>")
            .finish_non_exhaustive()
    }
}

impl CertificateAuthority {
    /// Load an existing CA from `dir`, or generate a fresh one and persist it.
    pub fn load_or_create(dir: &Path) -> Result<Arc<Self>, String> {
        let cert_path = dir.join(CA_CERT_FILE);
        let key_path = dir.join(CA_KEY_FILE);
        if cert_path.exists() && key_path.exists() {
            let cert_pem =
                std::fs::read_to_string(&cert_path).map_err(|e| e.to_string())?;
            let key_pem = std::fs::read_to_string(&key_path).map_err(|e| e.to_string())?;
            Self::from_pem(&cert_pem, &key_pem).map(Arc::new)
        } else {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let ca = Self::generate()?;
            std::fs::write(&cert_path, &ca.cert_pem).map_err(|e| e.to_string())?;
            std::fs::write(&key_path, ca.key.serialize_pem()).map_err(|e| e.to_string())?;
            // CA private key must stay readable only by the owner.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
            }
            Ok(Arc::new(ca))
        }
    }

    fn generate() -> Result<Self, String> {
        let mut issuer = CertificateParams::new(Vec::<String>::new())
            .map_err(|e| e.to_string())?;
        issuer.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        issuer
            .distinguished_name
            .push(DnType::CommonName, "Kairos MITM CA");
        issuer
            .distinguished_name
            .push(DnType::OrganizationName, "Kairos Platform");
        issuer.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let key = KeyPair::generate().map_err(|e| e.to_string())?;
        let issuer_cert = issuer
            .clone()
            .self_signed(&key)
            .map_err(|e| e.to_string())?;
        let cert_pem = issuer_cert.pem();
        Ok(Self {
            issuer_cert,
            key,
            cert_pem,
        })
    }

    fn from_pem(cert_pem: &str, key_pem: &str) -> Result<Self, String> {
        let issuer =
            CertificateParams::from_ca_cert_pem(cert_pem).map_err(|e| e.to_string())?;
        let key = KeyPair::from_pem(key_pem).map_err(|e| e.to_string())?;
        let issuer_cert = issuer.self_signed(&key).map_err(|e| e.to_string())?;
        Ok(Self {
            issuer_cert,
            key,
            cert_pem: cert_pem.to_string(),
        })
    }

    /// PEM encoding of the CA certificate, for injecting trust into tools.
    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }

    /// Issue a leaf certificate for a single hostname.
    pub fn issue(&self, host: &str) -> Result<SignedCert, String> {
        let key = KeyPair::generate().map_err(|e| e.to_string())?;
        let mut params = CertificateParams::new(vec![host.to_string()])
            .map_err(|e| e.to_string())?;
        params
            .distinguished_name
            .push(DnType::CommonName, host);
        let cert = params
            .signed_by(&key, &self.issuer_cert, &self.key)
            .map_err(|e| e.to_string())?;
        Ok(SignedCert {
            cert_der: cert.der().as_ref().to_vec(),
            key_der: key.serialize_der(),
        })
    }
}

/// Convenience helper for resolving the default CA directory.
pub fn default_ca_dir() -> PathBuf {
    std::env::var("PENTA_MITM_CA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("storage/mitm"))
}

/// Path to the CA certificate file inside a CA directory.
pub fn ca_cert_path(dir: &Path) -> PathBuf {
    dir.join(CA_CERT_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("mitm-ca-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn generates_and_issues_certificates() {
        let dir = temp_dir();
        let ca = CertificateAuthority::load_or_create(&dir).unwrap();
        assert!(!ca.cert_pem().is_empty());

        let signed = ca.issue("example.com").unwrap();
        assert!(!signed.cert_der.is_empty());
        assert!(!signed.key_der.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reloads_persisted_ca_and_still_issues() {
        let dir = temp_dir();
        {
            let ca = CertificateAuthority::load_or_create(&dir).unwrap();
            let _ = ca.issue("example.com").unwrap();
        }
        // A second process/startup reloads from disk.
        let ca = CertificateAuthority::load_or_create(&dir).unwrap();
        let signed = ca.issue("example.org").unwrap();
        assert!(!signed.cert_der.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

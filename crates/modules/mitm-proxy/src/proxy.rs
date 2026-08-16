//! HTTP forwarding, CONNECT tunneling, TLS termination and traffic capture.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use http::header;
use http::{HeaderValue, Method, Request, Response, StatusCode, Uri};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::upgrade::Upgraded;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tracing::{debug, warn};

use crate::ca::CertificateAuthority;
use crate::tls;
use crate::{MitmConfig, TrafficRecord, TrafficSink};

type HttpClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>;

/// Bind the proxy listener and serve connections until the process exits.
pub async fn serve(
    config: MitmConfig,
    ca: Arc<CertificateAuthority>,
    sink: Arc<dyn TrafficSink>,
) -> Result<(), String> {
    let handler = Arc::new(ProxyHandler::new(ca, sink, config.body_limit_bytes)?);
    let listener = TcpListener::bind(&config.bind_addr)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(addr = %config.bind_addr, "mitm proxy listening");

    loop {
        let (stream, peer) = listener.accept().await.map_err(|e| e.to_string())?;
        let handler = handler.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| {
                let handler = handler.clone();
                async move { handler.handle(req).await }
            });
            if let Err(error) = auto::Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(io, service)
                .await
            {
                debug!(%peer, %error, "mitm connection closed");
            }
        });
    }
}

struct ProxyHandler {
    client: HttpClient,
    acceptor: TlsAcceptor,
    sink: Arc<dyn TrafficSink>,
    body_limit: usize,
}

impl ProxyHandler {
    fn new(
        ca: Arc<CertificateAuthority>,
        sink: Arc<dyn TrafficSink>,
        body_limit: usize,
    ) -> Result<Self, String> {
        let acceptor = tls::build_acceptor(ca)?;
        let connector = HttpsConnectorBuilder::new()
            .with_tls_config(accept_invalid_certs_config()?)
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(connector);
        Ok(Self {
            client,
            acceptor,
            sink,
            body_limit,
        })
    }

    async fn handle(
        self: Arc<Self>,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        let result = if req.method() == Method::CONNECT {
            self.clone().handle_connect(req).await
        } else {
            self.forward(req).await
        };
        Ok(match result {
            Ok(response) => response,
            Err(error) => {
                warn!(%error, "mitm proxy error");
                bad_gateway()
            }
        })
    }

    async fn handle_connect(
        self: Arc<Self>,
        req: Request<Incoming>,
    ) -> anyhow::Result<Response<Full<Bytes>>> {
        let Some(authority) = req.uri().authority().cloned() else {
            return Ok(bad_request());
        };
        let host = authority.host().to_string();
        let port = authority.port_u16().unwrap_or(443);
        let terminate_tls = port == 443;

        let this = self.clone();
        tokio::spawn(async move {
            match hyper::upgrade::on(req).await {
                Ok(upgraded) => {
                    let io = TokioIo::new(upgraded);
                    if terminate_tls {
                        this.tunnel_tls(io, host, port).await;
                    } else {
                        this.tunnel_plain(io, host, port).await;
                    }
                }
                Err(error) => warn!(%error, "connect upgrade failed"),
            }
        });

        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::new()))
            .unwrap())
    }

    async fn tunnel_tls(self: Arc<Self>, io: TokioIo<Upgraded>, host: String, port: u16) {
        let tls_stream = match self.acceptor.accept(io).await {
            Ok(stream) => stream,
            Err(error) => {
                warn!(%error, host, "mitm tls accept failed");
                return;
            }
        };
        let this = self.clone();
        let host_for_svc = host.clone();
        let service = service_fn(move |req| {
            let this = this.clone();
            let host = host_for_svc.clone();
            async move {
                let req = rewrite_origin(req, "https", &host, port);
                match this.forward(req).await {
                    Ok(resp) => Ok::<_, Infallible>(resp),
                    Err(error) => {
                        warn!(%error, "mitm tls forward failed");
                        Ok(bad_gateway())
                    }
                }
            }
        });
        if let Err(error) = hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(tls_stream), service)
            .await
        {
            debug!(%error, host, "mitm tls tunnel closed");
        }
    }

    async fn tunnel_plain(self: Arc<Self>, io: TokioIo<Upgraded>, host: String, port: u16) {
        match TcpStream::connect((host.as_str(), port)).await {
            Ok(mut upstream) => {
                let mut client_io = io;
                if let Err(error) =
                    tokio::io::copy_bidirectional(&mut client_io, &mut upstream).await
                {
                    debug!(%error, host, port, "mitm plain tunnel closed");
                }
            }
            Err(error) => warn!(%error, host, port, "mitm plain tunnel connect failed"),
        }
    }

    async fn forward(&self, req: Request<Incoming>) -> anyhow::Result<Response<Full<Bytes>>> {
        let run_id = extract_run_id(&req);
        let (parts, body) = req.into_parts();
        let url = parts.uri.to_string();
        let request_headers = headers_to_json(&parts.headers);

        let body_bytes = http_body_util::Limited::new(body, self.body_limit)
            .collect()
            .await
            .map_err(|e| anyhow::anyhow!(e))?
            .to_bytes();
        let request_body = bytes_to_opt_string(&body_bytes);

        let mut builder = Request::builder()
            .method(parts.method.clone())
            .uri(parts.uri.clone())
            .version(parts.version);
        for (name, value) in parts.headers.iter() {
            if is_hop_by_hop(name.as_str()) || name == header::HOST {
                continue;
            }
            builder = builder.header(name, value);
        }
        let outbound = builder
            .body(Full::new(body_bytes.clone()))
            .map_err(|e| anyhow::anyhow!(e))?;

        let started = Instant::now();
        let response = self
            .client
            .request(outbound)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        let duration_ms = started.elapsed().as_millis() as i64;

        let (resp_parts, resp_body) = response.into_parts();
        let response_headers = headers_to_json(&resp_parts.headers);
        let response_body_bytes = http_body_util::Limited::new(resp_body, self.body_limit)
            .collect()
            .await
            .map_err(|e| anyhow::anyhow!(e))?
            .to_bytes();
        let response_body = bytes_to_opt_string(&response_body_bytes);

        let record = TrafficRecord {
            method: parts.method.as_str().to_string(),
            url,
            request_headers,
            request_body,
            response_status: Some(resp_parts.status.as_u16() as i32),
            response_headers,
            response_body,
            duration_ms: Some(duration_ms),
            run_id,
        };
        if let Err(error) = self.sink.record(record).await {
            warn!(%error, "traffic sink failed");
        }

        Ok(Response::from_parts(resp_parts, Full::new(response_body_bytes)))
    }
}

fn accept_invalid_certs_config() -> Result<ClientConfig, String> {
    let provider = tls::default_provider()?;
    let schemes = provider
        .signature_verification_algorithms
        .supported_schemes();
    let verifier = Arc::new(AcceptAnyVerifier { schemes });
    Ok(ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth())
}

/// A `ServerCertVerifier` that accepts any certificate. Penetration targets
/// frequently present self-signed or otherwise invalid certificates; capture
/// must not be blocked by upstream certificate validation.
#[derive(Debug)]
struct AcceptAnyVerifier {
    schemes: Vec<SignatureScheme>,
}

impl ServerCertVerifier for AcceptAnyVerifier {
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
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.schemes.clone()
    }
}

fn rewrite_origin(req: Request<Incoming>, scheme: &str, host: &str, port: u16) -> Request<Incoming> {
    let (mut parts, body) = req.into_parts();
    let path = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/")
        .to_string();
    let authority = if port == 443 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };
    if let Ok(uri) = Uri::try_from(format!("{scheme}://{authority}{path}")) {
        parts.uri = uri;
    }
    parts.headers.remove(header::HOST);
    if let Ok(value) = HeaderValue::from_str(&authority) {
        parts.headers.insert(header::HOST, value);
    }
    Request::from_parts(parts, body)
}

fn extract_run_id(req: &Request<Incoming>) -> Option<String> {
    req.headers()
        .get("x-kairos-run-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "proxy-connection"
            | "keep-alive"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "proxy-authorization"
            | "proxy-authenticate"
            | "content-length"
    )
}

fn headers_to_json(headers: &http::HeaderMap) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        let key = name.as_str().to_string();
        let value = value.to_str().unwrap_or_default().to_string();
        map.entry(key)
            .and_modify(|existing: &mut serde_json::Value| {
                if let Some(prev) = existing.as_str() {
                    *existing = serde_json::Value::String(format!("{prev}, {value}"));
                }
            })
            .or_insert(serde_json::Value::String(value));
    }
    serde_json::Value::Object(map)
}

fn bytes_to_opt_string(bytes: &Bytes) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn bad_request() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Full::new(Bytes::new()))
        .unwrap()
}

fn bad_gateway() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(Full::new(Bytes::new()))
        .unwrap()
}

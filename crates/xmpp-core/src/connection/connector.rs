//! XMPP transport connector that honours the client's TLS configuration.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use jid::Jid;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use tokio_rustls::rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio_rustls::TlsConnector as RustlsConnector;
use tokio_xmpp::connect::{ServerConnector, ServerConnectorError};
use tokio_xmpp::minidom::Element;
use tokio_xmpp::parsers::ns;
use tokio_xmpp::starttls::ServerConfig;
use tokio_xmpp::xmpp_stream::XMPPStream;
use tokio_xmpp::Packet;

use crate::{ProxyConfig, ProxyType, TlsConfig};

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("xmpp error: {0}")]
    Xmpp(#[from] tokio_xmpp::Error),
    #[error("STARTTLS error: {0}")]
    StartTls(#[from] tokio_xmpp::starttls::error::Error),
    #[error("TLS error: {0}")]
    Tls(#[from] tokio_rustls::rustls::Error),
    #[error("invalid server name: {0}")]
    ServerName(String),
    #[error("certificate error: {0}")]
    Certificate(String),
    #[error("server does not offer STARTTLS")]
    NoTls,
    #[error("SASL error: {0}")]
    Sasl(String),
    #[error("invalid bind response")]
    InvalidBindResponse,
    #[error("connection closed")]
    Disconnected,
    #[error("proxy error: {0}")]
    Proxy(String),
}

impl ServerConnectorError for ConnectorError {}

/// Encrypted stream produced by every connector.
pub type ClientStream = XMPPStream<TlsStream<TcpStream>>;

/// Connector selected for a connection attempt.
#[derive(Debug, Clone)]
pub enum Connector {
    /// SRV lookup plus the default webpki trust store.
    System(ServerConfig),
    /// Explicit host and a TLS client config built from [`TlsConfig`].
    Custom(CustomConnector),
}

impl ServerConnector for Connector {
    type Stream = TlsStream<TcpStream>;
    type Error = ConnectorError;

    async fn connect(&self, jid: &Jid, ns: &str) -> Result<XMPPStream<Self::Stream>, Self::Error> {
        match self {
            Connector::System(inner) => inner
                .connect(jid, ns)
                .await
                .map_err(ConnectorError::StartTls),
            Connector::Custom(inner) => inner.connect(jid, ns).await,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomConnector {
    host: String,
    port: u16,
    config: Arc<ClientConfig>,
    proxy: Option<ProxyConfig>,
    direct_tls: bool,
}

impl CustomConnector {
    pub fn new(
        host: String,
        port: u16,
        tls: &TlsConfig,
        proxy: Option<ProxyConfig>,
        direct_tls: bool,
    ) -> Result<Self, ConnectorError> {
        Ok(Self {
            host,
            port,
            config: Arc::new(build_client_config(tls)?),
            proxy,
            direct_tls,
        })
    }
}

impl ServerConnector for CustomConnector {
    type Stream = TlsStream<TcpStream>;
    type Error = ConnectorError;

    async fn connect(&self, jid: &Jid, ns: &str) -> Result<XMPPStream<Self::Stream>, Self::Error> {
        tracing::debug!(host = %self.host, port = self.port, "connecting XMPP TCP");
        let tcp = match &self.proxy {
            Some(proxy) => proxy_connect(proxy, &self.host, self.port).await?,
            None => TcpStream::connect((self.host.as_str(), self.port)).await?,
        };
        tracing::debug!("XMPP TCP connected");

        let server_name = ServerName::try_from(jid.domain().to_string())
            .map_err(|e| ConnectorError::ServerName(e.to_string()))?;

        if self.direct_tls {
            let tls = RustlsConnector::from(self.config.clone())
                .connect(server_name, tcp)
                .await?;
            tracing::debug!("XMPP TLS established (direct)");
            return Ok(XMPPStream::start(tls, jid.clone(), ns.to_owned()).await?);
        }

        let stream = XMPPStream::start(tcp, jid.clone(), ns.to_owned()).await?;

        if !stream.stream_features.can_starttls() {
            return Err(ConnectorError::NoTls);
        }

        let stream = negotiate_starttls(stream).await?;
        let stream = stream.into_inner();
        let tls = RustlsConnector::from(self.config.clone())
            .connect(server_name, stream)
            .await?;
        tracing::debug!("XMPP TLS established (STARTTLS)");

        Ok(XMPPStream::start(tls, jid.clone(), ns.to_owned()).await?)
    }
}

async fn proxy_connect(
    proxy: &ProxyConfig,
    host: &str,
    port: u16,
) -> Result<TcpStream, ConnectorError> {
    let mut stream = TcpStream::connect((proxy.host.as_str(), proxy.port)).await?;
    match proxy.proxy_type {
        ProxyType::Socks5 => socks5_connect(&mut stream, proxy, host, port).await?,
        ProxyType::Http | ProxyType::Https => http_connect(&mut stream, proxy, host, port).await?,
    }
    Ok(stream)
}

async fn socks5_connect(
    stream: &mut TcpStream,
    proxy: &ProxyConfig,
    host: &str,
    port: u16,
) -> Result<(), ConnectorError> {
    let wants_auth = proxy.username.is_some();
    let methods: &[u8] = if wants_auth { &[0x00, 0x02] } else { &[0x00] };
    let method_count = u8::try_from(methods.len()).expect("method list fits in a byte");
    let mut greeting = vec![0x05, method_count];
    greeting.extend_from_slice(methods);
    stream.write_all(&greeting).await?;

    let mut choice = [0u8; 2];
    stream.read_exact(&mut choice).await?;
    if choice[0] != 0x05 {
        return Err(ConnectorError::Proxy("invalid SOCKS version".into()));
    }
    match choice[1] {
        0x00 => {}
        0x02 => {
            let username = proxy.username.as_deref().unwrap_or_default().as_bytes();
            let password = proxy.password.as_deref().unwrap_or_default().as_bytes();
            if username.len() > 255 || password.len() > 255 {
                return Err(ConnectorError::Proxy("SOCKS credentials too long".into()));
            }
            let mut auth = vec![0x01, u8::try_from(username.len()).expect("username fits")];
            auth.extend_from_slice(username);
            auth.push(u8::try_from(password.len()).expect("password fits"));
            auth.extend_from_slice(password);
            stream.write_all(&auth).await?;
            let mut status = [0u8; 2];
            stream.read_exact(&mut status).await?;
            if status[1] != 0x00 {
                return Err(ConnectorError::Proxy("SOCKS authentication failed".into()));
            }
        }
        method => {
            return Err(ConnectorError::Proxy(format!(
                "SOCKS proxy rejected authentication methods (0x{method:02x})"
            )))
        }
    }

    let host_bytes = host.as_bytes();
    if host_bytes.len() > 255 {
        return Err(ConnectorError::Proxy("host name too long".into()));
    }
    let host_len = u8::try_from(host_bytes.len()).expect("host fits in a byte");
    let mut request = vec![0x05, 0x01, 0x00, 0x03, host_len];
    request.extend_from_slice(host_bytes);
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await?;

    let mut reply = [0u8; 4];
    stream.read_exact(&mut reply).await?;
    if reply[1] != 0x00 {
        return Err(ConnectorError::Proxy(format!(
            "SOCKS CONNECT failed (0x{:02x})",
            reply[1]
        )));
    }
    let skip = match reply[3] {
        0x01 => 4,
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            len[0] as usize
        }
        0x04 => 16,
        other => {
            return Err(ConnectorError::Proxy(format!(
                "invalid SOCKS address type 0x{other:02x}"
            )))
        }
    };
    let mut rest = vec![0u8; skip + 2];
    stream.read_exact(&mut rest).await?;
    Ok(())
}

async fn http_connect(
    stream: &mut TcpStream,
    proxy: &ProxyConfig,
    host: &str,
    port: u16,
) -> Result<(), ConnectorError> {
    let target = format!("{host}:{port}");
    let mut request =
        format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nProxy-Connection: keep-alive\r\n");
    if let Some(username) = &proxy.username {
        let password = proxy.password.as_deref().unwrap_or_default();
        let credentials =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        request.push_str("Proxy-Authorization: Basic ");
        request.push_str(&credentials);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await?;

    let mut response = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).await?;
        response.push(byte[0]);
        if response.len() >= 4 && response.ends_with(b"\r\n\r\n") {
            break;
        }
        if response.len() > 8192 {
            return Err(ConnectorError::Proxy("proxy response too long".into()));
        }
    }
    let status_line = String::from_utf8_lossy(&response)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok());
    if status != Some(200) {
        return Err(ConnectorError::Proxy(format!(
            "HTTP CONNECT failed: {status_line}"
        )));
    }
    Ok(())
}

async fn negotiate_starttls<S>(mut stream: XMPPStream<S>) -> Result<XMPPStream<S>, ConnectorError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use futures::{SinkExt, StreamExt};

    stream
        .send(Packet::Stanza(
            Element::builder("starttls", ns::TLS).build(),
        ))
        .await?;

    loop {
        match stream.next().await {
            Some(Ok(Packet::Stanza(stanza))) if stanza.name() == "proceed" => return Ok(stream),
            Some(Ok(Packet::Text(_))) => {}
            Some(Ok(_)) => return Err(ConnectorError::NoTls),
            Some(Err(e)) => return Err(e.into()),
            None => return Err(ConnectorError::NoTls),
        }
    }
}

fn build_client_config(tls: &TlsConfig) -> Result<ClientConfig, ConnectorError> {
    let builder = ClientConfig::builder();

    let builder = if tls.verify_cert && tls.verify_hostname {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        if let Some(path) = tls_path(&tls.ca_file) {
            let certs = read_certs(path, "tls_config.ca_file")?;
            let (added, ignored) = roots.add_parsable_certificates(certs);
            if added == 0 {
                return Err(ConnectorError::Certificate(format!(
                    "no usable certificates in {}",
                    path.display()
                )));
            }
            if ignored > 0 {
                tracing::warn!(
                    "Ignored {ignored} unparsable certificate(s) in {}",
                    path.display()
                );
            }
        }

        builder.with_root_certificates(roots)
    } else {
        tracing::warn!(
            "TLS certificate or hostname verification disabled; only use this for local testing"
        );
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
    };

    match (tls_path(&tls.cert_file), tls_path(&tls.key_file)) {
        (Some(cert_file), Some(key_file)) => {
            let chain = read_certs(cert_file, "tls_config.cert_file")?;
            let key = read_key(key_file, "tls_config.key_file")?;
            Ok(builder.with_client_auth_cert(chain, key)?)
        }
        (None, None) => Ok(builder.with_no_client_auth()),
        _ => Err(ConnectorError::Certificate(
            "cert_file and key_file must be set together".to_string(),
        )),
    }
}

/// A TLS path may be present but blank when a config file or environment
/// variable overrides it with an empty value; treat that as unset instead of
/// trying to open the current directory.
fn tls_path(path: &Option<String>) -> Option<&Path> {
    path.as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(Path::new)
}

fn open_tls_file(path: &Path, setting: &str) -> Result<File, ConnectorError> {
    File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            // The raw `io error: No such file or directory` does not say which
            // setting is wrong, which is the usual cause of a rejected connect.
            ConnectorError::Certificate(format!(
                "{setting} points to a file that does not exist: {}",
                path.display()
            ))
        } else {
            ConnectorError::Io(error)
        }
    })
}

fn read_certs(path: &Path, setting: &str) -> Result<Vec<CertificateDer<'static>>, ConnectorError> {
    let mut reader = BufReader::new(open_tls_file(path, setting)?);
    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(ConnectorError::Io)
}

fn read_key(path: &Path, setting: &str) -> Result<PrivateKeyDer<'static>, ConnectorError> {
    let mut reader = BufReader::new(open_tls_file(path, setting)?);
    rustls_pemfile::private_key(&mut reader)
        .map_err(ConnectorError::Io)?
        .ok_or_else(|| {
            ConnectorError::Certificate(format!("no private key found in {}", path.display()))
        })
}

#[derive(Debug)]
struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, tokio_rustls::rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    fn proxy(kind: ProxyType, username: Option<&str>, password: Option<&str>) -> ProxyConfig {
        ProxyConfig {
            proxy_type: kind,
            host: "127.0.0.1".into(),
            port: 0,
            username: username.map(|value| value.to_string()),
            password: password.map(|value| value.to_string()),
        }
    }

    #[test]
    fn blank_tls_paths_are_unset() {
        assert!(tls_path(&None).is_none());
        assert!(tls_path(&Some(String::new())).is_none());
        assert!(tls_path(&Some("  ".to_string())).is_none());
        assert_eq!(
            tls_path(&Some("/tmp/ca.pem".to_string())),
            Some(Path::new("/tmp/ca.pem"))
        );
    }

    #[test]
    fn missing_tls_file_names_the_setting() {
        let error = open_tls_file(
            Path::new("/nonexistent/jabu-test-ca.pem"),
            "tls_config.ca_file",
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("tls_config.ca_file"), "{message}");
        assert!(message.contains("does not exist"), "{message}");
    }

    #[test]
    fn missing_ca_file_is_reported_with_its_setting() {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
        let tls = TlsConfig {
            ca_file: Some("/nonexistent/jabu-test-ca.pem".to_string()),
            ..TlsConfig::default()
        };
        let message = build_client_config(&tls).unwrap_err().to_string();
        assert!(message.contains("tls_config.ca_file"), "{message}");
        assert!(message.contains("does not exist"), "{message}");
    }

    #[test]
    fn blank_ca_file_keeps_the_default_roots() {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
        let tls = TlsConfig {
            ca_file: Some("  ".to_string()),
            ..TlsConfig::default()
        };
        assert!(build_client_config(&tls).is_ok());
    }

    #[tokio::test]
    async fn http_connect_sends_authorization() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                socket.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
                if request.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8(request).unwrap()
        });

        let mut config = proxy(ProxyType::Http, Some("user"), Some("pass"));
        config.port = addr.port();
        let mut stream = TcpStream::connect(("127.0.0.1", addr.port()))
            .await
            .unwrap();
        http_connect(&mut stream, &config, "xmpp.example.org", 5222)
            .await
            .unwrap();

        let request = server.await.unwrap();
        assert!(request.starts_with("CONNECT xmpp.example.org:5222 HTTP/1.1\r\n"));
        assert!(request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"));
    }

    #[tokio::test]
    async fn socks5_connect_uses_domain_address() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();

            let mut greeting = [0u8; 2];
            socket.read_exact(&mut greeting).await.unwrap();
            let mut methods = vec![0u8; greeting[1] as usize];
            socket.read_exact(&mut methods).await.unwrap();
            socket.write_all(&[0x05, 0x00]).await.unwrap();

            let mut header = [0u8; 5];
            socket.read_exact(&mut header).await.unwrap();
            let mut host = vec![0u8; header[4] as usize];
            socket.read_exact(&mut host).await.unwrap();
            let mut port = [0u8; 2];
            socket.read_exact(&mut port).await.unwrap();

            socket
                .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();

            (String::from_utf8(host).unwrap(), u16::from_be_bytes(port))
        });

        let mut config = proxy(ProxyType::Socks5, None, None);
        config.port = addr.port();
        let mut stream = TcpStream::connect(("127.0.0.1", addr.port()))
            .await
            .unwrap();
        socks5_connect(&mut stream, &config, "xmpp.example.org", 5222)
            .await
            .unwrap();

        let (host, port) = server.await.unwrap();
        assert_eq!(host, "xmpp.example.org");
        assert_eq!(port, 5222);
    }
}

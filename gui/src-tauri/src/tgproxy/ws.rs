//! Минимальный клиент WebSocket поверх TLS: только бинарные кадры, как нужно
//! для MTProto-over-WebSocket (kwsN.web.telegram.org/apiws).

use std::{
    io,
    sync::{Arc, OnceLock},
    time::Duration,
};

use base64::Engine as _;
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    net::TcpStream,
    sync::Mutex,
    time::timeout,
};
use tokio_rustls::{client::TlsStream, TlsConnector};

type Tls = TlsStream<TcpStream>;

const OP_CONT: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;
const MAX_MESSAGE_LEN: u64 = 16 * 1024 * 1024;

fn roots() -> RootCertStore {
    RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() }
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Проверяет цепочку сертификатов, но не имя сервера: нужно для подключения
/// с подменённым SNI (domain fronting), когда сертификат выдан на другое имя.
#[derive(Debug)]
struct ChainOnly(Arc<rustls::client::WebPkiServerVerifier>);

impl ServerCertVerifier for ChainOnly {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self.0.verify_server_cert(end_entity, intermediates, server_name, ocsp, now) {
            Err(rustls::Error::InvalidCertificate(CertificateError::NotValidForName))
            | Err(rustls::Error::InvalidCertificate(CertificateError::NotValidForNameContext { .. })) => {
                Ok(ServerCertVerified::assertion())
            }
            other => other,
        }
    }

    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls12_signature(m, c, d)
    }

    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls13_signature(m, c, d)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_verify_schemes()
    }
}

fn config(fronting: bool) -> Arc<ClientConfig> {
    static NORMAL: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    static FRONTING: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    let build = |fronting: bool| {
        let builder = ClientConfig::builder_with_provider(provider()).with_safe_default_protocol_versions().expect("TLS");
        let mut cfg = if fronting {
            let inner = rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(roots()), provider())
                .build()
                .expect("TLS verifier");
            builder.dangerous().with_custom_certificate_verifier(Arc::new(ChainOnly(inner))).with_no_client_auth()
        } else {
            builder.with_root_certificates(roots()).with_no_client_auth()
        };
        cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
        Arc::new(cfg)
    };
    if fronting {
        FRONTING.get_or_init(|| build(true)).clone()
    } else {
        NORMAL.get_or_init(|| build(false)).clone()
    }
}

/// Куда и как подключаться.
pub struct Target<'a> {
    /// Адрес TCP-подключения: IP или имя.
    pub addr: &'a str,
    /// Заголовок Host и путь запроса.
    pub host: &'a str,
    pub path: &'a str,
    /// SNI для TLS; None — совпадает с host.
    pub sni: Option<&'a str>,
}

#[derive(Clone)]
pub struct WsWriter(Arc<Mutex<WriteHalf<Tls>>>);

pub struct WsReader {
    rd: BufReader<ReadHalf<Tls>>,
    wr: WsWriter,
    frag: Vec<u8>,
    closed: bool,
}

pub async fn connect(t: Target<'_>, limit: Duration) -> io::Result<(WsReader, WsWriter)> {
    let tcp = timeout(limit, TcpStream::connect((t.addr, 443u16)))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "TCP: таймаут"))??;
    let _ = tcp.set_nodelay(true);
    let sni = t.sni.unwrap_or(t.host);
    let name = ServerName::try_from(sni.to_string()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let tls = timeout(limit, TlsConnector::from(config(t.sni.is_some())).connect(name, tcp))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "TLS: таймаут"))??;

    let (rd, mut wr) = tokio::io::split(tls);
    let mut key = [0u8; 16];
    getrandom::getrandom(&mut key).map_err(|e| io::Error::other(e.to_string()))?;
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: binary\r\n\r\n",
        t.path,
        t.host,
        base64::engine::general_purpose::STANDARD.encode(key)
    );
    wr.write_all(req.as_bytes()).await?;
    wr.flush().await?;

    let mut rd = BufReader::new(rd);
    let status = timeout(limit, async {
        let mut first = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            if rd.read_line(&mut line).await? == 0 {
                break;
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if first.is_empty() {
                first = line.trim().to_string();
            }
            if line.len() > 8192 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "слишком длинный заголовок"));
            }
        }
        Ok::<_, io::Error>(first)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "WebSocket: таймаут ответа"))??;

    let code = status.split(' ').nth(1).and_then(|c| c.parse::<u16>().ok()).unwrap_or(0);
    if code != 101 {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            if status.is_empty() { "WebSocket: пустой ответ".to_string() } else { format!("WebSocket: {status}") },
        ));
    }
    let wr = WsWriter(Arc::new(Mutex::new(wr)));
    Ok((WsReader { rd, wr: wr.clone(), frag: Vec::new(), closed: false }, wr))
}

fn mask_key() -> [u8; 4] {
    let mut m = [0u8; 4];
    let _ = getrandom::getrandom(&mut m);
    m
}

/// Кадр клиента (всегда с маской, как требует RFC 6455).
fn push_frame(out: &mut Vec<u8>, opcode: u8, data: &[u8]) {
    out.push(0x80 | opcode);
    let len = data.len();
    if len < 126 {
        out.push(0x80 | len as u8);
    } else if len < 65536 {
        out.push(0x80 | 126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(0x80 | 127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    let mask = mask_key();
    out.extend_from_slice(&mask);
    let start = out.len();
    out.extend_from_slice(data);
    for (i, b) in out[start..].iter_mut().enumerate() {
        *b ^= mask[i & 3];
    }
}

impl WsWriter {
    async fn write_raw(&self, buf: &[u8]) -> io::Result<()> {
        let mut w = self.0.lock().await;
        w.write_all(buf).await?;
        w.flush().await
    }

    /// Отправляет несколько бинарных сообщений одной записью.
    pub async fn send_all(&self, parts: &[&[u8]]) -> io::Result<()> {
        let mut buf = Vec::with_capacity(parts.iter().map(|p| p.len() + 14).sum());
        for p in parts {
            push_frame(&mut buf, OP_BINARY, p);
        }
        self.write_raw(&buf).await
    }

    pub async fn send(&self, data: &[u8]) -> io::Result<()> {
        self.send_all(&[data]).await
    }

    pub async fn close(&self) {
        let mut buf = Vec::new();
        push_frame(&mut buf, OP_CLOSE, &[]);
        let _ = self.write_raw(&buf).await;
        let _ = self.0.lock().await.shutdown().await;
    }
}

impl WsReader {
    async fn read_frame(&mut self) -> io::Result<(u8, Vec<u8>, bool)> {
        let mut hdr = [0u8; 2];
        self.rd.read_exact(&mut hdr).await?;
        let fin = hdr[0] & 0x80 != 0;
        let opcode = hdr[0] & 0x0f;
        let mut len = (hdr[1] & 0x7f) as u64;
        if len == 126 {
            len = self.rd.read_u16().await? as u64;
        } else if len == 127 {
            len = self.rd.read_u64().await?;
        }
        if len > MAX_MESSAGE_LEN {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "слишком большой кадр WebSocket"));
        }
        let mask = if hdr[1] & 0x80 != 0 {
            let mut m = [0u8; 4];
            self.rd.read_exact(&mut m).await?;
            Some(m)
        } else {
            None
        };
        let mut payload = vec![0u8; len as usize];
        self.rd.read_exact(&mut payload).await?;
        if let Some(m) = mask {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= m[i & 3];
            }
        }
        Ok((opcode, payload, fin))
    }

    /// Следующее сообщение; None — сервер закрыл соединение.
    pub async fn recv(&mut self) -> io::Result<Option<Vec<u8>>> {
        while !self.closed {
            let (op, payload, fin) = self.read_frame().await?;
            match op {
                OP_CLOSE => {
                    self.closed = true;
                    let mut buf = Vec::new();
                    push_frame(&mut buf, OP_CLOSE, &payload[..payload.len().min(2)]);
                    let _ = self.wr.write_raw(&buf).await;
                    return Ok(None);
                }
                OP_PING => {
                    let mut buf = Vec::new();
                    push_frame(&mut buf, OP_PONG, &payload);
                    let _ = self.wr.write_raw(&buf).await;
                }
                OP_PONG => {}
                OP_CONT | OP_TEXT | OP_BINARY => {
                    if fin && self.frag.is_empty() {
                        return Ok(Some(payload));
                    }
                    self.frag.extend_from_slice(&payload);
                    if self.frag.len() as u64 > MAX_MESSAGE_LEN {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "слишком большое сообщение WebSocket"));
                    }
                    if fin {
                        return Ok(Some(std::mem::take(&mut self.frag)));
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_lengths_and_mask() {
        for len in [0usize, 5, 125, 126, 65535, 65536] {
            let data = vec![0xabu8; len];
            let mut out = Vec::new();
            push_frame(&mut out, OP_BINARY, &data);
            assert_eq!(out[0], 0x82);
            let (hlen, plen) = match out[1] & 0x7f {
                126 => (4, u16::from_be_bytes([out[2], out[3]]) as usize),
                127 => (10, u64::from_be_bytes(out[2..10].try_into().unwrap()) as usize),
                n => (2, n as usize),
            };
            assert_eq!(plen, len);
            let mask = &out[hlen..hlen + 4];
            let body: Vec<u8> = out[hlen + 4..].iter().enumerate().map(|(i, b)| b ^ mask[i & 3]).collect();
            assert_eq!(body, data);
        }
    }
}

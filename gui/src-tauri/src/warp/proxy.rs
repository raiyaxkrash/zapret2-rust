//! Локальный прокси WARP на 127.0.0.1: HTTP CONNECT, обычный HTTP и SOCKS5.
//! По адресу /proxy.pac он же отдаёт PAC-файл: через WARP идут только выбранные домены.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddrV4},
    sync::{Arc, RwLock},
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};

use super::tunnel::{Conn, Tunnel};

const HEADER_LIMIT: usize = 16 * 1024;

/// Домены, которые идут через WARP (для PAC).
pub type Domains = Arc<RwLock<Vec<String>>>;

pub fn pac(domains: &[String], port: u16) -> String {
    let list: Vec<String> = domains.iter().map(|d| format!("\"{}\"", d.replace('"', ""))).collect();
    format!(
        "function FindProxyForURL(url, host) {{\n  host = host.toLowerCase();\n  var d = [{}];\n  for (var i = 0; i < d.length; i++) {{\n    if (host == d[i] || dnsDomainIs(host, \".\" + d[i])) return \"PROXY 127.0.0.1:{port}; DIRECT\";\n  }}\n  return \"DIRECT\";\n}}\n",
        list.join(", ")
    )
}

/// Адрес назначения: через системный DNS, а если он подменён — через DNS-over-HTTPS.
async fn resolve(host: &str, port: u16) -> Result<SocketAddrV4, String> {
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(SocketAddrV4::new(ip, port));
    }
    let sys: Vec<Ipv4Addr> = tokio::net::lookup_host((host, port))
        .await
        .map(|it| it.filter_map(|a| match a.ip() { IpAddr::V4(v) => Some(v), _ => None }).collect())
        .unwrap_or_default();
    if let Some(ip) = sys.iter().find(|ip| !(ip.is_private() || ip.is_loopback() || ip.is_unspecified())) {
        return Ok(SocketAddrV4::new(*ip, port));
    }
    let c = reqwest::Client::builder().timeout(Duration::from_secs(8)).build().map_err(|e| e.to_string())?;
    let v: serde_json::Value = c
        .get(format!("https://cloudflare-dns.com/dns-query?name={host}&type=A"))
        .header("accept", "application/dns-json")
        .send()
        .await
        .map_err(|e| format!("{host}: {e}"))?
        .json()
        .await
        .map_err(|e| format!("{host}: {e}"))?;
    v["Answer"]
        .as_array()
        .and_then(|a| a.iter().filter(|r| r["type"] == 1).find_map(|r| r["data"].as_str()?.parse::<Ipv4Addr>().ok()))
        .map(|ip| SocketAddrV4::new(ip, port))
        .ok_or_else(|| format!("{host}: адрес не найден"))
}

/// Пересылка между клиентом и соединением в туннеле.
async fn bridge(client: TcpStream, mut conn: Conn, first: Vec<u8>, tunnel: Arc<Tunnel>) {
    let wake = tunnel.wake();
    let (mut rd, mut wr) = client.into_split();
    if !first.is_empty() && conn.tx.send(first).await.is_ok() {
        wake.notify_one();
    }
    let tx = conn.tx.clone();
    let wake2 = wake.clone();
    let up = async move {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            match rd.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                    wake2.notify_one();
                }
            }
        }
    };
    let down = async {
        while let Some(d) = conn.rx.recv().await {
            if wr.write_all(&d).await.is_err() {
                break;
            }
            wake.notify_one();
        }
        let _ = wr.shutdown().await;
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }
}

async fn read_header(s: &mut TcpStream, buf: &mut Vec<u8>) -> Option<usize> {
    let mut tmp = [0u8; 4096];
    loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return Some(i + 4);
        }
        if buf.len() > HEADER_LIMIT {
            return None;
        }
        let n = tokio::time::timeout(Duration::from_secs(15), s.read(&mut tmp)).await.ok()?.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
}

fn split_host_port(s: &str, default: u16) -> (String, u16) {
    match s.rsplit_once(':') {
        Some((h, p)) if !h.contains(']') || h.ends_with(']') => match p.parse() {
            Ok(p) => (h.trim_matches(['[', ']']).to_string(), p),
            Err(_) => (s.to_string(), default),
        },
        _ => (s.to_string(), default),
    }
}

async fn handle(mut s: TcpStream, tunnel: Arc<Tunnel>, domains: Domains, port: u16) {
    let _ = s.set_nodelay(true);
    let mut first = [0u8; 1];
    if tokio::time::timeout(Duration::from_secs(15), s.peek(&mut first)).await.ok().and_then(|r| r.ok()) != Some(1) {
        return;
    }
    if first[0] == 0x05 {
        socks5(s, tunnel).await;
        return;
    }
    let mut buf = Vec::new();
    let Some(end) = read_header(&mut s, &mut buf).await else { return };
    let head = String::from_utf8_lossy(&buf[..end]).to_string();
    let rest = buf[end..].to_vec();
    let mut parts = head.lines().next().unwrap_or_default().split_whitespace();
    let (method, target) = (parts.next().unwrap_or_default().to_string(), parts.next().unwrap_or_default().to_string());

    if method == "GET" && target.starts_with("/proxy.pac") {
        let body = pac(&domains.read().unwrap(), port);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = s.write_all(resp.as_bytes()).await;
        return;
    }

    if method == "CONNECT" {
        let (host, p) = split_host_port(&target, 443);
        let conn = match resolve(&host, p).await {
            Ok(addr) => tunnel.connect(addr).await,
            Err(e) => Err(e),
        };
        match conn {
            Ok(conn) => {
                if s.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.is_ok() {
                    bridge(s, conn, rest, tunnel).await;
                }
            }
            Err(_) => {
                let _ = s.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            }
        }
        return;
    }

    // Обычный HTTP через прокси: «GET http://host/path» → «GET /path» к host:80.
    let Some(after) = target.strip_prefix("http://") else {
        let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        return;
    };
    let (authority, path) = match after.find('/') {
        Some(i) => (&after[..i], &after[i..]),
        None => (after, "/"),
    };
    let (host, p) = split_host_port(authority, 80);
    let Ok(addr) = resolve(&host, p).await else {
        let _ = s.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        return;
    };
    let Ok(conn) = tunnel.connect(addr).await else {
        let _ = s.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        return;
    };
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let version = request_line.rsplit(' ').next().unwrap_or("HTTP/1.1");
    let mut out = format!("{method} {path} {version}\r\n");
    for l in lines {
        if l.to_ascii_lowercase().starts_with("proxy-") {
            continue;
        }
        out.push_str(l);
        out.push_str("\r\n");
    }
    let mut first = out.trim_end_matches("\r\n").to_string().into_bytes();
    first.extend_from_slice(b"\r\n\r\n");
    first.extend_from_slice(&rest);
    bridge(s, conn, first, tunnel).await;
}

async fn socks5(mut s: TcpStream, tunnel: Arc<Tunnel>) {
    let mut hdr = [0u8; 2];
    if s.read_exact(&mut hdr).await.is_err() {
        return;
    }
    let mut methods = vec![0u8; hdr[1] as usize];
    if s.read_exact(&mut methods).await.is_err() || s.write_all(&[5, 0]).await.is_err() {
        return;
    }
    let mut req = [0u8; 4];
    if s.read_exact(&mut req).await.is_err() || req[1] != 1 {
        let _ = s.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        return;
    }
    let host = match req[3] {
        1 => {
            let mut ip = [0u8; 4];
            if s.read_exact(&mut ip).await.is_err() {
                return;
            }
            Ipv4Addr::from(ip).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            if s.read_exact(&mut len).await.is_err() {
                return;
            }
            let mut name = vec![0u8; len[0] as usize];
            if s.read_exact(&mut name).await.is_err() {
                return;
            }
            String::from_utf8_lossy(&name).to_string()
        }
        _ => {
            let _ = s.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            return;
        }
    };
    let mut port = [0u8; 2];
    if s.read_exact(&mut port).await.is_err() {
        return;
    }
    let conn = match resolve(&host, u16::from_be_bytes(port)).await {
        Ok(a) => tunnel.connect(a).await,
        Err(e) => Err(e),
    };
    match conn {
        Ok(conn) => {
            if s.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.is_ok() {
                bridge(s, conn, Vec::new(), tunnel).await;
            }
        }
        Err(_) => {
            let _ = s.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        }
    }
}

/// Принимает подключения. Все соединения живут в JoinSet и закрываются вместе с задачей.
pub async fn serve(listener: TcpListener, shared: super::Shared, domains: Domains, port: u16) {
    let mut tasks = JoinSet::new();
    loop {
        while tasks.try_join_next().is_some() {}
        match listener.accept().await {
            Ok((s, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                // Туннель берётся на момент подключения: после переподключения — уже новый.
                let Some(tunnel) = shared.read().unwrap().clone() else { continue };
                tasks.spawn(handle(s, tunnel, domains.clone(), port));
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pac_routes_only_listed() {
        let p = pac(&["chatgpt.com".into(), "openai.com".into()], 40080);
        assert!(p.contains("\"chatgpt.com\", \"openai.com\""));
        assert!(p.contains("PROXY 127.0.0.1:40080; DIRECT"));
        assert!(p.contains("return \"DIRECT\""));
    }

    #[test]
    fn host_port() {
        assert_eq!(split_host_port("chatgpt.com:443", 80), ("chatgpt.com".into(), 443));
        assert_eq!(split_host_port("example.com", 80), ("example.com".into(), 80));
    }
}

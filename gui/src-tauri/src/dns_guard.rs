//! Защита от подмены DNS провайдером (DNS-over-HTTPS поверх WinDivert).
//!
//! Перехватывает исходящие UDP-запросы на порт 53 (IPv4 и IPv6), отправляет их в зашифрованном
//! виде по стандарту RFC 8484 напрямую по IP (без зависимости от системного DNS) и возвращает
//! программе подлинный DNS-ответ. Локальные имена (.local, .lan и без точки) не перехватываются,
//! а при недоступности DoH пакет пропускается обычным путём, чтобы не ломать сеть.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use tokio::sync::{Notify, Semaphore};

use crate::windivert::{
    fold, sum16, Addr, Dll, HandleGuard, ADDR_CHECKSUMS_OK, ADDR_IPV6, ADDR_LOOPBACK, ADDR_OUTBOUND, FLAG_DIVERT,
    FLAG_SEND_ONLY, PRIORITY_DNS_CAPTURE, PRIORITY_DNS_INJECT_IN, PRIORITY_DNS_PASSTHROUGH,
};

const CACHE_MIN_TTL: u32 = 15;
const CACHE_MAX_TTL: u32 = 300;
const CACHE_NXDOMAIN_TTL: u32 = 30;
const CACHE_MAX: usize = 512;
const MAX_CONCURRENT_DOH: usize = 64;

#[derive(Serialize, Clone, Debug, Default)]
pub struct DnsGuardStatus {
    pub running: bool,
    pub provider: String,
    pub upstream: String,
    pub total: u64,
    pub cached: u64,
    pub doh_ok: u64,
    pub fallback: u64,
}

/// Список DoH-адресов для выбранного провайдера.
/// Встроенные провайдеры обращаются напрямую по IP (TLS-сертификаты подписаны на IP),
/// чтобы запросы DoH не зависели от системного DNS и не уходили в рекурсию.
pub fn doh_endpoints(provider: &str, custom_url: &str) -> Vec<String> {
    match provider {
        "cloudflare" => vec!["https://1.1.1.1/dns-query".into(), "https://1.0.0.1/dns-query".into()],
        "google" => vec!["https://8.8.8.8/dns-query".into(), "https://8.8.4.4/dns-query".into()],
        "quad9" => vec!["https://9.9.9.9/dns-query".into(), "https://149.112.112.112/dns-query".into()],
        "adguard" => vec!["https://94.140.14.14/dns-query".into(), "https://94.140.15.15/dns-query".into()],
        "custom" => {
            let u = custom_url.trim();
            if u.starts_with("https://") && u.len() > 10 {
                vec![u.to_string(), "https://1.1.1.1/dns-query".into()]
            } else {
                vec!["https://1.1.1.1/dns-query".into(), "https://8.8.8.8/dns-query".into()]
            }
        }
        _ => vec!["https://1.1.1.1/dns-query".into(), "https://8.8.8.8/dns-query".into()],
    }
}

/// Строит входящий IPv4+UDP пакет с DNS-ответом и верными контрольными суммами.
fn build_udp_reply_v4(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, src_port: u16, dst_port: u16, payload: &[u8]) -> Option<Vec<u8>> {
    let udp_len = 8 + payload.len();
    let total_len = 20 + udp_len;
    if total_len > 65_535 {
        return None;
    }
    let mut pkt = vec![0u8; total_len];
    pkt[0] = 0x45;
    pkt[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
    pkt[6] = 0x40; // Don't Fragment
    pkt[8] = 64;   // TTL
    pkt[9] = 17;   // UDP
    pkt[12..16].copy_from_slice(&src_ip.octets());
    pkt[16..20].copy_from_slice(&dst_ip.octets());
    let ip_csum = fold(sum16(&pkt[..20], 0));
    pkt[10..12].copy_from_slice(&ip_csum.to_be_bytes());

    pkt[20..22].copy_from_slice(&src_port.to_be_bytes());
    pkt[22..24].copy_from_slice(&dst_port.to_be_bytes());
    pkt[24..26].copy_from_slice(&(udp_len as u16).to_be_bytes());
    pkt[28..].copy_from_slice(payload);

    let pseudo = sum16(&pkt[12..20], 0) + 17 + udp_len as u32;
    let mut udp_csum = fold(sum16(&pkt[20..], pseudo));
    if udp_csum == 0 {
        udp_csum = 0xffff;
    }
    pkt[26..28].copy_from_slice(&udp_csum.to_be_bytes());
    Some(pkt)
}

/// Строит входящий IPv6+UDP пакет с DNS-ответом и верной контрольной суммой.
fn build_udp_reply_v6(src_ip: Ipv6Addr, dst_ip: Ipv6Addr, src_port: u16, dst_port: u16, payload: &[u8]) -> Option<Vec<u8>> {
    let udp_len = 8 + payload.len();
    if udp_len > 65_535 {
        return None;
    }
    let mut pkt = vec![0u8; 40 + udp_len];
    pkt[0] = 0x60; // IPv6 version
    pkt[4..6].copy_from_slice(&(udp_len as u16).to_be_bytes());
    pkt[6] = 17;   // Next Header: UDP
    pkt[7] = 64;   // Hop Limit
    pkt[8..24].copy_from_slice(&src_ip.octets());
    pkt[24..40].copy_from_slice(&dst_ip.octets());

    pkt[40..42].copy_from_slice(&src_port.to_be_bytes());
    pkt[42..44].copy_from_slice(&dst_port.to_be_bytes());
    pkt[44..46].copy_from_slice(&(udp_len as u16).to_be_bytes());
    pkt[48..].copy_from_slice(payload);

    let pseudo = sum16(&pkt[8..40], 0) + 17 + udp_len as u32;
    let mut udp_csum = fold(sum16(&pkt[40..], pseudo));
    if udp_csum == 0 {
        udp_csum = 0xffff;
    }
    pkt[46..48].copy_from_slice(&udp_csum.to_be_bytes());
    Some(pkt)
}

/// Пропускает сжатое или обычное DNS-имя начиная с `pos`, возвращает новую позицию.
fn skip_dns_name(msg: &[u8], mut pos: usize) -> Option<usize> {
    for _ in 0..64 {
        let len = *msg.get(pos)? as usize;
        if len == 0 {
            return Some(pos + 1);
        }
        if len & 0xc0 == 0xc0 {
            msg.get(pos + 1)?;
            return Some(pos + 2);
        }
        if len >= 64 {
            return None;
        }
        pos = pos.checked_add(1 + len)?;
        if pos > msg.len() {
            return None;
        }
    }
    None
}

/// Определяет TTL кэширования по RCODE и минимальному TTL секции Answer.
/// Возвращает `None`, если ответ нельзя кэшировать (например, SERVFAIL / REFUSED).
fn response_cache_ttl(reply: &[u8]) -> Option<Duration> {
    if reply.len() < 12 || reply[2] & 0x80 == 0 {
        return None;
    }
    let rcode = reply[3] & 0x0f;
    if rcode == 3 {
        // NXDOMAIN кэшируем ненадолго.
        return Some(Duration::from_secs(CACHE_NXDOMAIN_TTL as u64));
    }
    if rcode != 0 {
        // SERVFAIL, FORMERR, REFUSED и др. не кэшируем.
        return None;
    }
    let qd = u16::from_be_bytes([reply[4], reply[5]]) as usize;
    let an = u16::from_be_bytes([reply[6], reply[7]]) as usize;
    let mut pos = 12usize;
    for _ in 0..qd {
        pos = skip_dns_name(reply, pos)?;
        pos = pos.checked_add(4)?;
        if pos > reply.len() {
            return None;
        }
    }
    let mut min_ttl: Option<u32> = None;
    for _ in 0..an {
        pos = skip_dns_name(reply, pos)?;
        let hdr = reply.get(pos..pos + 10)?;
        let ttl = u32::from_be_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
        let rdlen = u16::from_be_bytes([hdr[8], hdr[9]]) as usize;
        pos = pos.checked_add(10 + rdlen)?;
        if pos > reply.len() {
            break;
        }
        min_ttl = Some(match min_ttl {
            Some(cur) => cur.min(ttl),
            None => ttl,
        });
    }
    let secs = min_ttl.unwrap_or(CACHE_NXDOMAIN_TTL).clamp(CACHE_MIN_TTL, CACHE_MAX_TTL);
    Some(Duration::from_secs(secs as u64))
}

/// Извлекает имя и тип стандартного DNS-запроса. Локальные имена пропускаются.
fn parse_public_query(msg: &[u8]) -> Option<(String, u16)> {
    if msg.len() < 17 || msg[2] & 0x80 != 0 {
        return None;
    }
    let qd = u16::from_be_bytes([msg[4], msg[5]]);
    if qd != 1 {
        return None;
    }
    let mut pos = 12usize;
    let mut name = String::new();
    for _ in 0..64 {
        let len = *msg.get(pos)? as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if len >= 64 {
            return None;
        }
        let label = msg.get(pos + 1..pos + 1 + len)?;
        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(&String::from_utf8_lossy(label).to_ascii_lowercase());
        pos += 1 + len;
    }
    let qtype = u16::from_be_bytes([*msg.get(pos)?, *msg.get(pos + 1)?]);
    let is_local = !name.contains('.')
        || [".local", ".lan", ".home", ".internal", ".corp", ".arpa"]
            .iter()
            .any(|suf| name.ends_with(suf));
    if is_local {
        return None;
    }
    Some((name, qtype))
}

/// Строит минимальный RFC 1035 DNS-запрос типа A (qtype=1) для домена.
fn build_a_query(domain: &str) -> Option<Vec<u8>> {
    let mut q = vec![0, 0, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    let trimmed = domain.trim().trim_end_matches('.');
    if trimmed.is_empty() || trimmed.len() > 253 {
        return None;
    }
    for label in trimmed.split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0);
    q.extend_from_slice(&[0, 1, 0, 1]); // QTYPE=A, QCLASS=IN
    Some(q)
}

/// Извлекает IPv4 A-записи из бинарного DNS-ответа (RFC 1035).
fn extract_a_records(reply: &[u8]) -> Vec<IpAddr> {
    let mut out = Vec::new();
    if reply.len() < 12 || reply[2] & 0x80 == 0 || (reply[3] & 0x0f) != 0 {
        return out;
    }
    let qd = u16::from_be_bytes([reply[4], reply[5]]) as usize;
    let an = u16::from_be_bytes([reply[6], reply[7]]) as usize;
    let mut pos = 12usize;
    for _ in 0..qd {
        let Some(next) = skip_dns_name(reply, pos) else { return out };
        pos = next + 4;
        if pos > reply.len() {
            return out;
        }
    }
    for _ in 0..an {
        let Some(next) = skip_dns_name(reply, pos) else { break };
        let Some(hdr) = reply.get(next..next + 10) else { break };
        let rtype = u16::from_be_bytes([hdr[0], hdr[1]]);
        let rdlen = u16::from_be_bytes([hdr[8], hdr[9]]) as usize;
        let Some(rdata) = reply.get(next + 10..next + 10 + rdlen) else { break };
        if rtype == 1 && rdlen == 4 {
            out.push(IpAddr::V4(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3])));
        }
        pos = next + 10 + rdlen;
    }
    out
}

fn doh_client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(5))
                .build()
                .ok()
        })
        .as_ref()
}

async fn resolve_doh_wire(query: &[u8], endpoints: &[String]) -> Option<(Vec<u8>, String)> {
    let client = doh_client()?;
    if query.len() < 12 {
        return None;
    }
    let mut q = query.to_vec();
    q[0] = 0;
    q[1] = 0;
    for endpoint in endpoints {
        let Ok(resp) = client
            .post(endpoint)
            .header("content-type", "application/dns-message")
            .header("accept", "application/dns-message")
            .body(q.clone())
            .send()
            .await
        else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        if let Ok(bytes) = resp.bytes().await {
            if bytes.len() >= 12 {
                return Some((bytes.to_vec(), endpoint.clone()));
            }
        }
    }
    None
}

/// Разрешает A-записи домена через DoH напрямую по IP (`1.1.1.1` / `8.8.8.8`),
/// без обращения к системному DNS и системному прокси.
pub async fn resolve_doh_a(domain: &str) -> Option<Vec<IpAddr>> {
    let query = build_a_query(domain)?;
    let endpoints = doh_endpoints("auto", "");
    let (reply, _) = resolve_doh_wire(&query, &endpoints).await?;
    Some(extract_a_records(&reply))
}

struct CacheEntry {
    reply: Vec<u8>,
    expires: Instant,
}

type DnsCache = HashMap<(String, u16), CacheEntry>;

struct Running {
    stop: AtomicBool,
    dll: Dll,
    capture: Arc<HandleGuard>,
    inject_in: HandleGuard,
    pass_out: HandleGuard,
    provider: String,
    endpoints: Vec<String>,
    cache: Mutex<DnsCache>,
    inflight: Mutex<HashMap<(String, u16), Arc<Notify>>>,
    sem: Arc<Semaphore>,
    upstream: Mutex<String>,
    total: AtomicU64,
    cached: AtomicU64,
    doh_ok: AtomicU64,
    fallback: AtomicU64,
}

#[derive(Default)]
pub struct DnsGuard {
    state: Mutex<Option<Arc<Running>>>,
}

impl DnsGuard {
    pub fn is_running(&self) -> bool {
        self.state.lock().unwrap().is_some()
    }

    pub fn status(&self) -> DnsGuardStatus {
        let g = self.state.lock().unwrap();
        match g.as_ref() {
            Some(r) => DnsGuardStatus {
                running: true,
                provider: r.provider.clone(),
                upstream: r.upstream.lock().unwrap().clone(),
                total: r.total.load(Ordering::Relaxed),
                cached: r.cached.load(Ordering::Relaxed),
                doh_ok: r.doh_ok.load(Ordering::Relaxed),
                fallback: r.fallback.load(Ordering::Relaxed),
            },
            None => DnsGuardStatus::default(),
        }
    }

    pub fn start(&self, engine_bin: &Path, provider: &str, custom_url: &str) -> Result<(), String> {
        self.stop();
        let dll_path = engine_bin.join("WinDivert.dll");
        if !dll_path.exists() || !engine_bin.join("WinDivert64.sys").exists() {
            return Err("Защита DNS: файлы zapret2 (WinDivert) ещё не загружены".into());
        }
        let dll = Dll::load(&dll_path)?;
        let capture = Arc::new(dll.open(
            "outbound and !loopback and (ip or ipv6) and udp.DstPort == 53",
            PRIORITY_DNS_CAPTURE,
            FLAG_DIVERT,
            "WinDivert (DNS)",
        )?);
        let inject_in = dll.open("false", PRIORITY_DNS_INJECT_IN, FLAG_SEND_ONLY, "WinDivert (DNS)")?;
        let pass_out = dll.open("false", PRIORITY_DNS_PASSTHROUGH, FLAG_SEND_ONLY, "WinDivert (DNS)")?;
        let endpoints = doh_endpoints(provider, custom_url);
        let first_upstream = endpoints.first().cloned().unwrap_or_default();

        let running = Arc::new(Running {
            stop: AtomicBool::new(false),
            dll,
            capture: capture.clone(),
            inject_in,
            pass_out,
            provider: provider.to_string(),
            endpoints,
            cache: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            sem: Arc::new(Semaphore::new(MAX_CONCURRENT_DOH)),
            upstream: Mutex::new(first_upstream),
            total: AtomicU64::new(0),
            cached: AtomicU64::new(0),
            doh_ok: AtomicU64::new(0),
            fallback: AtomicU64::new(0),
        });
        *self.state.lock().unwrap() = Some(running.clone());

        std::thread::spawn(move || recv_loop(running, capture));
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(r) = self.state.lock().unwrap().take() {
            r.stop.store(true, Ordering::Relaxed);
            r.dll.shutdown(&r.capture);
        }
    }
}

impl Drop for DnsGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

fn recv_loop(r: Arc<Running>, capture: Arc<HandleGuard>) {
    let mut buf = vec![0u8; 65_535];
    let mut addr = Addr::default();
    while let Some(n) = r.dll.recv(&capture, &mut buf, &mut addr) {
        if r.stop.load(Ordering::Relaxed) {
            break;
        }
        let pkt = buf[..n].to_vec();
        let pkt_addr = addr;
        let state = r.clone();
        tauri::async_runtime::spawn(async move {
            handle_dns_packet(&state, pkt, pkt_addr).await;
        });
    }
}

enum ParsedUdp<'a> {
    V4 {
        src_ip: Ipv4Addr,
        dst_ip: Ipv4Addr,
        src_port: u16,
        dst_port: u16,
        payload: &'a [u8],
    },
    V6 {
        src_ip: Ipv6Addr,
        dst_ip: Ipv6Addr,
        src_port: u16,
        dst_port: u16,
        payload: &'a [u8],
    },
}

fn parse_udp_packet(pkt: &[u8]) -> Option<ParsedUdp<'_>> {
    let ver = *pkt.first()? >> 4;
    match ver {
        4 => {
            if pkt.len() < 28 || pkt[9] != 17 {
                return None;
            }
            let ihl = ((pkt[0] & 0x0f) as usize) * 4;
            if ihl < 20 || pkt.len() < ihl + 8 {
                return None;
            }
            Some(ParsedUdp::V4 {
                src_ip: Ipv4Addr::new(pkt[12], pkt[13], pkt[14], pkt[15]),
                dst_ip: Ipv4Addr::new(pkt[16], pkt[17], pkt[18], pkt[19]),
                src_port: u16::from_be_bytes([pkt[ihl], pkt[ihl + 1]]),
                dst_port: u16::from_be_bytes([pkt[ihl + 2], pkt[ihl + 3]]),
                payload: &pkt[ihl + 8..],
            })
        }
        6 => {
            // Базовый 40-байтный заголовок IPv6 с Next Header = UDP (17).
            if pkt.len() < 48 || pkt[6] != 17 {
                return None;
            }
            let src: [u8; 16] = pkt[8..24].try_into().ok()?;
            let dst: [u8; 16] = pkt[24..40].try_into().ok()?;
            Some(ParsedUdp::V6 {
                src_ip: Ipv6Addr::from(src),
                dst_ip: Ipv6Addr::from(dst),
                src_port: u16::from_be_bytes([pkt[40], pkt[41]]),
                dst_port: u16::from_be_bytes([pkt[42], pkt[43]]),
                payload: &pkt[48..],
            })
        }
        _ => None,
    }
}

fn cache_get(r: &Running, key: &(String, u16)) -> Option<Vec<u8>> {
    let now = Instant::now();
    let mut cache = r.cache.lock().unwrap();
    if let Some(entry) = cache.get(key) {
        if entry.expires > now {
            return Some(entry.reply.clone());
        }
        cache.remove(key);
    }
    None
}

async fn handle_dns_packet(r: &Running, pkt: Vec<u8>, addr: Addr) {
    let passthrough = || {
        r.dll.send(&r.pass_out, &pkt, &addr);
    };
    let Some(parsed) = parse_udp_packet(&pkt) else {
        passthrough();
        return;
    };
    let dns_msg = match &parsed {
        ParsedUdp::V4 { payload, .. } | ParsedUdp::V6 { payload, .. } => *payload,
    };

    let Some((qname, qtype)) = parse_public_query(dns_msg) else {
        passthrough();
        return;
    };
    r.total.fetch_add(1, Ordering::Relaxed);

    let txid = [dns_msg[0], dns_msg[1]];
    let key = (qname, qtype);

    let mut reply = if let Some(ans) = cache_get(r, &key) {
        r.cached.fetch_add(1, Ordering::Relaxed);
        ans
    } else {
        // Singleflight: если такой же запрос уже в полёте, дожидаемся его результата в кэше.
        let waiter = {
            let mut inflight = r.inflight.lock().unwrap();
            if let Some(n) = inflight.get(&key) {
                Some(n.clone())
            } else {
                inflight.insert(key.clone(), Arc::new(Notify::new()));
                None
            }
        };
        if let Some(notify) = waiter {
            let _ = tokio::time::timeout(Duration::from_secs(4), notify.notified()).await;
            if let Some(ans) = cache_get(r, &key) {
                r.cached.fetch_add(1, Ordering::Relaxed);
                ans
            } else {
                // Первичный запрос не закэшировал ответ — выполним самостоятельно под семафором.
                let Ok(_permit) = r.sem.clone().acquire_owned().await else {
                    r.fallback.fetch_add(1, Ordering::Relaxed);
                    passthrough();
                    return;
                };
                match resolve_doh_wire(dns_msg, &r.endpoints).await {
                    Some((ans, ep)) => {
                        r.doh_ok.fetch_add(1, Ordering::Relaxed);
                        *r.upstream.lock().unwrap() = ep;
                        ans
                    }
                    None => {
                        r.fallback.fetch_add(1, Ordering::Relaxed);
                        passthrough();
                        return;
                    }
                }
            }
        } else {
            let res = match r.sem.clone().acquire_owned().await {
                Ok(_permit) => resolve_doh_wire(dns_msg, &r.endpoints).await,
                Err(_) => None,
            };
            let result_ans = match res {
                Some((ans, ep)) => {
                    r.doh_ok.fetch_add(1, Ordering::Relaxed);
                    *r.upstream.lock().unwrap() = ep;
                    if let Some(ttl) = response_cache_ttl(&ans) {
                        let now = Instant::now();
                        let mut cache = r.cache.lock().unwrap();
                        if cache.len() >= CACHE_MAX {
                            cache.retain(|_, e| e.expires > now);
                            if cache.len() >= CACHE_MAX {
                                cache.clear();
                            }
                        }
                        cache.insert(key.clone(), CacheEntry { reply: ans.clone(), expires: now + ttl });
                    }
                    Some(ans)
                }
                None => None,
            };
            if let Some(n) = r.inflight.lock().unwrap().remove(&key) {
                n.notify_waiters();
            }
            match result_ans {
                Some(ans) => ans,
                None => {
                    r.fallback.fetch_add(1, Ordering::Relaxed);
                    passthrough();
                    return;
                }
            }
        }
    };

    reply[0] = txid[0];
    reply[1] = txid[1];

    let (in_pkt, is_v6) = match parsed {
        ParsedUdp::V4 { src_ip, dst_ip, src_port, dst_port, .. } => {
            (build_udp_reply_v4(dst_ip, src_ip, dst_port, src_port, &reply), false)
        }
        ParsedUdp::V6 { src_ip, dst_ip, src_port, dst_port, .. } => {
            (build_udp_reply_v6(dst_ip, src_ip, dst_port, src_port, &reply), true)
        }
    };

    if let Some(in_pkt) = in_pkt {
        let mut flags = ADDR_CHECKSUMS_OK & !(ADDR_OUTBOUND | ADDR_LOOPBACK);
        if is_v6 {
            flags |= ADDR_IPV6;
        }
        let in_addr = Addr {
            flags,
            if_idx: addr.if_idx,
            sub_if_idx: addr.sub_if_idx,
            ..Addr::default()
        };
        if !r.dll.send(&r.inject_in, &in_pkt, &in_addr) {
            r.fallback.fetch_add(1, Ordering::Relaxed);
            passthrough();
        }
    } else {
        r.fallback.fetch_add(1, Ordering::Relaxed);
        passthrough();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_public_and_skips_local_queries() {
        let mut q = vec![0xab, 0xcd, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        q.extend_from_slice(b"\x07youtube\x03com\x00\x00\x01\x00\x01");
        assert_eq!(parse_public_query(&q), Some(("youtube.com".into(), 1)));

        let mut loc = vec![0x11, 0x22, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        loc.extend_from_slice(b"\x07printer\x05local\x00\x00\x01\x00\x01");
        assert_eq!(parse_public_query(&loc), None);
    }

    #[test]
    fn builds_valid_udp_reply_v4_checksums() {
        let pkt = build_udp_reply_v4(
            Ipv4Addr::new(8, 8, 8, 8),
            Ipv4Addr::new(192, 168, 1, 10),
            53,
            54321,
            b"\xab\xcd\x81\x80\x00\x01\x00\x00\x00\x00\x00\x00",
        )
        .unwrap();
        assert_eq!(fold(sum16(&pkt[..20], 0)), 0);
        let udp_len = pkt.len() - 20;
        let pseudo = sum16(&pkt[12..20], 0) + 17 + udp_len as u32;
        assert_eq!(fold(sum16(&pkt[20..], pseudo)), 0);
    }

    #[test]
    fn builds_valid_udp_reply_v6_checksums() {
        let pkt = build_udp_reply_v6(
            "2606:4700:4700::1111".parse().unwrap(),
            "2001:db8::1".parse().unwrap(),
            53,
            54321,
            b"\xab\xcd\x81\x80\x00\x01\x00\x00\x00\x00\x00\x00",
        )
        .unwrap();
        assert_eq!(pkt[0] >> 4, 6);
        assert_eq!(pkt[6], 17);
        let udp_len = pkt.len() - 40;
        let pseudo = sum16(&pkt[8..40], 0) + 17 + udp_len as u32;
        assert_eq!(fold(sum16(&pkt[40..], pseudo)), 0);
    }

    #[test]
    fn response_ttl_respects_rcode_and_answer_ttl() {
        // NOERROR with 1 question (youtube.com) and 1 A answer with TTL=45s
        let mut ok = vec![0x00, 0x01, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        ok.extend_from_slice(b"\x07youtube\x03com\x00\x00\x01\x00\x01");
        ok.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 45, 0, 4, 142, 250, 74, 14]);
        assert_eq!(response_cache_ttl(&ok), Some(Duration::from_secs(45)));
        assert_eq!(extract_a_records(&ok), vec![IpAddr::V4(Ipv4Addr::new(142, 250, 74, 14))]);

        // SERVFAIL (rcode = 2) must not be cached
        let mut servfail = ok.clone();
        servfail[3] = 0x82;
        assert_eq!(response_cache_ttl(&servfail), None);

        // NXDOMAIN (rcode = 3) cached for short TTL
        let mut nx = ok;
        nx[3] = 0x83;
        assert_eq!(response_cache_ttl(&nx), Some(Duration::from_secs(CACHE_NXDOMAIN_TTL as u64)));
    }

    #[test]
    fn doh_provider_endpoints() {
        assert_eq!(doh_endpoints("quad9", "")[0], "https://9.9.9.9/dns-query");
        assert_eq!(doh_endpoints("adguard", "")[0], "https://94.140.14.14/dns-query");
        assert_eq!(doh_endpoints("custom", "https://dns.example/dns-query")[0], "https://dns.example/dns-query");
        assert_eq!(doh_endpoints("custom", "http://insecure")[0], "https://1.1.1.1/dns-query");
    }
}

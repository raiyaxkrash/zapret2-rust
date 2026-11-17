//! Защита от подмены DNS провайдером (DNS-over-HTTPS поверх WinDivert).
//!
//! Перехватывает исходящие UDP-запросы на порт 53, отправляет их в зашифрованном виде
//! по стандарту RFC 8484 напрямую на 1.1.1.1 (резерв — 8.8.8.8) и возвращает программе
//! подлинный DNS-ответ. Локальные имена (.local, .lan и без точки) не перехватываются,
//! а при недоступности DoH пакет пропускается обычным путём, чтобы не ломать сеть.

use std::{
    collections::HashMap,
    ffi::c_void,
    net::Ipv4Addr,
    os::windows::ffi::OsStrExt,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

const PRIORITY_CAPTURE: i16 = 950;
/// Выше, чем у перехвата DNS в WARP (1000), чтобы WARP для программ тоже видел чистые ответы.
const PRIORITY_INJECT_IN: i16 = 1100;
const PRIORITY_PASSTHROUGH_OUT: i16 = 900;

const FLAG_DIVERT: u64 = 0;
const FLAG_SEND_ONLY: u64 = 0x8;
const SHUTDOWN_BOTH: i32 = 3;

const ADDR_OUTBOUND: u32 = 1 << 17;
const ADDR_LOOPBACK: u32 = 1 << 18;
const ADDR_CHECKSUMS_OK: u32 = (1 << 21) | (1 << 22) | (1 << 23);

const CACHE_TTL: Duration = Duration::from_secs(120);
const CACHE_MAX: usize = 512;

#[repr(C)]
#[derive(Clone, Copy)]
struct Addr {
    timestamp: i64,
    flags: u32,
    reserved: u32,
    if_idx: u32,
    sub_if_idx: u32,
    rest: [u8; 56],
}

type Handle = *mut c_void;
type Proc = unsafe extern "system" fn() -> isize;
type OpenFn = unsafe extern "C" fn(*const u8, i32, i16, u64) -> Handle;
type RecvFn = unsafe extern "C" fn(Handle, *mut u8, u32, *mut u32, *mut Addr) -> i32;
type SendFn = unsafe extern "C" fn(Handle, *const u8, u32, *mut u32, *const Addr) -> i32;
type CloseFn = unsafe extern "C" fn(Handle) -> i32;
type ShutdownFn = unsafe extern "C" fn(Handle, i32) -> i32;

struct Dll {
    open: OpenFn,
    recv: RecvFn,
    send: SendFn,
    close: CloseFn,
    shutdown: ShutdownFn,
}

struct H {
    h: Handle,
    close: CloseFn,
}

impl Drop for H {
    fn drop(&mut self) {
        unsafe { (self.close)(self.h) };
    }
}
unsafe impl Send for H {}
unsafe impl Sync for H {}

impl Dll {
    fn load(path: &Path) -> Result<Dll, String> {
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let lib = unsafe { LoadLibraryW(wide.as_ptr()) };
        if lib.is_null() {
            return Err(format!("Не удалось загрузить {}", path.display()));
        }
        let sym = |name: &[u8]| {
            unsafe { GetProcAddress(lib, name.as_ptr()) }.ok_or_else(|| "WinDivert.dll: нет нужной функции".to_string())
        };
        unsafe {
            Ok(Dll {
                open: std::mem::transmute::<Proc, OpenFn>(sym(b"WinDivertOpen\0")?),
                recv: std::mem::transmute::<Proc, RecvFn>(sym(b"WinDivertRecv\0")?),
                send: std::mem::transmute::<Proc, SendFn>(sym(b"WinDivertSend\0")?),
                close: std::mem::transmute::<Proc, CloseFn>(sym(b"WinDivertClose\0")?),
                shutdown: std::mem::transmute::<Proc, ShutdownFn>(sym(b"WinDivertShutdown\0")?),
            })
        }
    }

    fn open(&self, filter: &str, priority: i16, flags: u64) -> Result<H, String> {
        let c = format!("{filter}\0");
        let h = unsafe { (self.open)(c.as_ptr(), 0, priority, flags) };
        if h.is_null() || h as isize == -1 {
            return Err(format!("WinDivert (DNS): ошибка {}", std::io::Error::last_os_error()));
        }
        Ok(H { h, close: self.close })
    }

    fn recv(&self, h: &H, buf: &mut [u8], addr: &mut Addr) -> Option<usize> {
        let mut n = 0u32;
        let ok = unsafe { (self.recv)(h.h, buf.as_mut_ptr(), buf.len() as u32, &mut n, addr) };
        (ok != 0).then_some(n as usize)
    }

    fn send(&self, h: &H, pkt: &[u8], addr: &Addr) -> bool {
        let mut n = 0u32;
        unsafe { (self.send)(h.h, pkt.as_ptr(), pkt.len() as u32, &mut n, addr) != 0 }
    }

    fn shutdown(&self, h: &H) {
        unsafe { (self.shutdown)(h.h, SHUTDOWN_BOTH) };
    }
}

fn sum16(data: &[u8], mut acc: u32) -> u32 {
    let (pairs, rest) = data.as_chunks::<2>();
    for c in pairs {
        acc += u16::from_be_bytes(*c) as u32;
    }
    if let [b] = rest {
        acc += (*b as u32) << 8;
    }
    acc
}

fn fold(mut acc: u32) -> u16 {
    while acc >> 16 != 0 {
        acc = (acc & 0xffff) + (acc >> 16);
    }
    !(acc as u16)
}

/// Строит входящий IPv4+UDP пакет с DNS-ответом и верными контрольными суммами.
fn build_udp_reply(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, src_port: u16, dst_port: u16, payload: &[u8]) -> Option<Vec<u8>> {
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

async fn resolve_doh_wire(query: &[u8]) -> Option<Vec<u8>> {
    let client = doh_client()?;
    let mut q = query.to_vec();
    if q.len() < 12 {
        return None;
    }
    q[0] = 0;
    q[1] = 0;
    // Обращаемся напрямую по IP (сертификаты 1.1.1.1 и 8.8.8.8 подписаны на эти IP),
    // чтобы запрос DoH сам не зависел от системного DNS и не уходил в рекурсию.
    for endpoint in ["https://1.1.1.1/dns-query", "https://8.8.8.8/dns-query"] {
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
                return Some(bytes.to_vec());
            }
        }
    }
    None
}

type DnsCache = HashMap<(String, u16), (Vec<u8>, Instant)>;

struct Running {
    stop: AtomicBool,
    dll: Dll,
    capture: Arc<H>,
    inject_in: H,
    pass_out: H,
    cache: Mutex<DnsCache>,
}

#[derive(Default)]
pub struct DnsGuard {
    state: Mutex<Option<Arc<Running>>>,
}

impl DnsGuard {
    pub fn is_running(&self) -> bool {
        self.state.lock().unwrap().is_some()
    }

    pub fn start(&self, engine_bin: &Path) -> Result<(), String> {
        self.stop();
        let dll_path = engine_bin.join("WinDivert.dll");
        if !dll_path.exists() || !engine_bin.join("WinDivert64.sys").exists() {
            return Err("Защита DNS: файлы zapret2 (WinDivert) ещё не загружены".into());
        }
        let dll = Dll::load(&dll_path)?;
        let capture = Arc::new(dll.open("outbound and !loopback and ip and udp.DstPort == 53", PRIORITY_CAPTURE, FLAG_DIVERT)?);
        let inject_in = dll.open("false", PRIORITY_INJECT_IN, FLAG_SEND_ONLY)?;
        let pass_out = dll.open("false", PRIORITY_PASSTHROUGH_OUT, FLAG_SEND_ONLY)?;

        let running = Arc::new(Running {
            stop: AtomicBool::new(false),
            dll,
            capture: capture.clone(),
            inject_in,
            pass_out,
            cache: Mutex::new(HashMap::new()),
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

fn recv_loop(r: Arc<Running>, capture: Arc<H>) {
    let mut buf = vec![0u8; 65_535];
    let mut addr = Addr { timestamp: 0, flags: 0, reserved: 0, if_idx: 0, sub_if_idx: 0, rest: [0; 56] };
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

async fn handle_dns_packet(r: &Running, pkt: Vec<u8>, addr: Addr) {
    let passthrough = || {
        r.dll.send(&r.pass_out, &pkt, &addr);
    };
    if pkt.len() < 28 || pkt[0] >> 4 != 4 || pkt[9] != 17 {
        passthrough();
        return;
    }
    let ihl = ((pkt[0] & 0x0f) as usize) * 4;
    if ihl < 20 || pkt.len() < ihl + 8 {
        passthrough();
        return;
    }
    let src_ip = Ipv4Addr::new(pkt[12], pkt[13], pkt[14], pkt[15]);
    let dst_ip = Ipv4Addr::new(pkt[16], pkt[17], pkt[18], pkt[19]);
    let src_port = u16::from_be_bytes([pkt[ihl], pkt[ihl + 1]]);
    let dst_port = u16::from_be_bytes([pkt[ihl + 2], pkt[ihl + 3]]);
    let dns_msg = &pkt[ihl + 8..];

    let Some((qname, qtype)) = parse_public_query(dns_msg) else {
        passthrough();
        return;
    };

    let txid = [dns_msg[0], dns_msg[1]];
    let key = (qname, qtype);
    let cached = {
        let mut cache = r.cache.lock().unwrap();
        cache.get(&key).and_then(|(ans, at)| (at.elapsed() < CACHE_TTL).then(|| ans.clone())).or_else(|| {
            cache.remove(&key);
            None
        })
    };

    let mut reply = match cached {
        Some(ans) => ans,
        None => match resolve_doh_wire(dns_msg).await {
            Some(ans) => {
                let mut cache = r.cache.lock().unwrap();
                if cache.len() >= CACHE_MAX {
                    cache.retain(|_, (_, at)| at.elapsed() < CACHE_TTL);
                    if cache.len() >= CACHE_MAX {
                        cache.clear();
                    }
                }
                cache.insert(key, (ans.clone(), Instant::now()));
                ans
            }
            None => {
                passthrough();
                return;
            }
        },
    };

    reply[0] = txid[0];
    reply[1] = txid[1];

    if let Some(in_pkt) = build_udp_reply(dst_ip, src_ip, dst_port, src_port, &reply) {
        let in_addr = Addr {
            timestamp: 0,
            flags: ADDR_CHECKSUMS_OK & !(ADDR_OUTBOUND | ADDR_LOOPBACK),
            reserved: 0,
            if_idx: addr.if_idx,
            sub_if_idx: addr.sub_if_idx,
            rest: [0; 56],
        };
        if !r.dll.send(&r.inject_in, &in_pkt, &in_addr) {
            passthrough();
        }
    } else {
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
    fn builds_valid_udp_reply_checksums() {
        let pkt = build_udp_reply(
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
}

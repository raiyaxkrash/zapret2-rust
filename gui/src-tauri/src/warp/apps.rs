//! WARP для программ, которые не слушают системный прокси, и маршрутизация выбранных .exe.
//! Пакеты перехватываются драйвером WinDivert и идут прямо в WireGuard-туннель (TCP и UDP,
//! кроме QUIC на 443, который отбрасывается для быстрого перехода HTTPS на TCP).

use std::{
    collections::HashMap,
    net::{Ipv4Addr, Ipv6Addr},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

use crate::windivert::{
    fold, sum16, Addr, Dll, HandleGuard as H, ADDR_CHECKSUMS_OK, ADDR_IPV6, ADDR_LOOPBACK, ADDR_OUTBOUND, FLAG_DIVERT,
    FLAG_SEND_ONLY, FLAG_SNIFF_RECV_ONLY, LAYER_SOCKET, PRIORITY_WARP_APPS as PRIORITY,
};

use super::{proxy::Domains, tunnel::Tunnel, Shared};

pub type Exes = Arc<RwLock<Vec<String>>>;

const MSS: u16 = 1200;
const FILTER_BUDGET: usize = 200;
const FLOW_IDLE: Duration = Duration::from_secs(600);

// ---------------------------------------------------------------- пакеты

fn ipv4(p: &[u8]) -> Option<(usize, u8, Ipv4Addr, Ipv4Addr)> {
    if p.len() < 20 || p[0] >> 4 != 4 {
        return None;
    }
    let ihl = ((p[0] & 0x0f) as usize) * 4;
    if ihl < 20 || p.len() < ihl {
        return None;
    }
    Some((ihl, p[9], Ipv4Addr::new(p[12], p[13], p[14], p[15]), Ipv4Addr::new(p[16], p[17], p[18], p[19])))
}

fn fix_checksums(p: &mut [u8]) {
    let Some((ihl, proto, _, _)) = ipv4(p) else { return };
    let total = (u16::from_be_bytes([p[2], p[3]]) as usize).min(p.len());
    p[10] = 0;
    p[11] = 0;
    let c = fold(sum16(&p[..ihl], 0));
    p[10..12].copy_from_slice(&c.to_be_bytes());
    let off = match proto {
        6 => 16,
        17 => 6,
        _ => return,
    };
    if total < ihl + off + 2 {
        return;
    }
    p[ihl + off] = 0;
    p[ihl + off + 1] = 0;
    let len = (total - ihl) as u32;
    let pseudo = sum16(&p[12..20], 0) + proto as u32 + len;
    let mut c = fold(sum16(&p[ihl..total], pseudo));
    if proto == 17 && c == 0 {
        c = 0xffff;
    }
    p[ihl + off..ihl + off + 2].copy_from_slice(&c.to_be_bytes());
}

fn clamp_mss(p: &mut [u8], ihl: usize) {
    if p.len() < ihl + 20 || p[ihl + 13] & 0x02 == 0 {
        return;
    }
    let doff = ((p[ihl + 12] >> 4) as usize) * 4;
    let end = (ihl + doff).min(p.len());
    let mut i = ihl + 20;
    while i < end {
        match p[i] {
            0 => break,
            1 => i += 1,
            kind => {
                let len = *p.get(i + 1).unwrap_or(&0) as usize;
                if len < 2 || i + len > end {
                    break;
                }
                if kind == 2 && len == 4 {
                    let mss = u16::from_be_bytes([p[i + 2], p[i + 3]]);
                    if mss > MSS {
                        p[i + 2..i + 4].copy_from_slice(&MSS.to_be_bytes());
                    }
                }
                i += len;
            }
        }
    }
}

// ---------------------------------------------------------------- DNS и процессы

fn dns_name(msg: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut end = None;
    for _ in 0..64 {
        let len = *msg.get(pos)? as usize;
        if len == 0 {
            return Some((name, end.unwrap_or(pos + 1)));
        }
        if len & 0xc0 == 0xc0 {
            let ptr = ((len & 0x3f) << 8) | *msg.get(pos + 1)? as usize;
            end.get_or_insert(pos + 2);
            pos = ptr;
            continue;
        }
        let label = msg.get(pos + 1..pos + 1 + len)?;
        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(&String::from_utf8_lossy(label).to_ascii_lowercase());
        pos += 1 + len;
    }
    None
}

fn parse_dns(msg: &[u8]) -> Option<(String, Vec<Ipv4Addr>, Vec<Ipv6Addr>)> {
    if msg.len() < 12 || msg[2] & 0x80 == 0 {
        return None;
    }
    let qd = u16::from_be_bytes([msg[4], msg[5]]);
    let an = u16::from_be_bytes([msg[6], msg[7]]);
    if qd != 1 {
        return None;
    }
    let (qname, mut pos) = dns_name(msg, 12)?;
    pos += 4;
    let (mut v4, mut v6) = (Vec::new(), Vec::new());
    for _ in 0..an {
        let (_, p) = dns_name(msg, pos)?;
        let h = msg.get(p..p + 10)?;
        let ty = u16::from_be_bytes([h[0], h[1]]);
        let len = u16::from_be_bytes([h[8], h[9]]) as usize;
        let data = msg.get(p + 10..p + 10 + len)?;
        match (ty, len) {
            (1, 4) => v4.push(Ipv4Addr::new(data[0], data[1], data[2], data[3])),
            (28, 16) => v6.push(Ipv6Addr::from(<[u8; 16]>::try_from(data).ok()?)),
            _ => {}
        }
        pos = p + 10 + len;
    }
    Some((qname, v4, v6))
}

fn matches(name: &str, domains: &[String]) -> bool {
    let name = name.trim_end_matches('.');
    domains.iter().any(|d| name == d || name.strip_suffix(d.as_str()).map(|r| r.ends_with('.')).unwrap_or(false))
}

fn pid_exe_name(pid: u32) -> Option<String> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        full.rsplit(['\\', '/']).next().map(|s| s.to_ascii_lowercase())
    }
}

fn socket_event_ipv4(addr: &Addr) -> Option<(u32, Ipv4Addr)> {
    if addr.flags & ADDR_IPV6 != 0 {
        return None;
    }
    let pid = u32::from_ne_bytes(addr.rest[16..20].try_into().ok()?);
    let remote_host_order = u32::from_ne_bytes(addr.rest[36..40].try_into().ok()?);
    let ip = Ipv4Addr::from(remote_host_order);
    (!ip.is_unspecified() && !ip.is_loopback() && !ip.is_private() && !ip.is_link_local()).then_some((pid, ip))
}

// ---------------------------------------------------------------- фильтр

#[derive(Default)]
struct IpSet {
    v4: Vec<Ipv4Addr>,
    v6: Vec<Ipv6Addr>,
}

impl IpSet {
    fn add(&mut self, v4: &[Ipv4Addr], v6: &[Ipv6Addr]) -> bool {
        let mut changed = false;
        for a in v4 {
            if !a.is_loopback() && !a.is_private() && !a.is_unspecified() && !a.is_link_local() && !self.v4.contains(a) {
                self.v4.push(*a);
                changed = true;
            }
        }
        for a in v6 {
            if !a.is_loopback() && !self.v6.contains(a) {
                self.v6.push(*a);
                changed = true;
            }
        }
        if self.v4.len() > 600 {
            self.v4.drain(..self.v4.len() - 600);
        }
        if self.v6.len() > 60 {
            self.v6.drain(..self.v6.len() - 60);
        }
        changed
    }
}

fn build_filter(set: &IpSet, exclude: Option<Ipv4Addr>) -> (Option<String>, usize) {
    let mut groups: Vec<([u8; 3], Vec<Ipv4Addr>)> = Vec::new();
    for a in set.v4.iter().rev() {
        let k = [a.octets()[0], a.octets()[1], a.octets()[2]];
        match groups.iter_mut().find(|g| g.0 == k) {
            Some(g) => g.1.push(*a),
            None => groups.push((k, vec![*a])),
        }
    }
    let mut budget = FILTER_BUDGET;
    let mut v4 = Vec::new();
    let mut truncated = 0usize;
    for (k, ips) in groups {
        let term = if ips.len() > 1 {
            format!("(ip.DstAddr >= {}.{}.{}.0 and ip.DstAddr <= {}.{}.{}.255)", k[0], k[1], k[2], k[0], k[1], k[2])
        } else {
            format!("ip.DstAddr == {}", ips[0])
        };
        let cost = if ips.len() > 1 { 2 } else { 1 };
        if cost > budget {
            truncated += ips.len();
            continue;
        }
        budget -= cost;
        v4.push(term);
    }
    let v6_limit = budget.min(40);
    if set.v6.len() > v6_limit {
        truncated += set.v6.len() - v6_limit;
    }
    let v6: Vec<String> = set.v6.iter().rev().take(v6_limit).map(|a| format!("ipv6.DstAddr == {a}")).collect();
    if v4.is_empty() && v6.is_empty() {
        return (None, 0);
    }
    let mut parts = Vec::new();
    if !v4.is_empty() {
        let excl = exclude.map(|e| format!(" and ip.DstAddr != {e}")).unwrap_or_default();
        parts.push(format!("(ip and (tcp or udp.DstPort != 53){excl} and ({}))", v4.join(" or ")));
    }
    if !v6.is_empty() {
        parts.push(format!("(ipv6 and (tcp or udp) and ({}))", v6.join(" or ")));
    }
    (Some(format!("outbound and !loopback and ({})", parts.join(" or "))), truncated)
}

// ---------------------------------------------------------------- перенаправление

struct Flow {
    src: Ipv4Addr,
    if_idx: u32,
    sub_if_idx: u32,
    seen: Instant,
}

struct Inner {
    dll: Dll,
    tunnel: Shared,
    domains: Domains,
    exes: Exes,
    stop: AtomicBool,
    send: Mutex<Option<H>>,
    dns: Mutex<Option<Arc<H>>>,
    sock: Mutex<Option<Arc<H>>>,
    capture: Mutex<Option<(String, Arc<H>)>>,
    ips: Mutex<IpSet>,
    flows: Mutex<HashMap<(u8, u16), Flow>>,
    log: Box<dyn Fn(String) + Send + Sync>,
}

pub struct Redirector(Arc<Inner>);

impl Redirector {
    pub fn start(
        engine_bin: &Path,
        tunnel: Shared,
        domains: Domains,
        exes: Exes,
        log: impl Fn(String) + Send + Sync + 'static,
    ) -> Result<Redirector, String> {
        let dll_path = engine_bin.join("WinDivert.dll");
        if !dll_path.exists() || !engine_bin.join("WinDivert64.sys").exists() {
            return Err("WARP для программ: zapret2 ещё не загружен".into());
        }
        let dll = Dll::load(&dll_path)?;
        let send = dll.open("false", PRIORITY, FLAG_SEND_ONLY, "WinDivert")?;
        let dns = Arc::new(dll.open("inbound and !loopback and udp.SrcPort == 53", PRIORITY, FLAG_DIVERT, "WinDivert")?);
        let sock = dll
            .open_layer("outbound and !loopback and (tcp or udp.DstPort != 53)", LAYER_SOCKET, 0, FLAG_SNIFF_RECV_ONLY, "WinDivert")
            .ok()
            .map(Arc::new);
        let inner = Arc::new(Inner {
            dll,
            tunnel,
            domains,
            exes,
            stop: AtomicBool::new(false),
            send: Mutex::new(Some(send)),
            dns: Mutex::new(Some(dns.clone())),
            sock: Mutex::new(sock.clone()),
            capture: Mutex::new(None),
            ips: Mutex::new(IpSet::default()),
            flows: Mutex::new(HashMap::new()),
            log: Box::new(log),
        });
        let i = inner.clone();
        std::thread::spawn(move || dns_loop(&i, dns));
        if let Some(sh) = sock {
            let i2 = inner.clone();
            std::thread::spawn(move || socket_loop(&i2, sh));
        }
        let r = Redirector(inner);
        r.reseed();
        Ok(r)
    }

    pub fn reseed(&self) {
        *self.0.ips.lock().unwrap() = IpSet::default();
        reopen(&self.0);
        let inner = self.0.clone();
        std::thread::spawn(move || {
            let _ = crate::util::hidden_command("ipconfig").arg("/flushdns").output();
            let domains = inner.domains.read().unwrap().clone();
            let mut v4 = Vec::new();
            for d in &domains {
                for host in [d.clone(), format!("www.{d}")] {
                    if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), 443)) {
                        v4.extend(addrs.filter_map(|a| match a.ip() {
                            std::net::IpAddr::V4(v) => Some(v),
                            _ => None,
                        }));
                    }
                }
            }
            if inner.ips.lock().unwrap().add(&v4, &[]) {
                reopen(&inner);
            }
        });
    }

    pub fn stop(&self) {
        let i = &self.0;
        i.stop.store(true, Ordering::Relaxed);
        if let Some(t) = i.tunnel.read().unwrap().as_ref() {
            t.set_raw_hook(None);
        }
        if let Some(h) = i.dns.lock().unwrap().take() {
            i.dll.shutdown(&h);
        }
        if let Some(h) = i.sock.lock().unwrap().take() {
            i.dll.shutdown(&h);
        }
        if let Some((_, h)) = i.capture.lock().unwrap().take() {
            i.dll.shutdown(&h);
        }
        i.send.lock().unwrap().take();
    }
}

impl Drop for Redirector {
    fn drop(&mut self) {
        self.stop();
    }
}

fn reopen(inner: &Arc<Inner>) {
    if inner.stop.load(Ordering::Relaxed) {
        return;
    }
    let exclude = inner.tunnel.read().unwrap().as_ref().and_then(|t| match t.endpoint.ip() {
        std::net::IpAddr::V4(v) => Some(v),
        _ => None,
    });
    let (filter, truncated) = build_filter(&inner.ips.lock().unwrap(), exclude);
    let mut slot = inner.capture.lock().unwrap();
    if slot.as_ref().map(|(f, _)| Some(f) == filter.as_ref()).unwrap_or(filter.is_none()) {
        return;
    }
    if truncated > 0 {
        (inner.log)(format!(
            "WARP для программ: превышен лимит фильтра WinDivert, {truncated} старых IP-адресов пропущено"
        ));
    }
    let new = match filter {
        Some(f) => match inner.dll.open(&f, PRIORITY, FLAG_DIVERT, "WinDivert") {
            Ok(h) => {
                let h = Arc::new(h);
                let (i, h2) = (inner.clone(), h.clone());
                std::thread::spawn(move || capture_loop(&i, h2));
                Some((f, h))
            }
            Err(e) => {
                (inner.log)(format!("WARP для программ: {e}"));
                return;
            }
        },
        None => None,
    };
    if let Some((_, old)) = std::mem::replace(&mut *slot, new) {
        inner.dll.shutdown(&old);
    }
}

fn dns_loop(inner: &Arc<Inner>, h: Arc<H>) {
    let mut buf = vec![0u8; 65_535];
    let mut addr = Addr { timestamp: 0, flags: 0, reserved: 0, if_idx: 0, sub_if_idx: 0, rest: [0; 56] };
    while let Some(n) = inner.dll.recv(&h, &mut buf, &mut addr) {
        let pkt = &buf[..n];
        if let Some((ihl, 17, _, _)) = ipv4(pkt) {
            if let Some((name, v4, v6)) = pkt.get(ihl + 8..).and_then(parse_dns) {
                if (!v4.is_empty() || !v6.is_empty()) && matches(&name, &inner.domains.read().unwrap()) && inner.ips.lock().unwrap().add(&v4, &v6) {
                    reopen(inner);
                }
            }
        }
        inner.dll.send(&h, pkt, &addr);
    }
}

/// Следит за новыми исходящими сокетами и автоматически направляет соединения выбранных .exe в WARP.
fn socket_loop(inner: &Arc<Inner>, h: Arc<H>) {
    let mut buf = [0u8; 64];
    let mut addr = Addr { timestamp: 0, flags: 0, reserved: 0, if_idx: 0, sub_if_idx: 0, rest: [0; 56] };
    let mut pid_cache: HashMap<u32, (String, Instant)> = HashMap::new();
    while inner.dll.recv(&h, &mut buf, &mut addr).is_some() {
        let targets = inner.exes.read().unwrap().clone();
        if targets.is_empty() {
            continue;
        }
        let Some((pid, dst_ip)) = socket_event_ipv4(&addr) else { continue };
        let exe = match pid_cache.get(&pid) {
            Some((name, at)) if at.elapsed() < Duration::from_secs(30) => name.clone(),
            _ => {
                let Some(name) = pid_exe_name(pid) else { continue };
                if pid_cache.len() > 256 {
                    pid_cache.retain(|_, (_, at)| at.elapsed() < Duration::from_secs(30));
                }
                pid_cache.insert(pid, (name.clone(), Instant::now()));
                name
            }
        };
        if targets.iter().any(|t| t.eq_ignore_ascii_case(&exe)) && inner.ips.lock().unwrap().add(&[dst_ip], &[]) {
            reopen(inner);
        }
    }
}

/// Исходящие пакеты программ (TCP и игровой/голосовой UDP) → туннель WARP.
fn capture_loop(inner: &Arc<Inner>, h: Arc<H>) {
    let mut buf = vec![0u8; 65_535];
    let mut addr = Addr { timestamp: 0, flags: 0, reserved: 0, if_idx: 0, sub_if_idx: 0, rest: [0; 56] };
    let mut hooked: usize = 0;
    let mut cleaned = Instant::now();
    while let Some(n) = inner.dll.recv(&h, &mut buf, &mut addr) {
        if addr.flags & ADDR_IPV6 != 0 {
            continue;
        }
        let pkt = &mut buf[..n];
        let Some((ihl, proto, src, _)) = ipv4(pkt) else { continue };
        match proto {
            6 if pkt.len() >= ihl + 20 => {}
            17 if pkt.len() >= ihl + 8 => {
                let dport = u16::from_be_bytes([pkt[ihl + 2], pkt[ihl + 3]]);
                // QUIC (UDP 443) отбрасываем, чтобы HTTPS сразу перешёл на быстрый TCP.
                if dport == 443 || dport == 53 {
                    continue;
                }
            }
            _ => continue,
        }
        let Some(t) = inner.tunnel.read().unwrap().clone() else { continue };
        let ptr = Arc::as_ptr(&t) as usize;
        if ptr != hooked {
            install_hook(inner, &t);
            hooked = ptr;
        }
        let sport = u16::from_be_bytes([pkt[ihl], pkt[ihl + 1]]);
        {
            let mut flows = inner.flows.lock().unwrap();
            flows.insert((proto, sport), Flow { src, if_idx: addr.if_idx, sub_if_idx: addr.sub_if_idx, seen: Instant::now() });
            if cleaned.elapsed() > Duration::from_secs(60) {
                flows.retain(|_, f| f.seen.elapsed() < FLOW_IDLE);
                cleaned = Instant::now();
            }
        }
        pkt[12..16].copy_from_slice(&t.local.octets());
        if proto == 6 {
            clamp_mss(pkt, ihl);
        }
        fix_checksums(pkt);
        t.send_raw(pkt.to_vec());
    }
}

/// Ответы из туннеля (TCP и UDP) → обратно в Windows с прежним адресом.
fn install_hook(inner: &Arc<Inner>, t: &Arc<Tunnel>) {
    let weak = Arc::downgrade(inner);
    let local = t.local;
    t.set_raw_hook(Some(Arc::new(move |p: &[u8]| {
        let Some(inner) = weak.upgrade() else { return false };
        let Some((ihl, proto, _, dst)) = ipv4(p) else { return false };
        let min_hdr = match proto {
            6 => 20,
            17 => 8,
            _ => return false,
        };
        if dst != local || p.len() < ihl + min_hdr {
            return false;
        }
        let dport = u16::from_be_bytes([p[ihl + 2], p[ihl + 3]]);
        let (src, if_idx, sub_if_idx) = {
            let mut flows = inner.flows.lock().unwrap();
            let Some(f) = flows.get_mut(&(proto, dport)) else { return false };
            f.seen = Instant::now();
            (f.src, f.if_idx, f.sub_if_idx)
        };
        let mut pkt = p.to_vec();
        pkt[16..20].copy_from_slice(&src.octets());
        if proto == 6 {
            clamp_mss(&mut pkt, ihl);
        }
        fix_checksums(&mut pkt);
        let addr = Addr {
            timestamp: 0,
            flags: ADDR_CHECKSUMS_OK & !(ADDR_OUTBOUND | ADDR_LOOPBACK),
            reserved: 0,
            if_idx,
            sub_if_idx,
            rest: [0; 56],
        };
        if let Some(h) = inner.send.lock().unwrap().as_ref() {
            inner.dll.send(h, &pkt, &addr);
        }
        true
    })));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_reference() {
        let mut p = vec![
            0x45, 0, 0, 44, 0, 1, 0x40, 0, 64, 6, 0, 0, 10, 0, 0, 2, 1, 1, 1, 1,
            0xc3, 0x50, 0x01, 0xbb, 0, 0, 0, 1, 0, 0, 0, 0, 0x60, 0x02, 0xff, 0xff, 0, 0, 0, 0,
            2, 4, 0x05, 0xb4,
        ];
        clamp_mss(&mut p, 20);
        assert_eq!(u16::from_be_bytes([p[42], p[43]]), MSS);
        fix_checksums(&mut p);
        assert_eq!(fold(sum16(&p[..20], 0)), 0);
        let pseudo = sum16(&p[12..20], 0) + 6 + 24;
        assert_eq!(fold(sum16(&p[20..], pseudo)), 0);
    }

    #[test]
    fn dns_answer_with_cname() {
        let mut m = vec![0x12, 0x34, 0x81, 0x80, 0, 1, 0, 3, 0, 0, 0, 0];
        m.extend_from_slice(b"\x07chatgpt\x03com\x00\x00\x01\x00\x01");
        m.extend_from_slice(&[0xc0, 12, 0, 5, 0, 1, 0, 0, 0, 60, 0, 6]);
        m.extend_from_slice(b"\x03cdn\xc0\x14");
        for last in [1u8, 2] {
            m.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 104, 18, 32, last]);
        }
        let (name, v4, v6) = parse_dns(&m).unwrap();
        assert_eq!(name, "chatgpt.com");
        assert_eq!(v4, vec![Ipv4Addr::new(104, 18, 32, 1), Ipv4Addr::new(104, 18, 32, 2)]);
        assert!(v6.is_empty());
        assert!(matches("api.chatgpt.com.", &["chatgpt.com".to_string()]));
        assert!(!matches("notchatgpt.com", &["chatgpt.com".to_string()]));
    }

    #[test]
    fn filter_groups_subnets() {
        let mut s = IpSet::default();
        s.add(&[Ipv4Addr::new(104, 18, 32, 1), Ipv4Addr::new(104, 18, 32, 2), Ipv4Addr::new(35, 186, 224, 25), Ipv4Addr::new(192, 168, 1, 1)], &[]);
        let (f, truncated) = build_filter(&s, Some(Ipv4Addr::new(162, 159, 192, 1)));
        let f = f.unwrap();
        assert_eq!(truncated, 0);
        assert!(f.contains("ip.DstAddr >= 104.18.32.0 and ip.DstAddr <= 104.18.32.255"), "{f}");
        assert!(f.contains("ip.DstAddr == 35.186.224.25"));
        assert!(!f.contains("192.168"));
        assert!(f.contains("ip.DstAddr != 162.159.192.1"));
        assert!(f.contains("udp.DstPort != 53"));
        assert!(build_filter(&IpSet::default(), None).0.is_none());
    }
}

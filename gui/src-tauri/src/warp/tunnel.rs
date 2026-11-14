//! Туннель WARP в пространстве пользователя: WireGuard (boringtun) поверх UDP
//! и сетевой стек smoltcp, через который локальный прокси открывает TCP-соединения.
//! Драйверы и изменение маршрутов Windows не нужны.

use std::{
    collections::{HashMap, VecDeque},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use boringtun::noise::{Tunn, TunnResult};
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    socket::tcp,
    time::Instant as SInstant,
    wire::{HardwareAddress, IpAddress, IpCidr, Ipv4Address},
};
use tokio::{
    net::UdpSocket,
    sync::{mpsc, oneshot, Notify},
};

use super::api::Account;

const MTU: usize = 1280;
const TCP_BUF: usize = 256 * 1024;
const WG_BUF: usize = 65_536;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Точки подключения WARP: если основной порт блокируется, пробуются другие.
pub const ENDPOINTS: &[&str] = &[
    "162.159.192.1:2408",
    "162.159.192.1:500",
    "162.159.193.10:1701",
    "162.159.195.1:4500",
    "162.159.192.1:854",
    "162.159.193.10:894",
    "162.159.195.1:2408",
    "162.159.192.1:8854",
];

/// Соединение через туннель: байты от клиента и к клиенту.
pub struct Conn {
    pub tx: mpsc::Sender<Vec<u8>>,
    pub rx: mpsc::Receiver<Vec<u8>>,
}

enum Cmd {
    Connect { dst: SocketAddrV4, reply: oneshot::Sender<Result<Conn, String>> },
}

#[derive(Default)]
pub struct Stats {
    pub rx: AtomicU64,
    pub tx: AtomicU64,
    /// Когда последний раз пришёл пакет от сервера (unix-время).
    pub last_rx: AtomicU64,
    /// Цикл туннеля завершился (ошибка сети).
    pub dead: std::sync::atomic::AtomicBool,
}

pub struct Tunnel {
    cmd: mpsc::Sender<Cmd>,
    wake: Arc<Notify>,
    pub endpoint: SocketAddr,
    pub stats: Arc<Stats>,
}

impl Tunnel {
    /// Открывает TCP-соединение к адресу через WARP. Ждёт установления соединения.
    pub async fn connect(&self, dst: SocketAddrV4) -> Result<Conn, String> {
        let (reply, rx) = oneshot::channel();
        self.cmd.send(Cmd::Connect { dst, reply }).await.map_err(|_| "Туннель WARP остановлен".to_string())?;
        self.wake.notify_one();
        tokio::time::timeout(CONNECT_TIMEOUT, rx)
            .await
            .map_err(|_| format!("{dst}: таймаут подключения через WARP"))?
            .map_err(|_| "Туннель WARP остановлен".to_string())?
    }

    pub fn wake(&self) -> Arc<Notify> {
        self.wake.clone()
    }
}

// ---------------------------------------------------------------- виртуальное устройство smoltcp

#[derive(Default)]
struct VDev {
    rx: VecDeque<Vec<u8>>,
    tx: VecDeque<Vec<u8>>,
}

struct VRx(Vec<u8>);
struct VTx<'a>(&'a mut VecDeque<Vec<u8>>);

impl RxToken for VRx {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        f(&self.0)
    }
}

impl TxToken for VTx<'_> {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut buf = vec![0u8; len];
        let r = f(&mut buf);
        self.0.push_back(buf);
        r
    }
}

impl Device for VDev {
    type RxToken<'a> = VRx;
    type TxToken<'a> = VTx<'a>;

    fn receive(&mut self, _t: SInstant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let p = self.rx.pop_front()?;
        Some((VRx(p), VTx(&mut self.tx)))
    }

    fn transmit(&mut self, _t: SInstant) -> Option<Self::TxToken<'_>> {
        Some(VTx(&mut self.tx))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut c = DeviceCapabilities::default();
        c.medium = Medium::Ip;
        c.max_transmission_unit = MTU;
        c
    }
}

// ---------------------------------------------------------------- WireGuard

struct Wg {
    tunn: Tunn,
    reserved: [u8; 3],
    junk: bool,
}

impl Wg {
    /// Готовит исходящий пакет: WARP ждёт client_id в «зарезервированных» байтах заголовка.
    fn mark(&self, pkt: &mut [u8]) {
        if pkt.len() >= 4 {
            pkt[1..4].copy_from_slice(&self.reserved);
        }
    }
}

/// «Мусорные» пакеты перед рукопожатием (как в AmneziaWG): система DPI не видит
/// характерного первого пакета WireGuard. Сервер WARP их просто отбрасывает.
async fn send_junk(udp: &UdpSocket) {
    let mut n = [0u8; 1];
    let _ = getrandom::getrandom(&mut n);
    for _ in 0..(3 + n[0] % 3) {
        let mut len = [0u8; 1];
        let _ = getrandom::getrandom(&mut len);
        let mut buf = vec![0u8; 40 + (len[0] as usize % 80)];
        let _ = getrandom::getrandom(&mut buf);
        // Первый байт вне диапазона типов WireGuard (1–4), чтобы сервер точно отбросил пакет.
        buf[0] = 0x40 | (buf[0] & 0x3f);
        let _ = udp.send(&buf).await;
    }
}

async fn send_wg(udp: &UdpSocket, wg: &Wg, pkt: &mut [u8], stats: &Stats) {
    if wg.junk && pkt.first() == Some(&1) {
        send_junk(udp).await;
    }
    wg.mark(pkt);
    stats.tx.fetch_add(pkt.len() as u64, Ordering::Relaxed);
    let _ = udp.send(pkt).await;
}

/// Подключается к точке WARP и проверяет рукопожатие. Возвращает UDP-сокет и состояние WireGuard.
async fn handshake(acc: &Account, endpoint: SocketAddr, junk: bool, limit: Duration) -> Result<(UdpSocket, Wg), String> {
    let udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await.map_err(|e| e.to_string())?;
    udp.connect(endpoint).await.map_err(|e| e.to_string())?;
    let mut idx = [0u8; 4];
    let _ = getrandom::getrandom(&mut idx);
    let tunn = Tunn::new(acc.secret()?, acc.peer()?, None, Some(25), u32::from_le_bytes(idx) >> 8, None);
    let wg = Wg { tunn, reserved: acc.reserved(), junk };
    let mut wg = wg;
    let stats = Stats::default();
    let mut out = vec![0u8; WG_BUF];
    let deadline = Instant::now() + limit;
    if let TunnResult::WriteToNetwork(p) = wg.tunn.format_handshake_initiation(&mut out, false) {
        let p = p.to_vec();
        let mut p = p;
        send_wg(&udp, &wg, &mut p, &stats).await;
    }
    let mut buf = vec![0u8; WG_BUF];
    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        let n = match tokio::time::timeout(left, udp.recv(&mut buf)).await {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(e.to_string()),
            Err(_) => break,
        };
        if n >= 4 {
            buf[1..4].fill(0);
        }
        let mut res = wg.tunn.decapsulate(None, &buf[..n], &mut out);
        while let TunnResult::WriteToNetwork(p) = res {
            let mut p = p.to_vec();
            send_wg(&udp, &wg, &mut p, &stats).await;
            res = wg.tunn.decapsulate(None, &[], &mut out);
        }
        if wg.tunn.time_since_last_handshake().is_some() {
            return Ok((udp, wg));
        }
    }
    Err(format!("{endpoint}: нет ответа"))
}

/// Подбирает рабочую точку подключения и запускает туннель.
pub async fn start(acc: &Account, junk: bool, log: impl Fn(String)) -> Result<Arc<Tunnel>, String> {
    let mut candidates: Vec<String> = vec![acc.endpoint.clone()];
    candidates.extend(ENDPOINTS.iter().map(|s| s.to_string()));
    candidates.dedup();
    let mut last = String::new();
    for ep in candidates {
        let Some(addr) = tokio::net::lookup_host(&ep).await.ok().and_then(|mut a| a.find(|x| x.is_ipv4())) else { continue };
        match handshake(acc, addr, junk, Duration::from_secs(4)).await {
            Ok((udp, wg)) => {
                log(format!("WARP подключён через {addr}"));
                return Ok(run(acc, udp, wg, addr));
            }
            Err(e) => last = e,
        }
    }
    Err(format!("Не удалось подключиться к WARP ни через одну точку ({last}). Возможно, WARP блокируется у вашего провайдера"))
}

// ---------------------------------------------------------------- основной цикл

struct Pending {
    handle: SocketHandle,
    reply: oneshot::Sender<Result<Conn, String>>,
    since: Instant,
    to_client: mpsc::Sender<Vec<u8>>,
    from_client: mpsc::Receiver<Vec<u8>>,
    client_rx: mpsc::Receiver<Vec<u8>>,
    client_tx: mpsc::Sender<Vec<u8>>,
}

struct Active {
    to_client: mpsc::Sender<Vec<u8>>,
    from_client: mpsc::Receiver<Vec<u8>>,
    /// Данные клиента, которые ещё не поместились в буфер сокета.
    pending_out: Vec<u8>,
    client_closed: bool,
}

fn run(acc: &Account, udp: UdpSocket, wg: Wg, endpoint: SocketAddr) -> Arc<Tunnel> {
    let (cmd_tx, cmd_rx) = mpsc::channel(64);
    let wake = Arc::new(Notify::new());
    let stats = Arc::new(Stats::default());
    let local: Ipv4Addr = acc.address_v4.split('/').next().unwrap_or("172.16.0.2").parse().unwrap_or(Ipv4Addr::new(172, 16, 0, 2));
    stats.last_rx.store(crate::util::now_unix(), Ordering::Relaxed);
    let t = Arc::new(Tunnel { cmd: cmd_tx, wake: wake.clone(), endpoint, stats: stats.clone() });
    let st = stats.clone();
    tokio::spawn(async move {
        stack_loop(udp, wg, local, cmd_rx, wake, stats).await;
        st.dead.store(true, Ordering::Relaxed);
    });
    t
}

impl Tunnel {
    /// Туннель жив: цикл работает и сервер отвечал за последние 5 минут.
    pub fn healthy(&self) -> bool {
        !self.stats.dead.load(Ordering::Relaxed)
            && crate::util::now_unix().saturating_sub(self.stats.last_rx.load(Ordering::Relaxed)) < 300
    }
}

fn now() -> SInstant {
    SInstant::now()
}

async fn stack_loop(udp: UdpSocket, mut wg: Wg, local: Ipv4Addr, mut cmd_rx: mpsc::Receiver<Cmd>, wake: Arc<Notify>, stats: Arc<Stats>) {
    let mut dev = VDev::default();
    let mut iface = Interface::new(Config::new(HardwareAddress::Ip), &mut dev, now());
    iface.update_ip_addrs(|a| {
        let o = local.octets();
        let _ = a.push(IpCidr::new(IpAddress::v4(o[0], o[1], o[2], o[3]), 32));
    });
    let _ = iface.routes_mut().add_default_ipv4_route(Ipv4Address::new(172, 16, 0, 1));
    let mut sockets = SocketSet::new(vec![]);
    let mut pending: Vec<Pending> = Vec::new();
    let mut active: HashMap<SocketHandle, Active> = HashMap::new();
    let mut next_port: u16 = 40_000;

    let mut udp_buf = vec![0u8; WG_BUF];
    let mut out = vec![0u8; WG_BUF];
    let mut timers = tokio::time::interval(Duration::from_millis(250));

    loop {
        let delay = iface.poll_delay(now(), &sockets).map(|d| Duration::from_micros(d.total_micros())).unwrap_or(Duration::from_millis(50));
        tokio::select! {
            r = udp.recv(&mut udp_buf) => {
                let Ok(n) = r else { break };
                stats.rx.fetch_add(n as u64, Ordering::Relaxed);
                stats.last_rx.store(crate::util::now_unix(), Ordering::Relaxed);
                if n >= 4 { udp_buf[1..4].fill(0); }
                let mut res = wg.tunn.decapsulate(None, &udp_buf[..n], &mut out);
                loop {
                    match res {
                        TunnResult::WriteToNetwork(p) => {
                            let mut p = p.to_vec();
                            send_wg(&udp, &wg, &mut p, &stats).await;
                            res = wg.tunn.decapsulate(None, &[], &mut out);
                        }
                        TunnResult::WriteToTunnelV4(p, _) => { dev.rx.push_back(p.to_vec()); break; }
                        _ => break,
                    }
                }
            }
            c = cmd_rx.recv() => {
                let Some(Cmd::Connect { dst, reply }) = c else { break };
                let mut sock = tcp::Socket::new(tcp::SocketBuffer::new(vec![0; TCP_BUF]), tcp::SocketBuffer::new(vec![0; TCP_BUF]));
                sock.set_nagle_enabled(false);
                sock.set_keep_alive(Some(smoltcp::time::Duration::from_secs(30)));
                next_port = if next_port >= 64_000 { 40_000 } else { next_port + 1 };
                let o = dst.ip().octets();
                match sock.connect(iface.context(), (IpAddress::v4(o[0], o[1], o[2], o[3]), dst.port()), next_port) {
                    Ok(()) => {
                        let handle = sockets.add(sock);
                        let (to_client, client_rx) = mpsc::channel(32);
                        let (client_tx, from_client) = mpsc::channel(32);
                        pending.push(Pending { handle, reply, since: Instant::now(), to_client, from_client, client_rx, client_tx });
                    }
                    Err(e) => { let _ = reply.send(Err(format!("{dst}: {e}"))); }
                }
            }
            _ = wake.notified() => {}
            _ = timers.tick() => {
                if let TunnResult::WriteToNetwork(p) = wg.tunn.update_timers(&mut out) {
                    let mut p = p.to_vec();
                    send_wg(&udp, &wg, &mut p, &stats).await;
                }
            }
            _ = tokio::time::sleep(delay) => {}
        }

        // Данные клиентов → сокеты; закрытие со стороны клиента.
        for (h, a) in active.iter_mut() {
            let sock = sockets.get_mut::<tcp::Socket>(*h);
            loop {
                if !a.pending_out.is_empty() {
                    if !sock.can_send() { break; }
                    let n = sock.send_slice(&a.pending_out).unwrap_or(0);
                    a.pending_out.drain(..n);
                    if !a.pending_out.is_empty() { break; }
                }
                if a.client_closed { break; }
                match a.from_client.try_recv() {
                    Ok(d) => a.pending_out = d,
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => { a.client_closed = true; break; }
                }
            }
            if a.client_closed && a.pending_out.is_empty() && sock.may_send() {
                sock.close();
            }
        }

        let _ = iface.poll(now(), &mut dev, &mut sockets);

        // Исходящие IP-пакеты → WireGuard → UDP.
        while let Some(ip) = dev.tx.pop_front() {
            if let TunnResult::WriteToNetwork(p) = wg.tunn.encapsulate(&ip, &mut out) {
                let mut p = p.to_vec();
                send_wg(&udp, &wg, &mut p, &stats).await;
            }
        }

        // Ожидающие подключения: установлено, отказ или таймаут.
        let mut i = 0;
        while i < pending.len() {
            let state = sockets.get::<tcp::Socket>(pending[i].handle).state();
            let timed_out = pending[i].since.elapsed() > CONNECT_TIMEOUT;
            if state == tcp::State::Established {
                let p = pending.swap_remove(i);
                let conn = Conn { tx: p.client_tx, rx: p.client_rx };
                if p.reply.send(Ok(conn)).is_ok() {
                    active.insert(p.handle, Active { to_client: p.to_client, from_client: p.from_client, pending_out: Vec::new(), client_closed: false });
                } else {
                    sockets.get_mut::<tcp::Socket>(p.handle).abort();
                }
            } else if state == tcp::State::Closed || timed_out {
                let p = pending.swap_remove(i);
                let _ = p.reply.send(Err("сервер не принял соединение".into()));
                sockets.remove(p.handle);
            } else {
                i += 1;
            }
        }

        // Данные из сокетов → клиентам; завершённые соединения.
        let mut dead = Vec::new();
        for (h, a) in active.iter_mut() {
            let sock = sockets.get_mut::<tcp::Socket>(*h);
            while sock.can_recv() {
                let Ok(permit) = a.to_client.try_reserve() else { break };
                let data = sock.recv(|b| (b.len(), b.to_vec())).unwrap_or_default();
                if data.is_empty() { break; }
                permit.send(data);
            }
            let finished = !sock.is_open() || (!sock.may_recv() && !sock.can_recv() && a.client_closed) || a.to_client.is_closed();
            if finished {
                dead.push(*h);
            }
        }
        for h in dead {
            if let Some(_a) = active.remove(&h) {
                sockets.get_mut::<tcp::Socket>(h).abort();
                let _ = iface.poll(now(), &mut dev, &mut sockets);
                sockets.remove(h);
            }
        }
        // Пакеты, появившиеся после закрытия сокетов.
        while let Some(ip) = dev.tx.pop_front() {
            if let TunnResult::WriteToNetwork(p) = wg.tunn.encapsulate(&ip, &mut out) {
                let mut p = p.to_vec();
                send_wg(&udp, &wg, &mut p, &stats).await;
            }
        }
    }
}

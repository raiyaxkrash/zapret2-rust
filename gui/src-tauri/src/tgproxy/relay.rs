//! Маршрутизация соединений Telegram Desktop: WebSocket к серверам Telegram
//! (через пул готовых соединений), запасные пути через Cloudflare-прокси и прямой TCP.
//! Логика повторяет tg-ws-proxy (Flowseal, MIT).

use std::{
    collections::{HashMap, VecDeque},
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use ctr::cipher::StreamCipher;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Notify,
    task::JoinSet,
    time::timeout,
};

use super::{
    obfs::{self, CryptoCtx, Proto, HANDSHAKE_LEN},
    ws::{self, Target, WsReader, WsWriter},
};

/// IP, через который доступны WebSocket-точки Telegram (kwsN.web.telegram.org).
const WS_IP: &str = "149.154.167.220";
/// DC, которые идут через WebSocket. Остальные — через запасные пути.
const WS_DCS: [i32; 2] = [2, 4];
/// SNI для подключения с подменой имени, если прямое подключение блокируется.
const FRONTING_SNI: &str = "sprinthost.ru";

const POOL_SIZE: usize = 4;
const POOL_MAX_AGE: Duration = Duration::from_secs(120);
const POOL_TICK: Duration = Duration::from_secs(5);
const CF_MAX_TRIES: usize = 5;
const TCP_BACKOFF_INITIAL: u64 = 30;
const TCP_BACKOFF_MAX: u64 = 3600;
const BUF: usize = 64 * 1024;

fn dc_ip(dc: i32, test: bool) -> Option<&'static str> {
    if test {
        return match dc {
            1 => Some("149.154.175.10"),
            2 => Some("149.154.167.40"),
            3 => Some("149.154.175.117"),
            _ => None,
        };
    }
    match dc {
        1 => Some("149.154.175.50"),
        2 => Some("149.154.167.51"),
        3 => Some("149.154.175.100"),
        4 => Some("149.154.167.91"),
        5 => Some("149.154.171.5"),
        203 => Some("91.105.192.100"),
        _ => None,
    }
}

fn ws_domains(dc: i32, media: bool) -> Vec<String> {
    let dc = if dc == 203 { 2 } else { dc };
    if media {
        vec![format!("kws{dc}-1.web.telegram.org"), format!("kws{dc}.web.telegram.org")]
    } else {
        vec![format!("kws{dc}.web.telegram.org")]
    }
}

// ---------------------------------------------------------------- Cloudflare-домены

/// Список доменов в репозитории tg-ws-proxy хранится в простом шифре сдвига.
pub fn decode_cf_domain(s: &str) -> String {
    let Some(p) = s.strip_suffix(".com") else { return s.to_string() };
    let n = p.chars().filter(|c| c.is_ascii_alphabetic()).count() as i32;
    let body: String = p
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_lowercase() { b'a' } else { b'A' } as i32;
                (((c as i32 - base - n).rem_euclid(26)) + base) as u8 as char
            } else {
                c
            }
        })
        .collect();
    body + ".co.uk"
}

const CF_DEFAULT: [&str; 20] = [
    "virkgj.com", "vmmzovy.com", "mkuosckvso.com", "zaewayzmplad.com", "twdmbzcm.com", "awzwsldi.com",
    "clngqrflngqin.com", "tjacxbqtj.com", "bxaxtxmrw.com", "dmohrsgmohcrwb.com", "vwbmtmoi.com", "khgrre.com",
    "ulihssf.com", "tmhqsdqmfpmk.com", "xwuwoqbm.com", "orgcnunpj.com", "zhkuldz.com", "zypoljnslxa.com",
    "efabnxaowuzs.com", "zaftuzsftqdq.com",
];
pub const CF_LIST_URL: &str = "https://raw.githubusercontent.com/Flowseal/tg-ws-proxy/main/.github/cfproxy-domains.txt";

fn valid_domain(d: &str) -> bool {
    let labels: Vec<&str> = d.split('.').collect();
    d.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && labels.last().map(|t| t.len() >= 2 && t.chars().any(|c| c.is_ascii_alphabetic())).unwrap_or(false)
}

/// Разбирает загруженный список; None — если в нём меньше трёх корректных доменов.
pub fn parse_cf_list(text: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let d = decode_cf_domain(line).to_lowercase();
        if valid_domain(&d) && !out.contains(&d) {
            out.push(d);
        }
    }
    (out.len() >= 3).then_some(out)
}

fn shuffle<T>(v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let mut b = [0u8; 4];
        let _ = getrandom::getrandom(&mut b);
        v.swap(i, u32::from_le_bytes(b) as usize % (i + 1));
    }
}

// ---------------------------------------------------------------- разрезание на пакеты

/// Режет поток MTProto на отдельные пакеты транспорта, чтобы каждый ушёл
/// отдельным сообщением WebSocket (так их ждут серверы Telegram).
pub struct Splitter {
    proto: Proto,
    cipher: Vec<u8>,
    plain: Vec<u8>,
    disabled: bool,
}

impl Splitter {
    pub fn new(proto: Proto) -> Self {
        Splitter { proto, cipher: Vec::new(), plain: Vec::new(), disabled: false }
    }

    /// Длина следующего пакета: None — данных пока мало, Some(0) — поток не разобрать.
    fn next_len(&self, off: usize) -> Option<usize> {
        let avail = self.plain.len() - off;
        if avail == 0 {
            return None;
        }
        let (header, payload) = match self.proto {
            Proto::Abridged => {
                let first = self.plain[off];
                if first == 0x7f || first == 0xff {
                    if avail < 4 {
                        return None;
                    }
                    let p = &self.plain[off + 1..off + 4];
                    (4, (p[0] as usize | (p[1] as usize) << 8 | (p[2] as usize) << 16) * 4)
                } else {
                    (1, (first & 0x7f) as usize * 4)
                }
            }
            Proto::Intermediate | Proto::PaddedIntermediate => {
                if avail < 4 {
                    return None;
                }
                let p = &self.plain[off..off + 4];
                (4, (u32::from_le_bytes([p[0], p[1], p[2], p[3]]) & 0x7fff_ffff) as usize)
            }
        };
        if payload == 0 {
            return Some(0);
        }
        (avail >= header + payload).then_some(header + payload)
    }

    pub fn split(&mut self, cipher: &[u8], plain: &[u8]) -> Vec<Vec<u8>> {
        if cipher.is_empty() {
            return vec![];
        }
        if self.disabled {
            return vec![cipher.to_vec()];
        }
        self.cipher.extend_from_slice(cipher);
        self.plain.extend_from_slice(plain);
        let mut parts = Vec::new();
        let mut off = 0;
        while off < self.cipher.len() {
            match self.next_len(off) {
                None => break,
                Some(0) => {
                    parts.push(self.cipher[off..].to_vec());
                    off = self.cipher.len();
                    self.disabled = true;
                }
                Some(n) => {
                    parts.push(self.cipher[off..off + n].to_vec());
                    off += n;
                }
            }
        }
        self.cipher.drain(..off);
        self.plain.drain(..off);
        parts
    }

    pub fn flush(&mut self) -> Option<Vec<u8>> {
        self.plain.clear();
        (!self.cipher.is_empty()).then(|| std::mem::take(&mut self.cipher))
    }
}

// ---------------------------------------------------------------- маршрутизатор

type PoolKey = (i32, bool, bool); // dc, media, test

#[derive(Default)]
struct Bucket {
    idle: VecDeque<(WsReader, WsWriter, Instant)>,
    failures: u32,
    retry_after: Option<Instant>,
}

pub type Logger = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Default)]
pub struct Stats {
    pub connections: AtomicU64,
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
}

pub struct Router {
    secret: [u8; 16],
    pool: Mutex<HashMap<PoolKey, Bucket>>,
    pool_wake: Notify,
    fronting_first: AtomicBool,
    cf_domains: Mutex<Vec<String>>,
    cf_current: Mutex<HashMap<i32, String>>,
    tcp_backoff: Mutex<HashMap<&'static str, (u32, Instant)>>,
    /// Последний способ подключения для каждого DC: в журнал пишем только смену способа.
    routes: Mutex<HashMap<(i32, bool), &'static str>>,
    bad_secret_logged: AtomicBool,
    pub stats: Stats,
    log: Logger,
}

impl Router {
    pub fn new(secret: [u8; 16], log: Logger) -> Arc<Router> {
        let mut pool = HashMap::new();
        for dc in WS_DCS {
            for media in [false, true] {
                pool.insert((dc, media, false), Bucket::default());
            }
        }
        Arc::new(Router {
            secret,
            pool: Mutex::new(pool),
            pool_wake: Notify::new(),
            fronting_first: AtomicBool::new(false),
            cf_domains: Mutex::new(CF_DEFAULT.iter().map(|d| decode_cf_domain(d)).collect()),
            cf_current: Mutex::new(HashMap::new()),
            tcp_backoff: Mutex::new(HashMap::new()),
            routes: Mutex::new(HashMap::new()),
            bad_secret_logged: AtomicBool::new(false),
            stats: Stats::default(),
            log,
        })
    }

    pub fn set_cf_domains(&self, list: Vec<String>) {
        *self.cf_domains.lock().unwrap() = list;
    }

    fn note_route(&self, dc: i32, media: bool, route: &'static str) {
        let prev = self.routes.lock().unwrap().insert((dc, media), route);
        if prev != Some(route) {
            (self.log)(format!("DC{dc}{}: {route}", if media { " (медиа)" } else { "" }));
        }
    }

    // ------------------------------------------------------------ пул WebSocket

    fn pool_take(&self, key: PoolKey) -> Option<(WsReader, WsWriter)> {
        let mut pool = self.pool.lock().unwrap();
        let bucket = pool.entry(key).or_default();
        let mut found = None;
        while let Some((r, w, created)) = bucket.idle.pop_front() {
            if created.elapsed() < POOL_MAX_AGE {
                found = Some((r, w));
                break;
            }
        }
        drop(pool);
        self.pool_wake.notify_one();
        found
    }

    async fn connect_ws(&self, dc: i32, media: bool, test: bool) -> Option<(WsReader, WsWriter)> {
        let path = if test { "/apiws_test" } else { "/apiws" };
        for domain in ws_domains(dc, media) {
            let front_first = self.fronting_first.load(Ordering::Relaxed);
            for fronted in [front_first, !front_first] {
                let t = Target { addr: WS_IP, host: &domain, path, sni: fronted.then_some(FRONTING_SNI) };
                let limit = Duration::from_secs(if fronted { 7 } else { 8 });
                if let Ok(c) = ws::connect(t, limit).await {
                    self.fronting_first.store(fronted, Ordering::Relaxed);
                    return Some(c);
                }
            }
        }
        None
    }

    /// Поддерживает пул готовых WebSocket-соединений: выбрасывает старые и
    /// добирает недостающие (с нарастающей паузой после неудач).
    pub async fn maintain_pool(self: Arc<Self>) {
        loop {
            let now = Instant::now();
            let mut wanted: Vec<(PoolKey, usize)> = Vec::new();
            {
                let mut pool = self.pool.lock().unwrap();
                for (key, b) in pool.iter_mut() {
                    b.idle.retain(|(_, _, c)| c.elapsed() < POOL_MAX_AGE);
                    let need = POOL_SIZE.saturating_sub(b.idle.len());
                    if need > 0 && b.retry_after.is_none_or(|t| now >= t) {
                        wanted.push((*key, need));
                    }
                }
            }
            if !wanted.is_empty() {
                let mut set = JoinSet::new();
                for (key, need) in &wanted {
                    for _ in 0..*need {
                        let me = Arc::clone(&self);
                        let key = *key;
                        set.spawn(async move { (key, me.connect_ws(key.0, key.1, key.2).await) });
                    }
                }
                let mut ok: HashMap<PoolKey, usize> = HashMap::new();
                while let Some(res) = set.join_next().await {
                    if let Ok((key, Some((r, w)))) = res {
                        *ok.entry(key).or_default() += 1;
                        self.pool.lock().unwrap().entry(key).or_default().idle.push_back((r, w, Instant::now()));
                    }
                }
                let mut pool = self.pool.lock().unwrap();
                for (key, _) in wanted {
                    let b = pool.entry(key).or_default();
                    if ok.get(&key).copied().unwrap_or(0) > 0 {
                        b.failures = 0;
                        b.retry_after = None;
                    } else {
                        b.failures += 1;
                        let delay = (1u64 << b.failures.min(12).saturating_sub(1)).min(3600);
                        b.retry_after = Some(Instant::now() + Duration::from_secs(delay));
                        if b.failures == 1 || b.failures.is_multiple_of(5) {
                            (self.log)(format!(
                                "Нет WebSocket-подключения к DC{}{}, повтор через {delay} с",
                                key.0,
                                if key.1 { " (медиа)" } else { "" }
                            ));
                        }
                    }
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(POOL_TICK) => {}
                _ = self.pool_wake.notified() => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        }
    }

    // ------------------------------------------------------------ обработка клиента

    pub async fn handle(self: Arc<Self>, mut client: TcpStream) {
        let _ = client.set_nodelay(true);
        self.stats.connections.fetch_add(1, Ordering::Relaxed);
        let mut hs = [0u8; HANDSHAKE_LEN];
        match timeout(Duration::from_secs(10), client.read_exact(&mut hs)).await {
            Ok(Ok(_)) => {}
            _ => return,
        }
        let Some(hello) = obfs::parse_client_hello(&hs, &self.secret) else {
            if !self.bad_secret_logged.swap(true, Ordering::Relaxed) {
                (self.log)("Подключение с неверным секретом. Подключите прокси в Telegram заново кнопкой «Подключить в Telegram»".into());
            }
            let mut sink = [0u8; 4096];
            let mut left = 64 * 1024;
            let _ = timeout(Duration::from_secs(10), async {
                while left > 0 {
                    match client.read(&mut sink).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => left -= n.min(left),
                    }
                }
            })
            .await;
            return;
        };

        let test = hello.dc >= 10000;
        let dc = if test { hello.dc - 10000 } else { hello.dc };
        let media = hello.is_media;
        let proto = hello.proto;
        let dc_idx = (if media { -dc } else { dc }) as i16;
        let init = obfs::relay_init(proto, dc_idx);
        let ctx = obfs::crypto_ctx(hello, &self.secret, &init);

        if WS_DCS.contains(&dc) {
            if let Some((r, w)) = self.pool_take((dc, media, test)) {
                self.note_route(dc, media, "WebSocket");
                self.bridge_ws(client, r, w, &init, ctx, Some(Splitter::new(proto))).await;
                return;
            }
        }
        self.fallback(client, dc, media, test, proto, &init, ctx).await;
    }

    #[allow(clippy::too_many_arguments)]
    async fn fallback(&self, client: TcpStream, dc: i32, media: bool, test: bool, proto: Proto, init: &[u8; 64], ctx: CryptoCtx) {
        if !test {
            let mut domains = self.cf_domains.lock().unwrap().clone();
            shuffle(&mut domains);
            if let Some(cur) = self.cf_current.lock().unwrap().get(&dc).cloned() {
                domains.retain(|d| d != &cur);
                domains.insert(0, cur);
            }
            for base in domains.into_iter().take(CF_MAX_TRIES) {
                let host = format!("kws{dc}.{base}");
                let t = Target { addr: &host, host: &host, path: "/apiws", sni: None };
                if let Ok((r, w)) = ws::connect(t, Duration::from_secs(10)).await {
                    self.cf_current.lock().unwrap().insert(dc, base);
                    self.note_route(dc, media, "через Cloudflare");
                    self.bridge_ws(client, r, w, init, ctx, Some(Splitter::new(proto))).await;
                    return;
                }
            }
        }
        if let Some(ip) = dc_ip(dc, test) {
            if let Some(remote) = self.tcp_connect(ip).await {
                self.note_route(dc, media, "напрямую по TCP");
                self.bridge_tcp(client, remote, init, ctx).await;
                return;
            }
        }
        self.note_route(dc, media, "нет доступного пути");
    }

    async fn tcp_connect(&self, ip: &'static str) -> Option<TcpStream> {
        if let Some((_, until)) = self.tcp_backoff.lock().unwrap().get(ip) {
            if Instant::now() < *until {
                return None;
            }
        }
        match timeout(Duration::from_secs(10), TcpStream::connect((ip, 443u16))).await {
            Ok(Ok(s)) => {
                let _ = s.set_nodelay(true);
                self.tcp_backoff.lock().unwrap().remove(ip);
                Some(s)
            }
            _ => {
                let mut b = self.tcp_backoff.lock().unwrap();
                let fails = b.get(ip).map(|x| x.0).unwrap_or(0) + 1;
                let delay = (TCP_BACKOFF_INITIAL << (fails - 1).min(7)).min(TCP_BACKOFF_MAX);
                b.insert(ip, (fails, Instant::now() + Duration::from_secs(delay)));
                None
            }
        }
    }

    // ------------------------------------------------------------ пересылка

    async fn bridge_ws(&self, client: TcpStream, mut r: WsReader, w: WsWriter, init: &[u8; 64], ctx: CryptoCtx, mut splitter: Option<Splitter>) {
        if w.send(init).await.is_err() {
            return;
        }
        let CryptoCtx { mut clt_dec, mut clt_enc, mut tg_enc, mut tg_dec } = ctx;
        let (mut crd, mut cwr) = client.into_split();
        let stats = &self.stats;

        let up = async {
            let mut buf = vec![0u8; BUF];
            loop {
                let n = match crd.read(&mut buf).await {
                    Ok(0) | Err(_) => {
                        if let Some(tail) = splitter.as_mut().and_then(Splitter::flush) {
                            let _ = w.send(&tail).await;
                        }
                        return;
                    }
                    Ok(n) => n,
                };
                stats.bytes_up.fetch_add(n as u64, Ordering::Relaxed);
                let data = &mut buf[..n];
                clt_dec.apply_keystream(data);
                let plain = data.to_vec();
                tg_enc.apply_keystream(data);
                let res = match splitter.as_mut() {
                    Some(s) => {
                        let parts = s.split(data, &plain);
                        if parts.is_empty() {
                            continue;
                        }
                        let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
                        w.send_all(&refs).await
                    }
                    None => w.send(data).await,
                };
                if res.is_err() {
                    return;
                }
            }
        };
        let down = async {
            loop {
                let mut data = match r.recv().await {
                    Ok(Some(d)) => d,
                    _ => return,
                };
                stats.bytes_down.fetch_add(data.len() as u64, Ordering::Relaxed);
                tg_dec.apply_keystream(&mut data);
                clt_enc.apply_keystream(&mut data);
                if cwr.write_all(&data).await.is_err() {
                    return;
                }
            }
        };
        tokio::select! {
            _ = up => {}
            _ = down => {}
        }
        w.close().await;
        let _ = cwr.shutdown().await;
    }

    async fn bridge_tcp(&self, client: TcpStream, remote: TcpStream, init: &[u8; 64], ctx: CryptoCtx) {
        let CryptoCtx { mut clt_dec, mut clt_enc, mut tg_enc, mut tg_dec } = ctx;
        let (mut crd, mut cwr) = client.into_split();
        let (mut rrd, mut rwr) = remote.into_split();
        if rwr.write_all(init).await.is_err() {
            return;
        }
        let stats = &self.stats;
        let up = async {
            let mut buf = vec![0u8; BUF];
            loop {
                let n = match crd.read(&mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                stats.bytes_up.fetch_add(n as u64, Ordering::Relaxed);
                clt_dec.apply_keystream(&mut buf[..n]);
                tg_enc.apply_keystream(&mut buf[..n]);
                if rwr.write_all(&buf[..n]).await.is_err() {
                    return;
                }
            }
        };
        let down = async {
            let mut buf = vec![0u8; BUF];
            loop {
                let n = match rrd.read(&mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                stats.bytes_down.fetch_add(n as u64, Ordering::Relaxed);
                tg_dec.apply_keystream(&mut buf[..n]);
                clt_enc.apply_keystream(&mut buf[..n]);
                if cwr.write_all(&buf[..n]).await.is_err() {
                    return;
                }
            }
        };
        tokio::select! {
            _ = up => {}
            _ = down => {}
        }
        let _ = cwr.shutdown().await;
    }
}

/// Загружает актуальный список Cloudflare-доменов из репозитория tg-ws-proxy.
pub async fn fetch_cf_domains() -> io::Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .user_agent("zapret2-gui")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(io::Error::other)?;
    let text = client
        .get(CF_LIST_URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(io::Error::other)?
        .text()
        .await
        .map_err(io::Error::other)?;
    parse_cf_list(&text).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "список доменов пуст"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Живая проверка через настоящие серверы Telegram: клиент как у Telegram Desktop
    /// отправляет через прокси req_pq_multi и ждёт resPQ.
    /// Запуск: cargo test --lib live_telegram -- --ignored --nocapture
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_telegram() {
        let secret = [0x42u8; 16];
        let log: Logger = Arc::new(|l| println!("[tg] {l}"));
        let router = Router::new(secret, log);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(Arc::clone(&router).maintain_pool());
        {
            let router = Arc::clone(&router);
            tokio::spawn(async move {
                if let Ok(list) = fetch_cf_domains().await {
                    println!("CF domains: {}", list.len());
                    router.set_cf_domains(list);
                }
            });
        }
        {
            let router = Arc::clone(&router);
            tokio::spawn(async move {
                loop {
                    let (s, _) = listener.accept().await.unwrap();
                    tokio::spawn(Arc::clone(&router).handle(s));
                }
            });
        }
        // Пусть пул WebSocket успеет заполниться.
        tokio::time::sleep(Duration::from_secs(6)).await;

        let mut failed = Vec::new();
        for dc_idx in [2i16, -2, 4, -4, 1, 3, 5] {
            let started = Instant::now();
            let r = timeout(Duration::from_secs(40), req_pq(addr, &secret, dc_idx)).await;
            match r {
                Ok(Ok(())) => println!("DC{dc_idx}: resPQ получен за {} мс", started.elapsed().as_millis()),
                other => {
                    println!("DC{dc_idx}: ОШИБКА {other:?}");
                    failed.push(dc_idx);
                }
            }
        }
        assert!(failed.is_empty(), "не ответили DC: {failed:?}");
    }

    async fn req_pq(addr: std::net::SocketAddr, secret: &[u8; 16], dc_idx: i16) -> io::Result<()> {
        let mut rnd = [0u8; 64];
        loop {
            getrandom::getrandom(&mut rnd).unwrap();
            if acceptable(&rnd) {
                break;
            }
        }
        let (hs, mut enc, mut dec) = obfs::tests::client_hello(secret, Proto::PaddedIntermediate, dc_idx, rnd);
        let mut s = TcpStream::connect(addr).await?;
        s.write_all(&hs).await?;

        let mut nonce = [0u8; 16];
        getrandom::getrandom(&mut nonce).unwrap();
        let msg_id = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() << 32) & !3;
        let mut body = Vec::new();
        body.extend_from_slice(&[0u8; 8]); // auth_key_id = 0
        body.extend_from_slice(&msg_id.to_le_bytes());
        body.extend_from_slice(&20u32.to_le_bytes());
        body.extend_from_slice(&0xbe7e8ef1u32.to_le_bytes()); // req_pq_multi
        body.extend_from_slice(&nonce);
        body.extend_from_slice(&[0u8; 3]); // паддинг padded intermediate
        let mut pkt = (body.len() as u32).to_le_bytes().to_vec();
        pkt.extend_from_slice(&body);
        enc.apply_keystream(&mut pkt);
        s.write_all(&pkt).await?;

        let mut len = [0u8; 4];
        s.read_exact(&mut len).await?;
        dec.apply_keystream(&mut len);
        let n = u32::from_le_bytes(len) as usize;
        if !(28..=4096).contains(&n) {
            return Err(io::Error::other(format!("странная длина ответа {n}")));
        }
        let mut resp = vec![0u8; n];
        s.read_exact(&mut resp).await?;
        dec.apply_keystream(&mut resp);
        let ctor = u32::from_le_bytes(resp[20..24].try_into().unwrap());
        if ctor != 0x05162463 || resp[24..40] != nonce {
            return Err(io::Error::other(format!("не resPQ: {ctor:08x}")));
        }
        Ok(())
    }

    fn acceptable(r: &[u8; 64]) -> bool {
        r[0] != 0xef && &r[..4] != b"GET " && &r[..4] != b"POST" && &r[..4] != b"HEAD" && r[..4] != [0xee; 4] && r[..4] != [0xdd; 4] && r[4..8] != [0; 4]
    }

    /// Эталон — функция _dd из tg-ws-proxy.
    #[test]
    fn cf_domain_decoding() {
        assert_eq!(decode_cf_domain("virkgj.com"), "pclead.co.uk");
        assert_eq!(decode_cf_domain("zaewayzmplad.com"), "noskomnadzor.co.uk");
        assert_eq!(decode_cf_domain("clngqrflngqin.com"), "pyatdesyatdva.co.uk");
        assert_eq!(decode_cf_domain("example.net"), "example.net");
        assert!(CF_DEFAULT.iter().all(|d| valid_domain(&decode_cf_domain(d))));
        assert_eq!(parse_cf_list("# c\nvirkgj.com\nvirkgj.com\nkhgrre.com\n"), None);
        assert_eq!(parse_cf_list("virkgj.com\nkhgrre.com\nulihssf.com\n").map(|v| v.len()), Some(3));
    }

    #[test]
    fn splitter_intermediate() {
        let mut s = Splitter::new(Proto::Intermediate);
        // Два пакета по 4+8 байт, пришедшие кусками.
        let mut stream = Vec::new();
        for _ in 0..2 {
            stream.extend_from_slice(&8u32.to_le_bytes());
            stream.extend_from_slice(&[7u8; 8]);
        }
        let a = s.split(&stream[..5], &stream[..5]);
        assert!(a.is_empty());
        let b = s.split(&stream[5..20], &stream[5..20]);
        assert_eq!(b, vec![stream[..12].to_vec()]);
        let c = s.split(&stream[20..], &stream[20..]);
        assert_eq!(c, vec![stream[12..].to_vec()]);
        assert_eq!(s.flush(), None);
    }

    #[test]
    fn splitter_abridged() {
        let mut s = Splitter::new(Proto::Abridged);
        let mut stream = vec![0x02u8];
        stream.extend_from_slice(&[1u8; 8]);
        stream.extend_from_slice(&[0x7f, 0x01, 0x00, 0x00]);
        stream.extend_from_slice(&[2u8; 4]);
        let parts = s.split(&stream, &stream);
        assert_eq!(parts, vec![stream[..9].to_vec(), stream[9..].to_vec()]);
        // Нулевая длина — дальше без разбора.
        let parts = s.split(&[0x00, 9, 9], &[0x00, 9, 9]);
        assert_eq!(parts, vec![vec![0x00, 9, 9]]);
        assert_eq!(s.split(&[5], &[5]), vec![vec![5]]);
    }
}

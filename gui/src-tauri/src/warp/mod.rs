//! Встроенный Cloudflare WARP: бесплатный туннель для сайтов, которые провайдер блокирует
//! так, что обход DPI их не открывает. Трафик выбранных сайтов идёт через WARP по системному
//! PAC-файлу (и перехватом для программ), остальной — напрямую. Сервисы, закрытые по стране,
//! WARP не откроет: Cloudflare привязывает адрес WARP к стране пользователя.

pub mod api;
pub mod apps;
pub mod proxy;
pub mod sysproxy;
pub mod tunnel;

use std::{
    fs,
    net::{Ipv4Addr, SocketAddrV4},
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc, Mutex, RwLock},
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;

use crate::{util::now_unix, AppState};

pub const DEFAULT_PORT: u16 = 40080;

/// Текущий туннель: при переподключении заменяется, прокси берёт новый для следующих соединений.
pub type Shared = Arc<RwLock<Option<Arc<tunnel::Tunnel>>>>;

#[derive(Default)]
struct Inner {
    task: Option<tokio::task::JoinHandle<()>>,
    since: Option<u64>,
    last_error: Option<String>,
    /// Папка данных: там лежит сохранённая настройка прокси пользователя.
    root: Option<PathBuf>,
    port: u16,
    /// WARP для программ без поддержки системного прокси.
    apps: Option<apps::Redirector>,
}

#[derive(Default)]
pub struct Warp {
    inner: Mutex<Inner>,
    tunnel: Shared,
    domains: proxy::Domains,
    exes: apps::Exes,
    /// Способ подключения, которым сейчас работает туннель.
    route: Arc<Mutex<Option<tunnel::Route>>>,
}

#[derive(Serialize, Clone, Debug)]
pub struct WarpStatus {
    pub running: bool,
    /// Туннель поднят и отвечает.
    pub connected: bool,
    pub since: Option<u64>,
    pub last_error: Option<String>,
    pub endpoint: String,
    pub port: u16,
    pub rx: u64,
    pub tx: u64,
    pub domains: usize,
    pub exes: usize,
    /// Работает перехват для программ.
    pub apps: bool,
    /// Маскировка, которую подобрала программа: "dns", "quic", "junk" или "plain".
    pub mask: Option<tunnel::Mask>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Trace {
    pub ip: String,
    pub loc: String,
    pub colo: String,
    pub warp: bool,
}

fn account_file(root: &Path) -> PathBuf {
    root.join("warp.json")
}

/// Аккаунт WARP: сохранённый или новый (регистрация один раз).
async fn account(root: &Path) -> Result<api::Account, String> {
    let file = account_file(root);
    if let Some(acc) = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<api::Account>(&t).ok()) {
        return Ok(acc);
    }
    let acc = api::register().await.map_err(|e| {
        format!("Не удалось зарегистрировать WARP: адрес регистрации у вашего провайдера заблокирован ({e}). Включите обход (сферу на главной) — регистрация пройдёт через него, и WARP подключится сам")
    })?;
    let _ = fs::write(&file, serde_json::to_string_pretty(&acc).unwrap_or_default());
    Ok(acc)
}

fn route_file(root: &Path) -> PathBuf {
    root.join("warp-route.json")
}

/// Способ подключения, который сработал в прошлый раз.
fn load_route(root: &Path) -> Option<tunnel::Route> {
    fs::read_to_string(route_file(root)).ok().and_then(|t| serde_json::from_str(&t).ok())
}

fn save_route(root: &Path, r: &tunnel::Route) {
    let _ = fs::write(route_file(root), serde_json::to_string_pretty(r).unwrap_or_default());
}

/// Настройки подключения: подбор маскировки, QUIC из zapret2 и прошлый рабочий способ.
fn options(root: &Path, auto: bool, preferred: Option<tunnel::Route>) -> tunnel::Options {
    let quic = fs::read(root.join("engine").join("fake").join("quic_initial_www_google_com.bin")).ok().map(Arc::new);
    tunnel::Options { auto, quic, preferred }
}

/// Забыть подобранный способ: при следующем подключении подбор начнётся заново.
pub fn reset_route(root: &Path) {
    let _ = fs::remove_file(route_file(root));
}

/// Перевыпустить бесплатный аккаунт WARP (удаляет сохранённые ключи и маршрут).
pub fn reset_account(root: &Path) {
    let _ = fs::remove_file(account_file(root));
    reset_route(root);
}

/// Привязать ключ WARP+ к текущему или новому аккаунту.
pub async fn apply_license(root: &Path, license: &str) -> Result<(), String> {
    let acc = account(root).await?;
    api::bind_license(&acc, license).await
}

fn log(app: &AppHandle, line: impl AsRef<str>) {
    app.state::<AppState>().engine.log(app, format!("[warp] {}", line.as_ref()));
}

impl Warp {
    pub fn status(&self) -> WarpStatus {
        let g = self.inner.lock().unwrap();
        let running = g.task.as_ref().map(|t| !t.is_finished()).unwrap_or(false);
        let t = self.tunnel.read().unwrap().clone();
        WarpStatus {
            running,
            connected: running && t.as_ref().map(|t| t.healthy()).unwrap_or(false),
            since: if running { g.since } else { None },
            last_error: g.last_error.clone(),
            endpoint: t.as_ref().map(|t| t.endpoint.to_string()).unwrap_or_default(),
            port: g.port,
            rx: t.as_ref().map(|t| t.stats.rx.load(Ordering::Relaxed)).unwrap_or(0),
            tx: t.as_ref().map(|t| t.stats.tx.load(Ordering::Relaxed)).unwrap_or(0),
            domains: self.domains.read().unwrap().len(),
            exes: self.exes.read().unwrap().len(),
            apps: running && g.apps.is_some(),
            mask: if running { self.route.lock().unwrap().as_ref().map(|r| r.mask) } else { None },
        }
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().unwrap().task.as_ref().map(|t| !t.is_finished()).unwrap_or(false)
    }

    fn pac_url(port: u16) -> String {
        // Метка времени в адресе заставляет браузеры перечитать PAC после изменения списка.
        format!("http://127.0.0.1:{port}/proxy.pac?v={}", now_unix())
    }

    /// Обновляет список доменов и программ на лету (без переподключения туннеля).
    pub fn set_targets(&self, domains: Vec<String>, exes: Vec<String>) {
        *self.domains.write().unwrap() = domains;
        *self.exes.write().unwrap() = exes;
        let g = self.inner.lock().unwrap();
        if g.task.as_ref().map(|t| !t.is_finished()).unwrap_or(false) {
            let _ = sysproxy::set(&Self::pac_url(g.port));
            if let Some(a) = &g.apps {
                a.reseed();
            }
        }
    }

    /// `apps` — папка с WinDivert (zapret2), если нужен WARP для программ.
    /// `auto` — подбирать маскировку (настройка «Маскировка»).
    #[allow(clippy::too_many_arguments)]
    pub async fn start(&self, app: &AppHandle, root: &Path, port: u16, auto: bool, domains: Vec<String>, exes: Vec<String>, apps: Option<PathBuf>) -> Result<(), String> {
        self.stop(app);
        let r = self.spawn(app, root, port, auto, domains, exes, apps).await;
        if let Err(e) = &r {
            log(app, e);
            self.inner.lock().unwrap().last_error = Some(e.clone());
            let _ = app.emit("warp-state", ());
        }
        r
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn(&self, app: &AppHandle, root: &Path, port: u16, auto: bool, domains: Vec<String>, exes: Vec<String>, apps: Option<PathBuf>) -> Result<(), String> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|e| format!("Порт {port} для WARP занят другой программой ({e})"))?;
        log(app, "Подключение к WARP…");
        let acc = account(root).await?;
        let h = app.clone();
        let (t, route) = tunnel::start(&acc, &options(root, auto, load_route(root)), move |l| log(&h, l)).await?;
        save_route(root, &route);
        *self.route.lock().unwrap() = Some(route);
        *self.tunnel.write().unwrap() = Some(t);
        *self.domains.write().unwrap() = domains;
        *self.exes.write().unwrap() = exes;

        let shared = self.tunnel.clone();
        let doms = self.domains.clone();
        let current_route = self.route.clone();
        let data_root = root.to_path_buf();
        let h = app.clone();
        let task = tokio::spawn(async move {
            let serve = proxy::serve(listener, shared.clone(), doms, port);
            let supervise = async {
                // Раз в минуту настоящий запрос через туннель. Переподключение, если цикл
                // туннеля упал, сервер давно молчит или две проверки подряд не прошли.
                let mut failures = 0;
                loop {
                    tokio::time::sleep(PROBE_EVERY).await;
                    let current = shared.read().unwrap().clone();
                    let alive = match current {
                        Some(t) if t.healthy() => tokio::time::timeout(PROBE_TIMEOUT, trace_tunnel(t)).await.map(|r| r.is_ok()).unwrap_or(false),
                        _ => false,
                    };
                    failures = if alive { 0 } else { failures + 1 };
                    let dead = shared.read().unwrap().as_ref().map(|t| !t.healthy()).unwrap_or(true);
                    if !dead && failures < 2 {
                        continue;
                    }
                    failures = 0;
                    log(&h, "Связь с WARP пропала, переподключение…");
                    // Сначала прежний способ, затем подбор заново (провайдер мог поменять блокировку).
                    let prev = current_route.lock().unwrap().clone();
                    match tunnel::start(&acc, &options(&data_root, auto, prev), |l| log(&h, l)).await {
                        Ok((t, route)) => {
                            save_route(&data_root, &route);
                            *current_route.lock().unwrap() = Some(route);
                            if let Some(old) = shared.write().unwrap().replace(t) {
                                old.close();
                            }
                        }
                        Err(e) => log(&h, e),
                    }
                    let _ = h.emit("warp-state", ());
                }
            };
            tokio::select! {
                _ = serve => {}
                _ = supervise => {}
            }
        });

        if let Err(e) = sysproxy::enable(root, &Self::pac_url(port)) {
            task.abort();
            if let Some(old) = self.tunnel.write().unwrap().take() {
                old.close();
            }
            return Err(e);
        }
        let redirector = apps.and_then(|bin| {
            let h = app.clone();
            match apps::Redirector::start(&bin, self.tunnel.clone(), self.domains.clone(), self.exes.clone(), move |l| log(&h, l)) {
                Ok(r) => {
                    log(app, "WARP для программ включён");
                    Some(r)
                }
                Err(e) => {
                    log(app, e);
                    None
                }
            }
        });
        {
            let mut g = self.inner.lock().unwrap();
            g.task = Some(task);
            g.since = Some(now_unix());
            g.last_error = None;
            g.root = Some(root.to_path_buf());
            g.port = port;
            g.apps = redirector;
        }
        let _ = app.emit("warp-state", ());
        Ok(())
    }

    pub fn stop(&self, app: &AppHandle) {
        let (had, previous) = {
            let mut g = self.inner.lock().unwrap();
            g.since = None;
            if let Some(a) = g.apps.take() {
                a.stop();
            }
            (g.task.take().map(|t| t.abort()).is_some(), g.root.take())
        };
        if let Some(old) = self.tunnel.write().unwrap().take() {
            old.close();
        }
        if had {
            if let Some(root) = previous {
                sysproxy::disable(&root);
            }
            log(app, "WARP выключен");
            let _ = app.emit("warp-state", ());
        }
    }

    pub fn clear_error(&self) {
        self.inner.lock().unwrap().last_error = None;
    }

    /// Проверка: через туннель запрашивает cloudflare.com/cdn-cgi/trace.
    pub async fn trace(&self) -> Result<Trace, String> {
        let t = self.tunnel.read().unwrap().clone().ok_or("WARP не запущен")?;
        trace_tunnel(t).await
    }
}

const PROBE_EVERY: Duration = Duration::from_secs(60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// Запрос cloudflare.com/cdn-cgi/trace через туннель: адрес и страна выхода.
async fn trace_tunnel(t: Arc<tunnel::Tunnel>) -> Result<Trace, String> {
    let ip = tokio::net::lookup_host("cloudflare.com:80")
        .await
        .map_err(|e| e.to_string())?
        .find_map(|a| match a { std::net::SocketAddr::V4(v) => Some(v), _ => None })
        .ok_or("cloudflare.com: адрес не найден")?;
    let mut c = t.connect(ip).await?;
    c.tx
        .send(b"GET /cdn-cgi/trace HTTP/1.1\r\nHost: cloudflare.com\r\nConnection: close\r\n\r\n".to_vec())
        .await
        .map_err(|_| "Туннель WARP остановлен".to_string())?;
    t.wake().notify_one();
    let mut body = Vec::new();
    while let Ok(Some(chunk)) = tokio::time::timeout(Duration::from_secs(10), c.rx.recv()).await {
        body.extend_from_slice(&chunk);
    }
    let text = String::from_utf8_lossy(&body);
    let field = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or_default().to_string();
    if field("ip").is_empty() {
        return Err("Сервер не ответил через WARP".into());
    }
    Ok(Trace { ip: field("ip"), loc: field("loc"), colo: field("colo"), warp: field("warp") == "on" })
}

/// При запуске с выключенным WARP: если после аварийного завершения остался наш PAC,
/// возвращаем настройку прокси, сохранённую до включения WARP.
pub fn cleanup_stale(root: &Path) {
    sysproxy::disable(root);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Живая проверка: cargo test --lib live_warp -- --ignored --nocapture
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_warp() {
        let acc = api::register().await.unwrap();
        println!("зарегистрирован: адрес {}, точка {}", acc.address_v4, acc.endpoint);
        let t = tunnel::start(&acc, &tunnel::Options { auto: true, ..Default::default() }, |l| println!("{l}")).await.unwrap().0;
        let tr = trace_tunnel(t.clone()).await.unwrap();
        println!("проверка туннеля: {} {} warp={}", tr.ip, tr.loc, tr.warp);
        assert!(tr.warp);
        let shared: Shared = Arc::new(RwLock::new(Some(t)));

        // Прокси на случайном порту: HTTP CONNECT и обычный HTTP через WARP.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(proxy::serve(listener, shared, Arc::new(RwLock::new(vec!["cloudflare.com".into()])), port));
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).unwrap())
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();
        let https = client.get("https://www.cloudflare.com/cdn-cgi/trace").send().await.unwrap().text().await.unwrap();
        println!("HTTPS через прокси:\n{https}");
        assert!(https.contains("warp=on"));
        let http = client.get("http://cloudflare.com/cdn-cgi/trace").send().await.unwrap().text().await.unwrap();
        assert!(http.contains("warp=on"));

        // Видят ли сервисы страну: OpenAI отвечает 403 unsupported_country для закрытых стран.
        if std::env::var("Z2_GEO").is_ok() {
            let t = client.get("https://chatgpt.com/cdn-cgi/trace").send().await.unwrap().text().await.unwrap();
            println!("chatgpt.com видит: {}", t.lines().filter(|l| l.starts_with("loc=") || l.starts_with("colo=")).collect::<Vec<_>>().join(" "));
            let r = client.get("https://api.openai.com/v1/models").send().await.unwrap();
            let code = r.status();
            let body = r.text().await.unwrap_or_default();
            println!("api.openai.com: HTTP {code} {}", body.chars().take(160).collect::<String>().replace('\n', " "));
        }

        // Скорость: 10 МБ через туннель.
        let started = std::time::Instant::now();
        let bytes = client.get("https://speed.cloudflare.com/__down?bytes=10000000").send().await.unwrap().bytes().await.unwrap();
        let secs = started.elapsed().as_secs_f64();
        println!("10 МБ за {secs:.1} с: {:.1} Мбит/с", bytes.len() as f64 * 8.0 / secs / 1e6);
        assert_eq!(bytes.len(), 10_000_000);
    }

    /// WARP для программ, нужны права администратора и загруженный zapret2:
    /// cargo test --lib live_apps -- --ignored --nocapture
    /// curl не знает о системном прокси, поэтому через WARP он может пойти только через перехват.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_apps() {
        let curl = |url: &str, fmt: Option<&str>| {
            let mut c = std::process::Command::new("curl.exe");
            c.args(["-s", "--max-time", "30"]);
            if let Some(f) = fmt {
                c.args(["-o", "NUL", "-w", f]);
            }
            String::from_utf8_lossy(&c.arg(url).output().unwrap().stdout).to_string()
        };
        let direct = curl("https://www.cloudflare.com/cdn-cgi/trace", None);
        println!("без перехвата:\n{direct}");

        let acc = api::register().await.unwrap();
        let t = tunnel::start(&acc, &tunnel::Options { auto: true, ..Default::default() }, |l| println!("{l}")).await.unwrap().0;
        let shared: Shared = Arc::new(RwLock::new(Some(t)));
        let domains: proxy::Domains = Arc::new(RwLock::new(vec!["cloudflare.com".into()]));
        let bin = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("Zapret2GUI").join("engine").join("bin");
        let r = apps::Redirector::start(&bin, shared, domains, Arc::default(), |l| println!("{l}")).expect("WinDivert: нужны права администратора");
        tokio::time::sleep(Duration::from_secs(3)).await;

        let via = tokio::task::spawn_blocking(move || {
            let trace = curl("https://www.cloudflare.com/cdn-cgi/trace", None);
            let other = curl("https://ya.ru", Some("%{http_code}"));
            let speed = curl("https://speed.cloudflare.com/__down?bytes=10000000", Some("%{speed_download}"));
            (trace, other, speed)
        })
        .await
        .unwrap();
        r.stop();
        println!("через перехват:\n{}", via.0);
        println!("ya.ru напрямую: HTTP {}", via.1);
        let bps: f64 = via.2.trim().parse().unwrap_or(0.0);
        println!("скорость curl через WARP: {:.1} Мбит/с", bps * 8.0 / 1e6);
        assert!(via.0.contains("warp=on"), "curl пошёл не через WARP");
        assert!(via.1.starts_with('2') || via.1.starts_with('3'), "сайт не из списка перестал открываться");
    }
}

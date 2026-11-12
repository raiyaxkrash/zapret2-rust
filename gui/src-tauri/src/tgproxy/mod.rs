//! Встроенный MTProto-прокси для Telegram Desktop.
//!
//! Прокси слушает 127.0.0.1:<порт>, принимает обфусцированные соединения Telegram
//! Desktop (секрет dd…) и пересылает их серверам Telegram через WebSocket (TLS),
//! а если это не получается — через Cloudflare-прокси или напрямую по TCP.
//! Работает внутри приложения, независимо от winws2. Протокол и маршруты
//! повторяют tg-ws-proxy (https://github.com/Flowseal/tg-ws-proxy, MIT).

mod obfs;
mod relay;
mod ws;

use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::{net::TcpListener, task::JoinSet};

use crate::{util::now_unix, AppState};

pub const DEFAULT_PORT: u16 = 1443;
const LOG_PREFIX: &str = "[tg]";
const STATS_EVERY: Duration = Duration::from_secs(600);
const CF_REFRESH_EVERY: Duration = Duration::from_secs(3600);

#[derive(Default)]
struct Inner {
    task: Option<tokio::task::JoinHandle<()>>,
    since: Option<u64>,
    last_error: Option<String>,
}

#[derive(Default)]
pub struct TgProxy {
    inner: Mutex<Inner>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TgProxyStatus {
    /// Прокси встроен в приложение и доступен всегда (поле оставлено для интерфейса).
    pub available: bool,
    pub running: bool,
    pub since: Option<u64>,
    pub last_error: Option<String>,
    pub port: u16,
    /// Ссылка для подключения (tg://proxy?...), пустая, если секрет ещё не создан.
    pub link: String,
}

/// Случайный секрет из 16 байт (32 hex-символа).
pub fn new_secret() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("системный генератор случайных чисел недоступен");
    hex::encode(b)
}

pub fn valid_secret(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Ссылка, которую понимает Telegram Desktop. Префикс dd — обфусцированный
/// транспорт с паддингом.
pub fn link(port: u16, secret: &str) -> String {
    if !valid_secret(secret) {
        return String::new();
    }
    format!("tg://proxy?server=127.0.0.1&port={port}&secret=dd{}", secret.to_lowercase())
}

fn log(app: &AppHandle, line: impl AsRef<str>) {
    app.state::<AppState>().engine.log(app, format!("{LOG_PREFIX} {}", line.as_ref()));
}

fn human_bytes(n: u64) -> String {
    let mut v = n as f64;
    for unit in ["Б", "КБ", "МБ", "ГБ"] {
        if v < 1024.0 {
            return format!("{v:.1} {unit}");
        }
        v /= 1024.0;
    }
    format!("{v:.1} ТБ")
}

impl TgProxy {
    pub fn status(&self, port: u16, secret: &str) -> TgProxyStatus {
        let g = self.inner.lock().unwrap();
        let running = g.task.as_ref().map(|t| !t.is_finished()).unwrap_or(false);
        TgProxyStatus {
            available: true,
            running,
            since: if running { g.since } else { None },
            last_error: g.last_error.clone(),
            port,
            link: link(port, secret),
        }
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().unwrap().task.as_ref().map(|t| !t.is_finished()).unwrap_or(false)
    }

    fn emit_state(&self, app: &AppHandle) {
        let _ = app.emit("tg-proxy-state", ());
    }

    pub async fn start(&self, app: &AppHandle, port: u16, secret: &str) -> Result<(), String> {
        self.stop(app);
        let r = self.spawn(app, port, secret).await;
        if let Err(e) = &r {
            log(app, e);
            self.inner.lock().unwrap().last_error = Some(e.clone());
            self.emit_state(app);
        }
        r
    }

    async fn spawn(&self, app: &AppHandle, port: u16, secret: &str) -> Result<(), String> {
        if !valid_secret(secret) {
            return Err("Некорректный секрет прокси".into());
        }
        let mut key = [0u8; 16];
        hex::decode_to_slice(secret, &mut key).map_err(|_| "Некорректный секрет прокси".to_string())?;
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).await.map_err(|e| {
            format!("Порт {port} уже занят другой программой (возможно, запущен TgWsProxy). Закройте её или выберите другой порт в настройках. ({e})")
        })?;

        let h = app.clone();
        let logger: relay::Logger = Arc::new(move |line| log(&h, line));
        let router = relay::Router::new(key, logger.clone());
        let task = tokio::spawn(serve(listener, router, logger));
        {
            let mut g = self.inner.lock().unwrap();
            g.task = Some(task);
            g.since = Some(now_unix());
            g.last_error = None;
        }
        log(app, format!("Прокси для Telegram работает на 127.0.0.1:{port}"));
        self.emit_state(app);
        Ok(())
    }

    pub fn stop(&self, app: &AppHandle) {
        let had = {
            let mut g = self.inner.lock().unwrap();
            g.since = None;
            match g.task.take() {
                Some(t) => {
                    // Задача сервера владеет всеми соединениями: при отмене они закрываются.
                    t.abort();
                    true
                }
                None => false,
            }
        };
        if had {
            log(app, "Прокси для Telegram остановлен");
            self.emit_state(app);
        }
    }

    pub fn clear_error(&self) {
        self.inner.lock().unwrap().last_error = None;
    }
}

/// Сервер: приём подключений, пул WebSocket, обновление списка доменов, статистика.
/// Все задачи живут в одном JoinSet и завершаются вместе с ним.
async fn serve(listener: TcpListener, router: Arc<relay::Router>, log: relay::Logger) {
    let mut tasks = JoinSet::new();
    tasks.spawn(Arc::clone(&router).maintain_pool());
    {
        let router = Arc::clone(&router);
        let log = log.clone();
        tasks.spawn(async move {
            loop {
                match relay::fetch_cf_domains().await {
                    Ok(list) => router.set_cf_domains(list),
                    Err(e) => log(format!("Не удалось обновить список запасных серверов: {e}")),
                }
                tokio::time::sleep(CF_REFRESH_EVERY).await;
            }
        });
    }
    {
        let router = Arc::clone(&router);
        let log = log.clone();
        tasks.spawn(async move {
            let mut last = (0, 0, 0);
            loop {
                tokio::time::sleep(STATS_EVERY).await;
                let s = &router.stats;
                let now = (
                    s.connections.load(Ordering::Relaxed),
                    s.bytes_up.load(Ordering::Relaxed),
                    s.bytes_down.load(Ordering::Relaxed),
                );
                if now != last {
                    log(format!("Подключений: {}, отправлено {}, получено {}", now.0, human_bytes(now.1), human_bytes(now.2)));
                    last = now;
                }
            }
        });
    }

    let mut errors = 0u32;
    loop {
        // Завершённые соединения убираем, чтобы JoinSet не рос.
        while tasks.try_join_next().is_some() {}
        match listener.accept().await {
            Ok((stream, _)) => {
                errors = 0;
                tasks.spawn(Arc::clone(&router).handle(stream));
            }
            Err(e) => {
                errors += 1;
                if errors == 1 || errors.is_multiple_of(50) {
                    log(format!("Ошибка приёма подключения: {e}"));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets() {
        let a = new_secret();
        let b = new_secret();
        assert!(valid_secret(&a) && valid_secret(&b));
        assert_ne!(a, b);
        assert!(!valid_secret("xyz"));
        assert!(!valid_secret(&"g".repeat(32)));
    }

    #[test]
    fn links() {
        let s = "0123456789ABCDEF0123456789abcdef";
        assert_eq!(link(1443, s), "tg://proxy?server=127.0.0.1&port=1443&secret=dd0123456789abcdef0123456789abcdef");
        assert_eq!(link(1443, "bad"), "");
    }
}

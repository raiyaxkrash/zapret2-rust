//! Локальный MTProto-прокси для Telegram Desktop на базе tg-ws-proxy
//! (https://github.com/Flowseal/tg-ws-proxy, MIT).
//!
//! Прокси слушает 127.0.0.1:<порт> и пересылает трафик Telegram к его серверам
//! через WebSocket (TLS), поэтому работает независимо от winws2.
//! Сам прокси собран из исходников скриптом gui/scripts/build-tg-proxy.ps1
//! и лежит в ресурсах приложения: resources/tg-ws-proxy/tg-ws-proxy.exe.

use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    io::{BufRead, BufReader, Read},
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    util::{hidden_command, now_unix},
    AppState,
};

pub const DEFAULT_PORT: u16 = 1443;
const LOG_PREFIX: &str = "[tg]";

#[derive(Default)]
struct Inner {
    child: Option<Child>,
    since: Option<u64>,
    stopping: bool,
    generation: u64,
    last_error: Option<String>,
}

#[derive(Default)]
pub struct TgProxy {
    inner: Mutex<Inner>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TgProxyStatus {
    /// Прокси входит в состав установленного приложения.
    pub available: bool,
    pub running: bool,
    pub since: Option<u64>,
    pub last_error: Option<String>,
    pub port: u16,
    /// Ссылка для подключения (tg://proxy?...), пустая, если секрет ещё не создан.
    pub link: String,
    pub version: String,
}

/// Каталог прокси в ресурсах приложения.
pub fn dir(resources: &Path) -> PathBuf {
    resources.join("tg-ws-proxy")
}

pub fn exe(resources: &Path) -> PathBuf {
    dir(resources).join("tg-ws-proxy.exe")
}

pub fn version(resources: &Path) -> String {
    std::fs::read_to_string(dir(resources).join("version.txt"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Случайный секрет из 16 байт (32 hex-символа). RandomState берёт начальное
/// значение из системного генератора случайных чисел.
pub fn new_secret() -> String {
    let part = || {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(now_unix());
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

pub fn valid_secret(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Ссылка, которую понимает Telegram Desktop. Префикс dd — обфусцированный
/// транспорт с паддингом, как в самом tg-ws-proxy.
pub fn link(port: u16, secret: &str) -> String {
    if !valid_secret(secret) {
        return String::new();
    }
    format!("tg://proxy?server=127.0.0.1&port={port}&secret=dd{}", secret.to_lowercase())
}

fn log(app: &AppHandle, line: impl AsRef<str>) {
    app.state::<AppState>().engine.log(app, format!("{LOG_PREFIX} {}", line.as_ref()));
}

impl TgProxy {
    pub fn status(&self, resources: &Path, port: u16, secret: &str) -> TgProxyStatus {
        let mut g = self.inner.lock().unwrap();
        let running = matches!(g.child.as_mut().map(|c| c.try_wait()), Some(Ok(None)));
        TgProxyStatus {
            available: exe(resources).exists(),
            running,
            since: if running { g.since } else { None },
            last_error: g.last_error.clone(),
            port,
            link: link(port, secret),
            version: version(resources),
        }
    }

    pub fn is_running(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        matches!(g.child.as_mut().map(|c| c.try_wait()), Some(Ok(None)))
    }

    fn emit_state(&self, app: &AppHandle) {
        let _ = app.emit("tg-proxy-state", ());
    }

    fn set_error(&self, app: &AppHandle, msg: String) {
        log(app, &msg);
        self.inner.lock().unwrap().last_error = Some(msg);
        self.emit_state(app);
    }

    /// Запускает прокси и ждёт, пока он начнёт принимать подключения.
    pub async fn start(self: &Arc<Self>, app: &AppHandle, resources: &Path, port: u16, secret: &str) -> Result<(), String> {
        self.stop(app);
        let r = self.spawn(app, resources, port, secret).await;
        if let Err(e) = &r {
            self.stop(app);
            self.set_error(app, e.clone());
        }
        r
    }

    async fn spawn(self: &Arc<Self>, app: &AppHandle, resources: &Path, port: u16, secret: &str) -> Result<(), String> {
        let exe = exe(resources);
        if !exe.exists() {
            return Err("Прокси для Telegram не входит в эту сборку приложения".into());
        }
        if !valid_secret(secret) {
            return Err("Некорректный секрет прокси".into());
        }
        // Занятый порт — частая причина: например, уже запущен TgWsProxy.
        if TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).is_err() {
            return Err(format!(
                "Порт {port} уже занят другой программой (возможно, запущен TgWsProxy). Закройте её или выберите другой порт в настройках."
            ));
        }

        let args = ["--host".to_string(), "127.0.0.1".into(), "--port".into(), port.to_string(), "--secret".into(), secret.to_lowercase()];
        log(app, format!("Запуск прокси для Telegram на 127.0.0.1:{port}"));
        let mut child = hidden_command(&exe.to_string_lossy())
            .args(&args)
            .current_dir(dir(resources))
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Не удалось запустить прокси для Telegram: {e}"))?;
        crate::engine::job::attach(&child);
        if let Some(out) = child.stdout.take() {
            spawn_reader(app.clone(), out);
        }
        if let Some(err) = child.stderr.take() {
            spawn_reader(app.clone(), err);
        }

        let generation = {
            let mut g = self.inner.lock().unwrap();
            g.child = Some(child);
            g.since = Some(now_unix());
            g.stopping = false;
            g.last_error = None;
            g.generation += 1;
            g.generation
        };

        // Ждём, пока прокси откроет порт (до 10 с: первый запуск может быть медленным
        // из-за проверки антивирусом).
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if !self.is_running() {
                return Err("Прокси для Telegram сразу завершился. Подробности в журнале.".into());
            }
            if std::net::TcpStream::connect_timeout(&SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(), Duration::from_millis(200)).is_ok() {
                self.spawn_monitor(app.clone(), generation);
                self.emit_state(app);
                log(app, "Прокси для Telegram работает");
                return Ok(());
            }
        }
        Err("Прокси для Telegram не ответил за 10 секунд. Подробности в журнале.".into())
    }

    fn spawn_monitor(self: &Arc<Self>, app: AppHandle, generation: u64) {
        let me = Arc::clone(self);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(500));
            let mut crashed_after_work = false;
            let exited = {
                let mut g = me.inner.lock().unwrap();
                if g.generation != generation {
                    return;
                }
                let st = match g.child.as_mut() {
                    Some(c) => c.try_wait(),
                    None => return,
                };
                match st {
                    Ok(Some(st)) => {
                        let worked = g.since.map(|s| now_unix().saturating_sub(s)).unwrap_or(0);
                        g.child = None;
                        g.since = None;
                        if !g.stopping {
                            g.last_error = Some(format!(
                                "Прокси для Telegram неожиданно завершился (код {}). Подробности в журнале.",
                                st.code().unwrap_or(-1)
                            ));
                            crashed_after_work = worked >= 10;
                        }
                        true
                    }
                    _ => false,
                }
            };
            if exited {
                if let Some(e) = me.inner.lock().unwrap().last_error.clone() {
                    log(&app, e);
                }
                me.emit_state(&app);
                if crashed_after_work {
                    let _ = app.emit("tg-proxy-crashed", ());
                }
                return;
            }
        });
    }

    pub fn stop(&self, app: &AppHandle) {
        let had = {
            let mut g = self.inner.lock().unwrap();
            g.generation += 1;
            g.since = None;
            match g.child.take() {
                Some(mut c) => {
                    g.stopping = true;
                    let _ = c.kill();
                    let _ = c.wait();
                    g.stopping = false;
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

fn spawn_reader<R: Read + Send + 'static>(app: AppHandle, stream: R) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                    // Секрет в журнал не выводим: журнал часто копируют целиком в чаты поддержки.
                    if line.is_empty() || line.contains("Secret:") || line.contains("tg://proxy") {
                        continue;
                    }
                    log(&app, line);
                }
            }
        }
    });
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

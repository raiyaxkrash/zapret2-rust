//! Диагностика одной кнопкой: что именно мешает открывать сервисы и что с этим делать.
//! Различает отсутствие интернета, подмену DNS, блокировку по IP, обрыв соединения
//! системой DPI (это лечит обход) и ограничение со стороны самого сервиса (это лечит WARP).

use std::{
    net::{IpAddr, SocketAddr},
    sync::atomic::Ordering,
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::{net::TcpStream, time::timeout};

use crate::{tester, AppState};

#[derive(Serialize, Clone, Debug)]
pub struct DiagItem {
    pub title: String,
    /// "good" | "warn" | "bad" | "info"
    pub status: &'static str,
    pub detail: String,
    pub advice: String,
}

fn item(title: impl Into<String>, status: &'static str, detail: impl Into<String>, advice: impl Into<String>) -> DiagItem {
    DiagItem { title: title.into(), status, detail: detail.into(), advice: advice.into() }
}

/// Домены для проверки DNS.
const DNS_DOMAINS: [&str; 5] = ["youtube.com", "discord.com", "instagram.com", "x.com", "rutracker.org"];
/// Сервисы для проверки блокировок: (название, адрес).
const SERVICES: [(&str, &str); 4] = [
    ("YouTube", "https://www.youtube.com/"),
    ("Discord", "https://discord.com/"),
    ("Instagram", "https://www.instagram.com/"),
    ("X (Twitter)", "https://x.com/"),
];

fn bogus(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_unspecified() || v.is_link_local(),
        IpAddr::V6(v) => v.is_loopback() || v.is_unspecified(),
    }
}

async fn system_resolve(domain: &str) -> Vec<IpAddr> {
    match timeout(Duration::from_secs(5), tokio::net::lookup_host((domain, 443))).await {
        Ok(Ok(it)) => it.map(|a| a.ip()).filter(|ip| ip.is_ipv4()).collect(),
        _ => vec![],
    }
}

/// Ответ DNS-over-HTTPS (Cloudflare, запасной — Google).
async fn doh_resolve(c: &reqwest::Client, domain: &str) -> Option<Vec<IpAddr>> {
    for url in [
        format!("https://cloudflare-dns.com/dns-query?name={domain}&type=A"),
        format!("https://dns.google/resolve?name={domain}&type=A"),
    ] {
        let Ok(resp) = c.get(&url).header("accept", "application/dns-json").send().await else { continue };
        let Ok(v) = resp.json::<serde_json::Value>().await else { continue };
        let ips: Vec<IpAddr> = v["Answer"]
            .as_array()
            .map(|a| a.iter().filter(|r| r["type"] == 1).filter_map(|r| r["data"].as_str()?.parse().ok()).collect())
            .unwrap_or_default();
        return Some(ips);
    }
    None
}

async fn tcp_ok(ip: IpAddr) -> bool {
    matches!(timeout(Duration::from_secs(5), TcpStream::connect(SocketAddr::new(ip, 443))).await, Ok(Ok(_)))
}

pub async fn run(app: AppHandle) -> Vec<DiagItem> {
    let state = app.state::<AppState>();
    let mut out = Vec::new();
    let c = match reqwest::Client::builder().user_agent("zapret2-gui").timeout(Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(e) => return vec![item("Диагностика", "bad", e.to_string(), "")],
    };
    let running = state.engine.is_running();

    // 1. Интернет.
    let net = tester::check("net".into(), "https://ya.ru/".into()).await;
    if !net.ok {
        out.push(item("Интернет", "bad", "Не открывается даже ya.ru", "Проверьте подключение к интернету, затем повторите диагностику."));
        return out;
    }
    out.push(item("Интернет", "good", format!("Подключение есть ({} мс)", net.ms), ""));

    // 2. DNS.
    let mut poisoned = Vec::new();
    let mut doh_failed = true;
    for d in DNS_DOMAINS {
        let sys = system_resolve(d).await;
        let Some(doh) = doh_resolve(&c, d).await else { continue };
        doh_failed = false;
        let fake = (sys.is_empty() && !doh.is_empty()) || (!sys.is_empty() && sys.iter().all(bogus));
        if fake {
            poisoned.push(d);
        }
    }
    out.push(if doh_failed {
        item("DNS", "info", "Не удалось сравнить с DNS-over-HTTPS: сервисы Cloudflare и Google DNS недоступны", "")
    } else if poisoned.is_empty() {
        item("DNS", "good", "Подмены адресов не найдено", "")
    } else {
        item(
            "DNS",
            "bad",
            format!("Провайдер подменяет адреса: {}", poisoned.join(", ")),
            "Включите DNS-over-HTTPS: Параметры Windows → Сеть и Интернет → свойства подключения → DNS → 1.1.1.1 с шифрованием HTTPS.",
        )
    });

    // 3. Сервисы: как именно они блокируются.
    for (name, url) in SERVICES {
        let host = url.trim_start_matches("https://").trim_end_matches('/');
        let ips = {
            let sys = system_resolve(host).await;
            if sys.iter().all(bogus) { doh_resolve(&c, host).await.unwrap_or_default() } else { sys }
        };
        let r = tester::check(name.into(), url.into()).await;
        let how = if running { "с обходом" } else { "без обхода" };
        if r.ok {
            out.push(item(name, "good", format!("Открывается {how} ({} мс)", r.ms), ""));
            continue;
        }
        let tcp = match ips.first() {
            Some(ip) => tcp_ok(*ip).await,
            None => false,
        };
        let (detail, advice) = if ips.is_empty() {
            ("Адрес не определяется".to_string(), "Похоже на блокировку через DNS: включите DNS-over-HTTPS.".to_string())
        } else if !tcp {
            (
                "Сервер не отвечает: похоже на блокировку по IP-адресу".to_string(),
                "Обход по DPI тут не поможет. Попробуйте WARP или проверьте, работает ли сервис сейчас.".to_string(),
            )
        } else {
            (
                format!("Соединение обрывается ({}): так работает система DPI", r.error.clone().unwrap_or_default()),
                if running {
                    "Текущая стратегия не помогает: подберите стратегию заново на странице «Стратегии».".to_string()
                } else {
                    "Включите обход: это как раз его случай.".to_string()
                },
            )
        };
        out.push(item(name, "bad", detail, advice));
    }

    // 4. Ограничение со стороны самого сервиса (на примере ChatGPT).
    let warp_on = state.warp.is_running();
    match c.get("https://api.openai.com/v1/models").send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            if status == 403 && body.contains("unsupported_country") {
                out.push(item(
                    "ChatGPT и другие нейросети",
                    "warn",
                    "Сервис сам закрыл доступ из вашего региона",
                    if warp_on {
                        "WARP включён: проверьте, что набор «Нейросети» выбран на странице WARP. Эта проверка идёт напрямую, а браузер пойдёт через WARP."
                    } else {
                        "Обход DPI тут не поможет: нужен иностранный IP-адрес. Включите WARP."
                    },
                ));
            } else {
                out.push(item("ChatGPT и другие нейросети", "good", "Доступ из вашего региона не ограничен", ""));
            }
        }
        Err(_) => out.push(item("ChatGPT и другие нейросети", "info", "Не удалось проверить", "")),
    }

    // 5. WARP.
    if state.settings.lock().unwrap().warp {
        let w = state.warp.clone();
        out.push(match w.trace().await {
            Ok(t) if t.warp => item("WARP", "good", format!("Работает: адрес {} ({})", t.ip, t.loc), ""),
            Ok(t) => item("WARP", "warn", format!("Ответ получен, но WARP не активен ({})", t.ip), "Выключите и включите WARP на странице WARP."),
            Err(e) => item("WARP", "bad", e, "Возможно, провайдер блокирует WARP. Проверьте, что включена «Маскировка» на странице WARP."),
        });
    }

    // 6. Состояние приложения.
    let conflicts = state.engine.conflicts();
    if !conflicts.is_empty() {
        let names: Vec<String> = conflicts.iter().map(|c| c.name.clone()).collect();
        out.push(item(
            "Другие программы обхода",
            "bad",
            format!("Запущены: {}", names.join(", ")),
            "Они используют тот же драйвер и мешают Zapret2. Закройте их кнопкой «Завершить» на главной.",
        ));
    }
    let ver = state.paths.engine_version();
    out.push(if ver.is_empty() || !state.paths.winws().exists() {
        item("zapret2", "warn", "Ещё не загружен", "Он скачается автоматически при включении обхода.")
    } else if running {
        item("zapret2", "good", format!("Версия {ver}, обход включён"), "")
    } else {
        item("zapret2", "info", format!("Версия {ver}, обход выключен"), "")
    });
    if state.testing.load(Ordering::SeqCst) {
        out.push(item("Подбор стратегии", "info", "Сейчас идёт подбор: результаты могут быть неточными", ""));
    }
    out
}

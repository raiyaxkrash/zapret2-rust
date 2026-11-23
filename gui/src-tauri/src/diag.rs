//! Диагностика одной кнопкой: что именно мешает открывать сервисы и что с этим делать.
//! Различает отсутствие интернета, подмену DNS, блокировку по IP, обрыв соединения
//! системой DPI (это лечит обход) и ограничение со стороны самого сервиса.

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
    /// Быстрое действие в интерфейсе: "start_engine" | "strategies" | "warp" | "kill_conflicts"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<&'static str>,
}

fn item(title: impl Into<String>, status: &'static str, detail: impl Into<String>, advice: impl Into<String>) -> DiagItem {
    DiagItem { title: title.into(), status, detail: detail.into(), advice: advice.into(), action: None }
}

fn item_act(
    title: impl Into<String>,
    status: &'static str,
    detail: impl Into<String>,
    advice: impl Into<String>,
    action: &'static str,
) -> DiagItem {
    DiagItem { title: title.into(), status, detail: detail.into(), advice: advice.into(), action: Some(action) }
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

/// Ответ DNS-over-HTTPS напрямую по IP (Cloudflare 1.1.1.1, запасной — Google 8.8.8.8),
/// без зависимости от системного DNS и прокси.
async fn doh_resolve(_c: &reqwest::Client, domain: &str) -> Option<Vec<IpAddr>> {
    crate::dns_guard::resolve_doh_a(domain).await
}

async fn tcp_ok(ip: IpAddr) -> bool {
    matches!(timeout(Duration::from_secs(5), TcpStream::connect(SocketAddr::new(ip, 443))).await, Ok(Ok(_)))
}

async fn check_service(c: reqwest::Client, name: &'static str, url: &'static str, running: bool) -> DiagItem {
    let host = url.trim_start_matches("https://").trim_end_matches('/');
    let ips = {
        let sys = system_resolve(host).await;
        if sys.is_empty() || sys.iter().all(bogus) {
            doh_resolve(&c, host).await.unwrap_or(sys)
        } else {
            sys
        }
    };
    let r = tester::check(name.into(), url.into()).await;
    let how = if running { "с обходом" } else { "без обхода" };
    if r.ok {
        return item(name, "good", format!("Открывается {how} ({} мс)", r.ms), "");
    }
    let tcp = match ips.first() {
        Some(ip) => tcp_ok(*ip).await,
        None => false,
    };
    if ips.is_empty() {
        item(name, "bad", "Адрес не определяется", "Похоже на блокировку через DNS: включите DNS-over-HTTPS.")
    } else if !tcp {
        item_act(
            name,
            "bad",
            "Сервер не отвечает: похоже на блокировку по IP-адресу",
            "Обход по DPI тут не поможет. Попробуйте WARP или проверьте, работает ли сервис сейчас.",
            "warp",
        )
    } else if running {
        item_act(
            name,
            "bad",
            format!("Соединение обрывается ({}): так работает система DPI", r.error.unwrap_or_default()),
            "Текущая стратегия не помогает: подберите стратегию заново на странице «Стратегии».",
            "strategies",
        )
    } else {
        item_act(
            name,
            "bad",
            format!("Соединение обрывается ({}): так работает система DPI", r.error.unwrap_or_default()),
            "Включите обход: это как раз его случай.",
            "start_engine",
        )
    }
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

    // 2. DNS (параллельно по всем тестовым доменам).
    let dns_tasks: Vec<_> = DNS_DOMAINS
        .iter()
        .map(|&d| {
            let client = c.clone();
            tauri::async_runtime::spawn(async move {
                let sys = system_resolve(d).await;
                let doh = doh_resolve(&client, d).await;
                (d, sys, doh)
            })
        })
        .collect();

    let mut poisoned = Vec::new();
    let mut doh_failed = true;
    for task in dns_tasks {
        let Ok((d, sys, Some(doh))) = task.await else { continue };
        doh_failed = false;
        let fake = (sys.is_empty() && !doh.is_empty()) || (!sys.is_empty() && sys.iter().all(bogus));
        if fake {
            poisoned.push(d);
        }
    }
    let doh_active = state.dns_guard.is_running();
    out.push(if doh_failed {
        item("DNS", "info", "Не удалось сравнить с DNS-over-HTTPS: сервисы Cloudflare и Google DNS недоступны", "")
    } else if poisoned.is_empty() {
        item(
            "DNS",
            "good",
            if doh_active { "Подмены адресов не найдено (защита DNS включена)" } else { "Подмены адресов не найдено" },
            "",
        )
    } else {
        item_act(
            "DNS",
            "bad",
            format!("Провайдер подменяет адреса: {}", poisoned.join(", ")),
            "Включите встроенную защиту DNS одной кнопкой или настройте DNS-over-HTTPS (1.1.1.1) в параметрах Windows.",
            "enable_doh",
        )
    });

    // 3. Сервисы и проверка региональных ограничений — параллельно, чтобы не ждать таймауты по очереди.
    let svc_tasks: Vec<_> = SERVICES
        .iter()
        .map(|&(name, url)| {
            let client = c.clone();
            tauri::async_runtime::spawn(async move { check_service(client, name, url, running).await })
        })
        .collect();

    let geo_client = c.clone();
    let geo_task = tauri::async_runtime::spawn(async move {
        match geo_client.get("https://api.openai.com/v1/models").send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                if status == 403 && body.contains("unsupported_country") {
                    item(
                        "ChatGPT и другие нейросети",
                        "warn",
                        "Сервис сам закрыл доступ из вашего региона",
                        "Ни обход, ни WARP тут не помогут: сервис проверяет страну, а Cloudflare показывает ему вашу настоящую страну. Нужен VPN с сервером за границей.",
                    )
                } else {
                    item("ChatGPT и другие нейросети", "good", "Доступ из вашего региона не ограничен", "")
                }
            }
            Err(_) => item("ChatGPT и другие нейросети", "info", "Не удалось проверить", ""),
        }
    });

    for task in svc_tasks {
        if let Ok(res) = task.await {
            out.push(res);
        }
    }
    if let Ok(geo) = geo_task.await {
        out.push(geo);
    }

    // 5. WARP.
    if state.settings.lock().unwrap().warp {
        let w = state.warp.clone();
        out.push(match w.trace().await {
            Ok(t) if t.warp => item("WARP", "good", format!("Работает: адрес {} ({})", t.ip, t.loc), "Сайты видят через WARP эту же страну: закрытые для неё сервисы WARP не откроет."),
            Ok(t) => item_act("WARP", "warn", format!("Ответ получен, но WARP не активен ({})", t.ip), "Выключите и включите WARP на странице WARP.", "warp"),
            Err(e) => item_act("WARP", "bad", e, "Возможно, провайдер блокирует WARP. Проверьте, что включена «Маскировка» на странице WARP.", "warp"),
        });
    }

    // 6. Состояние приложения.
    let conflicts = state.engine.conflicts();
    if !conflicts.is_empty() {
        let names: Vec<String> = conflicts.iter().map(|c| c.name.clone()).collect();
        out.push(item_act(
            "Другие программы обхода",
            "bad",
            format!("Запущены: {}", names.join(", ")),
            "Они используют тот же драйвер и мешают Zapret2. Закройте их кнопкой «Отключить» на главной.",
            "kill_conflicts",
        ));
    }
    let ver = state.paths.engine_version();
    out.push(if ver.is_empty() || !state.paths.winws().exists() {
        item("zapret2", "warn", "Ещё не загружен", "Он скачается автоматически при включении обхода.")
    } else if running {
        item("zapret2", "good", format!("Версия {ver}, обход включён"), "")
    } else {
        item_act("zapret2", "info", format!("Версия {ver}, обход выключен"), "", "start_engine")
    });
    if state.testing.load(Ordering::SeqCst) {
        out.push(item("Подбор стратегии", "info", "Сейчас идёт подбор: результаты могут быть неточными", ""));
    }
    out
}

//! Автоподбор стратегии: по очереди запускаем winws2 с каждой стратегией
//! и проверяем, открываются ли тестовые адреса.

use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{paths::first_domains, presets::Presets, settings::Settings, AppState};

/// Сколько байт ответа нужно получить, чтобы считать сайт доступным.
/// ТСПУ часто пропускает начало соединения и обрывает его после ~16 КБ.
const ENOUGH_BYTES: usize = 64 * 1024;

#[derive(Serialize, Clone, Debug)]
pub struct CheckResult {
    pub url: String,
    /// "youtube" | "discord" | "custom"
    pub service: String,
    pub ok: bool,
    pub ms: u64,
    pub bytes: usize,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct StrategyResult {
    pub id: String,
    pub name: String,
    pub ok: usize,
    pub total: usize,
    pub avg_ms: u64,
    pub error: Option<String>,
    pub checks: Vec<CheckResult>,
    /// Отдельно по YouTube и по остальным сервисам.
    pub yt_ok: usize,
    pub yt_total: usize,
    pub other_ok: usize,
    pub other_total: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct TestReport {
    pub baseline: StrategyResult,
    pub results: Vec<StrategyResult>,
    /// Лучшая стратегия в целом (по сервисам, кроме YouTube, если они проверялись).
    pub best: Option<String>,
    /// Лучшая стратегия для YouTube.
    pub best_youtube: Option<String>,
    pub cancelled: bool,
}

#[derive(Serialize, Clone)]
struct Progress<'a> {
    index: usize,
    total: usize,
    current: &'a str,
    result: Option<&'a StrategyResult>,
}

/// Адреса для проверки: (сервис, url).
pub fn targets(settings: &Settings, state: &AppState) -> Vec<(String, String)> {
    let mut t = Vec::new();
    let mut add = |svc: &str, url: &str| t.push((svc.to_string(), url.to_string()));
    let custom_only = settings.custom && !settings.youtube && !settings.discord;
    if settings.youtube || (!settings.discord && !custom_only) {
        add("youtube", "https://www.youtube.com/");
        add("youtube", "https://i.ytimg.com/vi/jNQXAC9IVRw/maxresdefault.jpg");
        add("youtube", "https://redirector.googlevideo.com/report_mapping");
        add("youtube", "https://yt3.ggpht.com/favicon.ico");
    }
    if settings.discord || (!settings.youtube && !custom_only) {
        add("discord", "https://discord.com/");
        add("discord", "https://gateway.discord.gg/");
        add("discord", "https://cdn.discordapp.com/");
    }
    if settings.custom {
        for d in first_domains(&state.paths.lists.join("user.txt"), 3) {
            add("custom", &format!("https://{}/", d.trim_start_matches("*.")));
        }
    }
    t
}

async fn check(service: String, url: String) -> CheckResult {
    let started = Instant::now();
    let fail = |url: String, service: String, ms: u64, bytes: usize, e: String| CheckResult { url, service, ok: false, ms, bytes, error: Some(e) };
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(9))
        .pool_max_idle_per_host(0)
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36")
        .build();
    let client = match client {
        Ok(c) => c,
        Err(e) => return fail(url, service, 0, 0, e.to_string()),
    };
    let mut bytes = 0usize;
    let res: Result<(), reqwest::Error> = async {
        let mut resp = client.get(&url).send().await?;
        while let Some(chunk) = resp.chunk().await? {
            bytes += chunk.len();
            if bytes >= ENOUGH_BYTES {
                break;
            }
        }
        Ok(())
    }
    .await;
    let ms = started.elapsed().as_millis() as u64;
    match res {
        Ok(()) => CheckResult { url, service, ok: true, ms, bytes, error: None },
        Err(e) => {
            let msg = if e.is_timeout() {
                if bytes > 0 { format!("обрыв после {} КБ", bytes / 1024) } else { "таймаут".into() }
            } else if e.is_connect() {
                "не удалось подключиться".into()
            } else {
                short_error(&e)
            };
            fail(url, service, ms, bytes, msg)
        }
    }
}

fn short_error(e: &reqwest::Error) -> String {
    let mut s = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(inner) = src {
        s = inner.to_string();
        src = inner.source();
    }
    s.chars().take(120).collect()
}

fn summarize(id: &str, name: &str, checks: Vec<CheckResult>, total: usize, error: Option<String>) -> StrategyResult {
    let ok = checks.iter().filter(|c| c.ok).count();
    let avg_ms = if ok > 0 { checks.iter().filter(|c| c.ok).map(|c| c.ms).sum::<u64>() / ok as u64 } else { 0 };
    let yt = |c: &&CheckResult| c.service == "youtube";
    StrategyResult {
        id: id.into(),
        name: name.into(),
        ok,
        total,
        avg_ms,
        error,
        yt_ok: checks.iter().filter(yt).filter(|c| c.ok).count(),
        yt_total: checks.iter().filter(yt).count(),
        other_ok: checks.iter().filter(|c| !yt(c)).filter(|c| c.ok).count(),
        other_total: checks.iter().filter(|c| !yt(c)).count(),
        checks,
    }
}

async fn run_checks(id: &str, name: &str, urls: &[(String, String)]) -> StrategyResult {
    let handles: Vec<_> = urls
        .iter()
        .cloned()
        .map(|(s, u)| tauri::async_runtime::spawn(check(s, u)))
        .collect();
    let mut checks = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            checks.push(r);
        }
    }
    let total = checks.len();
    summarize(id, name, checks, total, None)
}

/// Лучшая стратегия по метрике (успехи, затем скорость).
fn pick(results: &[StrategyResult], score: impl Fn(&StrategyResult) -> usize) -> Option<String> {
    results
        .iter()
        .filter(|r| score(r) > 0)
        .max_by(|a, b| score(a).cmp(&score(b)).then(b.avg_ms.cmp(&a.avg_ms)))
        .map(|r| r.id.clone())
}

pub async fn run(app: AppHandle) -> Result<TestReport, String> {
    let state = app.state::<AppState>();
    if state.testing.swap(true, Ordering::SeqCst) {
        return Err("Подбор уже выполняется".into());
    }
    state.cancel_test.store(false, Ordering::SeqCst);
    let res = run_inner(&app).await;
    let state = app.state::<AppState>();
    state.testing.store(false, Ordering::SeqCst);
    res
}

async fn run_inner(app: &AppHandle) -> Result<TestReport, String> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    let presets = Presets::load(&state.paths)?;
    let urls = targets(&settings, &state);
    let was_running = state.engine.is_running();
    let strategies = presets.all(&settings);
    let total = strategies.len() + 1;

    state.engine.stop(app);
    state.engine.log(app, format!("[app] Подбор стратегии: {} адресов, {} стратегий", urls.len(), strategies.len()));

    let _ = app.emit("test-progress", Progress { index: 0, total, current: "Проверка без обхода", result: None });
    let baseline = run_checks("baseline", "Без обхода", &urls).await;
    let _ = app.emit("test-progress", Progress { index: 1, total, current: "", result: Some(&baseline) });

    let mut results = Vec::new();
    let mut cancelled = false;
    for (i, s) in strategies.iter().enumerate() {
        if state.cancel_test.load(Ordering::SeqCst) {
            cancelled = true;
            break;
        }
        let _ = app.emit("test-progress", Progress { index: i + 1, total, current: &s.name, result: None });
        // Проверяемая стратегия применяется ко всем сервисам, включая YouTube.
        let r = match crate::start_with(app, &settings, &s.id, Some(&s.id)).await {
            Ok(()) => {
                let r = run_checks(&s.id, &s.name, &urls).await;
                state.engine.stop(app);
                r
            }
            Err(e) => {
                state.engine.stop(app);
                summarize(&s.id, &s.name, vec![], urls.len(), Some(e))
            }
        };
        state.engine.log(
            app,
            format!("[app] {}: {}/{} (YouTube {}/{})", r.name, r.ok, r.total, r.yt_ok, r.yt_total),
        );
        let _ = app.emit("test-progress", Progress { index: i + 2, total, current: "", result: Some(&r) });
        results.push(r);
    }

    let has_other = urls.iter().any(|(s, _)| s != "youtube");
    let has_yt = urls.iter().any(|(s, _)| s == "youtube");
    let best = if has_other { pick(&results, |r| r.other_ok) } else { pick(&results, |r| r.ok) };
    let best_youtube = if has_yt { pick(&results, |r| r.yt_ok) } else { None };

    if was_running {
        let id = settings.strategy.clone();
        if let Err(e) = crate::start_with(app, &settings, &id, None).await {
            state.engine.log(app, format!("[app] Не удалось вернуть обход: {e}"));
        }
    }

    let report = TestReport { baseline, results, best, best_youtube, cancelled };
    *state.last_report.lock().unwrap() = Some(report.clone());
    Ok(report)
}


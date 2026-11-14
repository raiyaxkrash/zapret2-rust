//! Сторож: пока обход включён, раз в 15 минут проверяет сервисы. Если сервис раньше
//! открывался, а теперь дважды подряд не открывается (провайдер поменял блокировку),
//! сторож переподбирает стратегию сам или предупреждает пользователя.

use std::{
    collections::{HashMap, HashSet},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{tester, AppState};

const FIRST_CHECK: Duration = Duration::from_secs(5 * 60);
const INTERVAL: Duration = Duration::from_secs(15 * 60);
/// Не чаще раза в 6 часов, чтобы не мешать, если блокировку не обойти.
const COOLDOWN: Duration = Duration::from_secs(6 * 3600);
const FAILS_TO_ACT: u32 = 2;
/// Адрес, который открывается без обхода: если и он недоступен, пропал интернет целиком.
const CONNECTIVITY_URL: &str = "https://ya.ru/";

#[derive(Serialize, Clone)]
pub struct Broken {
    /// Сервисы, которые перестали открываться ("youtube", "discord", "custom", id набора).
    pub services: Vec<String>,
    /// Сторож уже переподобрал стратегию сам.
    pub repicked: bool,
    /// Переподбор помог: все эти сервисы снова открываются.
    pub fixed: bool,
}

/// Состояние между проверками.
#[derive(Default)]
struct Tracker {
    worked: HashSet<String>,
    fails: HashMap<String, u32>,
}

impl Tracker {
    /// Учитывает результат проверки и возвращает сервисы, которые «сломались».
    fn update(&mut self, by_service: &HashMap<String, bool>) -> Vec<String> {
        let mut broken = Vec::new();
        for (svc, ok) in by_service {
            if *ok {
                self.worked.insert(svc.clone());
                self.fails.remove(svc);
            } else if self.worked.contains(svc) {
                let n = self.fails.entry(svc.clone()).or_default();
                *n += 1;
                if *n >= FAILS_TO_ACT {
                    broken.push(svc.clone());
                }
            }
        }
        broken.sort();
        broken
    }

    fn reset(&mut self) {
        self.fails.clear();
    }
}

/// Сервис открывается, если открылся хотя бы один его адрес.
fn by_service(r: &tester::StrategyResult) -> HashMap<String, bool> {
    let mut m: HashMap<String, bool> = HashMap::new();
    for c in &r.checks {
        *m.entry(c.service.clone()).or_default() |= c.ok;
    }
    m
}

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tracker = Tracker::default();
        let mut last_action: Option<Instant> = None;
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            check_once(&app, &mut tracker, &mut last_action).await;
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

async fn check_once(app: &AppHandle, tracker: &mut Tracker, last_action: &mut Option<Instant>) {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    if !settings.watchdog || !state.engine.is_running() || state.testing.load(Ordering::SeqCst) {
        tracker.reset();
        return;
    }
    let urls = tester::targets(&settings, &state);
    if urls.is_empty() {
        return;
    }
    let r = tester::run_checks("watchdog", "Сторож", &urls).await;
    let services = by_service(&r);
    if services.values().all(|ok| !ok) {
        // Всё недоступно — возможно, пропал интернет. Тогда это не поломка стратегии.
        let net = tester::run_checks("net", "Интернет", &[("net".into(), CONNECTIVITY_URL.into())]).await;
        if net.ok == 0 {
            tracker.reset();
            return;
        }
    }
    let broken = tracker.update(&services);
    if broken.is_empty() || last_action.map(|t| t.elapsed() < COOLDOWN).unwrap_or(false) {
        return;
    }
    *last_action = Some(Instant::now());
    tracker.reset();
    state.engine.log(app, format!("[app] Сторож: перестали открываться {}", broken.join(", ")));

    if !settings.watchdog_auto {
        let _ = app.emit("strategy-broken", Broken { services: broken, repicked: false, fixed: false });
        return;
    }

    let _ = app.emit("toast", "Сервисы перестали открываться. Zapret2 подбирает стратегию заново, это займёт около минуты.");
    let report = {
        let _guard = state.busy.lock().await;
        tester::run(app.clone(), None).await
    };
    let fixed = match report {
        Ok(rep) if rep.best.is_some() || rep.best_youtube.is_some() => {
            if let Err(e) = crate::apply_report(app, &rep).await {
                state.engine.log(app, format!("[app] Сторож: не удалось применить стратегию: {e}"));
            }
            let settings = state.settings.lock().unwrap().clone();
            let after = by_service(&tester::run_checks("after", "После", &tester::targets(&settings, &state)).await);
            broken.iter().all(|s| after.get(s).copied().unwrap_or(false))
        }
        Ok(_) => false,
        Err(e) => {
            state.engine.log(app, format!("[app] Сторож: подбор не удался: {e}"));
            false
        }
    };
    state.engine.log(app, if fixed { "[app] Сторож: новая стратегия применена, сервисы снова открываются" } else { "[app] Сторож: подходящая стратегия не найдена" });
    let _ = app.emit("strategy-broken", Broken { services: broken, repicked: true, fixed });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, bool)]) -> HashMap<String, bool> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn needs_two_fails_after_working() {
        let mut t = Tracker::default();
        // Сервис, который никогда не открывался, тревогу не поднимает.
        assert!(t.update(&m(&[("youtube", false)])).is_empty());
        assert!(t.update(&m(&[("youtube", false)])).is_empty());
        assert!(t.update(&m(&[("youtube", true), ("discord", true)])).is_empty());
        assert!(t.update(&m(&[("youtube", false), ("discord", true)])).is_empty());
        assert_eq!(t.update(&m(&[("youtube", false), ("discord", true)])), vec!["youtube"]);
        // Восстановился — счётчик сброшен.
        assert!(t.update(&m(&[("youtube", true)])).is_empty());
        assert!(t.update(&m(&[("youtube", false)])).is_empty());
    }
}

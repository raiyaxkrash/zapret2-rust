//! Память стратегий по сетям. Блокировки у разных провайдеров разные: дома работает одна
//! стратегия, через раздачу с телефона другая. Для каждой сети (провайдера, номер AS)
//! запоминается история стратегий, и при переходе в другую сеть включается та,
//! что работала в ней в прошлый раз.

use std::{
    fs,
    net::{IpAddr, UdpSocket},
    path::Path,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::{isp, settings::Settings, util::now_unix, AppState};

const HISTORY: usize = 10;
const WATCH_EVERY: Duration = Duration::from_secs(20);
/// После смены сети даём подключению устояться, прежде чем определять провайдера.
const SETTLE: Duration = Duration::from_secs(5);

/// Стратегии, которые были выбраны в сети.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub strategy: String,
    #[serde(default)]
    pub youtube_strategy: String,
    #[serde(default = "yes")]
    pub quic: bool,
    pub at: u64,
    /// Откуда: "test" (подбор), "manual" (выбрана вручную), "rollback" (возврат после
    /// неудачного переподбора), "restore" (возвращена из истории), "initial" (была при первом входе в сеть).
    pub source: String,
}

fn yes() -> bool {
    true
}

impl Entry {
    pub fn from_settings(s: &Settings, source: &str) -> Entry {
        Entry { strategy: s.strategy.clone(), youtube_strategy: s.youtube_strategy.clone(), quic: s.quic, at: now_unix(), source: source.into() }
    }

    fn same(&self, o: &Entry) -> bool {
        self.strategy == o.strategy && self.youtube_strategy == o.youtube_strategy && self.quic == o.quic
    }

    pub fn matches(&self, s: &Settings) -> bool {
        self.strategy == s.strategy && self.youtube_strategy == s.youtube_strategy && self.quic == s.quic
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Network {
    pub asn: u32,
    pub name: String,
    /// Новые записи первыми; первая — текущая стратегия этой сети.
    pub history: Vec<Entry>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Memory {
    pub networks: Vec<Network>,
}

impl Memory {
    pub fn load(path: &Path) -> Memory {
        fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        let _ = fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default());
    }

    pub fn get(&self, asn: u32) -> Option<&Network> {
        self.networks.iter().find(|n| n.asn == asn)
    }

    /// Записывает стратегию для сети; повтор текущей не записывается. true, если изменилось.
    pub fn record(&mut self, asn: u32, name: &str, e: Entry) -> bool {
        let net = match self.networks.iter().position(|n| n.asn == asn) {
            Some(i) => &mut self.networks[i],
            None => {
                self.networks.push(Network { asn, name: name.into(), history: Vec::new() });
                self.networks.last_mut().unwrap()
            }
        };
        if !name.is_empty() {
            net.name = name.into();
        }
        if net.history.first().map(|h| h.same(&e)).unwrap_or(false) {
            return false;
        }
        net.history.insert(0, e);
        net.history.truncate(HISTORY);
        true
    }
}

fn file(root: &Path) -> std::path::PathBuf {
    root.join("networks.json")
}

/// Запоминает текущие настройки стратегии для сети, в которой сейчас компьютер.
pub fn remember(app: &AppHandle, source: &str) {
    let state = app.state::<AppState>();
    let s = state.settings.lock().unwrap().clone();
    if !s.network_memory {
        return;
    }
    let entry = Entry::from_settings(&s, source);
    let known = state.isp.lock().unwrap().clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let isp = match known {
            Some(i) => i,
            None => match isp::detect().await {
                Ok(i) => {
                    *app.state::<AppState>().isp.lock().unwrap() = Some(i.clone());
                    i
                }
                Err(_) => return,
            },
        };
        let root = app.state::<AppState>().paths.root.clone();
        let mut m = Memory::load(&file(&root));
        if m.record(isp.asn, &isp.name, entry) {
            m.save(&file(&root));
            let _ = app.emit("networks-changed", ());
        }
    });
}

/// История текущей сети для интерфейса.
#[derive(Serialize)]
pub struct View {
    pub asn: u32,
    pub name: String,
    pub history: Vec<Entry>,
}

pub fn current_view(app: &AppHandle) -> Option<View> {
    let state = app.state::<AppState>();
    let isp = state.isp.lock().unwrap().clone()?;
    let m = Memory::load(&file(&state.paths.root));
    let n = m.get(isp.asn)?;
    Some(View { asn: n.asn, name: n.name.clone(), history: n.history.clone() })
}

/// Запись из истории текущей сети по времени.
pub fn find(app: &AppHandle, at: u64) -> Option<Entry> {
    let state = app.state::<AppState>();
    let isp = state.isp.lock().unwrap().clone()?;
    Memory::load(&file(&state.paths.root)).get(isp.asn)?.history.iter().find(|e| e.at == at).cloned()
}

/// Локальный адрес, через который компьютер выходит в интернет. Пакеты не отправляются:
/// UDP-сокет только выбирает маршрут.
fn fingerprint() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("1.1.1.1:53").ok()?;
    Some(s.local_addr().ok()?.ip())
}

/// Следит за сменой сети и включает стратегию, которая в ней работала.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_fp: Option<IpAddr> = None;
        let mut last_asn: Option<u32> = None;
        loop {
            let fp = tokio::task::spawn_blocking(fingerprint).await.ok().flatten();
            if fp.is_some() && fp != last_fp {
                tokio::time::sleep(SETTLE).await;
                if let Ok(isp) = isp::detect().await {
                    last_fp = fp;
                    let changed = last_asn != Some(isp.asn);
                    last_asn = Some(isp.asn);
                    *app.state::<AppState>().isp.lock().unwrap() = Some(isp.clone());
                    if changed {
                        let _ = app.emit("isp-changed", ());
                        entered(&app, &isp).await;
                    }
                }
            } else if fp.is_none() {
                last_fp = None;
            }
            tokio::time::sleep(WATCH_EVERY).await;
        }
    });
}

/// Компьютер в сети `isp`: включаем её стратегию или запоминаем текущую как её.
async fn entered(app: &AppHandle, isp: &isp::Isp) {
    let state = app.state::<AppState>();
    let s = state.settings.lock().unwrap().clone();
    if !s.network_memory || state.testing.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let root = state.paths.root.clone();
    let mut m = Memory::load(&file(&root));
    let Some(saved) = m.get(isp.asn).and_then(|n| n.history.first()).cloned() else {
        if m.record(isp.asn, &isp.name, Entry::from_settings(&s, "initial")) {
            m.save(&file(&root));
        }
        return;
    };
    if saved.matches(&s) {
        return;
    }
    let who = if isp.name.is_empty() { format!("AS{}", isp.asn) } else { isp.name.clone() };
    state.engine.log(app, format!("[app] Сеть сменилась ({who}): включаю стратегию, которая в ней работала"));
    if crate::set_strategy(app, &saved, None).await.is_ok() {
        let _ = app.emit("notice", format!("Сеть сменилась: {who}. Включена стратегия, которая работала в этой сети."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(strategy: &str, at: u64) -> Entry {
        Entry { strategy: strategy.into(), youtube_strategy: String::new(), quic: true, at, source: "test".into() }
    }

    #[test]
    fn records_per_network_without_repeats() {
        let mut m = Memory::default();
        assert!(m.record(12389, "Rostelecom", e("a", 1)));
        assert!(!m.record(12389, "Rostelecom", e("a", 2)), "та же стратегия не дублируется");
        assert!(m.record(12389, "", e("b", 3)));
        assert!(m.record(8359, "MTS", e("c", 4)));
        let r = m.get(12389).unwrap();
        assert_eq!(r.name, "Rostelecom");
        assert_eq!(r.history.iter().map(|x| x.strategy.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(m.get(8359).unwrap().history[0].strategy, "c");
        for i in 0..20 {
            m.record(8359, "MTS", e(&format!("s{i}"), 10 + i));
        }
        assert_eq!(m.get(8359).unwrap().history.len(), HISTORY);
        assert_eq!(m.get(8359).unwrap().history[0].strategy, "s19");
    }

    #[test]
    fn fingerprint_works_offline_safe() {
        // Без сети вернёт None, с сетью — локальный адрес; главное, что не паникует.
        let _ = fingerprint();
    }
}

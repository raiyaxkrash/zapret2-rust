use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

/// Адрес манифеста обновлений приложения (latest.json из GitHub Releases).
/// Его можно переопределить в настройках.
pub const DEFAULT_APP_UPDATE_URL: &str = "https://github.com/raiyaxkrash/zapret2-rust/releases/latest/download/latest.json";

/// Сохранённая пользователем именованная стратегия обхода.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CustomStrategy {
    pub id: String,
    pub name: String,
    pub tls: String,
    pub http: String,
    #[serde(default)]
    pub quic: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// id выбранной стратегии ("custom" — свои параметры).
    pub strategy: String,
    /// Отдельная стратегия для YouTube; пустая строка — как основная.
    pub youtube_strategy: String,

    pub youtube: bool,
    pub discord: bool,
    pub discord_voice: bool,
    pub custom: bool,
    /// "list" — только сайты из «Моих сайтов», "auto" — плюс автоматически найденные, "all" — все сайты, кроме исключений.
    pub custom_mode: String,
    pub quic: bool,
    /// Включённые готовые наборы сайтов (id из presets.json).
    pub packs: Vec<String>,
    /// Временный профиль для проверки конкретного сайта (не сохраняется).
    #[serde(skip)]
    pub test_domains: Vec<String>,

    pub autostart: bool,
    pub start_minimized: bool,
    pub engine_on_launch: bool,
    pub close_to_tray: bool,
    /// Глобальная горячая клавиша Ctrl+Shift+Z для переключения обхода.
    pub hotkey: bool,
    pub debug_log: bool,
    /// Встроенная защита от подмены DNS (DNS-over-HTTPS поверх WinDivert).
    pub dns_doh: bool,
    /// Провайдер DoH: "auto" | "cloudflare" | "google" | "quad9" | "adguard" | "custom".
    pub doh_provider: String,
    /// Пользовательский URL DoH (для doh_provider == "custom").
    pub doh_custom_url: String,

    /// Встроенный WARP для заблокированных по IP сайтов и программ.
    pub warp: bool,
    pub warp_port: u16,
    /// «Мусорные» пакеты перед рукопожатием, чтобы WARP хуже распознавался.
    pub warp_junk: bool,
    /// Перехват для программ, которые не используют системный прокси.
    pub warp_apps: bool,
    /// Включённые наборы сервисов WARP (id из presets.json, поле warp_packs).
    pub warp_packs: Vec<String>,
    /// Свои домены через WARP.
    pub warp_custom: Vec<String>,
    /// Программы (.exe), весь трафик которых направляется в WARP.
    pub warp_exes: Vec<String>,
    /// Необязательный ключ подписки 1.1.1.1 WARP+.
    pub warp_license: String,

    /// Сторож: следить, не перестали ли открываться сервисы.
    pub watchdog: bool,
    /// Сторож сам переподбирает стратегию (иначе только предупреждает).
    pub watchdog_auto: bool,
    /// Помнить стратегию для каждой сети и включать её при смене сети.
    pub network_memory: bool,

    pub engine_auto_update: bool,
    pub content_auto_update: bool,
    pub app_auto_check: bool,
    pub app_update_url: String,

    /// "system" | "light" | "dark"
    pub theme: String,
    /// Режим для опытных: стратегии, журнал и тонкие настройки видны.
    pub expert: bool,
    /// Язык интерфейса: "auto" | "ru" | "en".
    pub lang: String,
    pub animations: bool,
    /// Сохранённое положение и размер главного окна: [x, y, width, height].
    pub window_bounds: Option<[i32; 4]>,

    /// Локальный прокси для Telegram Desktop (tg-ws-proxy).
    pub tg_proxy: bool,
    pub tg_proxy_port: u16,
    /// Секрет MTProto (32 hex-символа). Создаётся один раз, чтобы ссылка в Telegram не менялась.
    pub tg_proxy_secret: String,

    pub custom_tls: String,
    pub custom_http: String,
    pub custom_quic: String,
    /// Сохранённые пользовательские стратегии (именованные пресеты).
    pub custom_strategies: Vec<CustomStrategy>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            strategy: "fake_multisplit_md5".into(),
            youtube_strategy: String::new(),
            youtube: true,
            discord: true,
            discord_voice: true,
            custom: false,
            custom_mode: "list".into(),
            quic: true,
            packs: Vec::new(),
            test_domains: Vec::new(),
            autostart: false,
            start_minimized: true,
            engine_on_launch: true,
            close_to_tray: true,
            hotkey: false,
            debug_log: false,
            dns_doh: false,
            doh_provider: "auto".into(),
            doh_custom_url: String::new(),
            warp: false,
            warp_port: crate::warp::DEFAULT_PORT,
            warp_junk: true,
            warp_apps: true,
            warp_packs: vec![],
            warp_custom: Vec::new(),
            warp_exes: Vec::new(),
            warp_license: String::new(),
            watchdog: true,
            watchdog_auto: true,
            network_memory: true,
            engine_auto_update: true,
            content_auto_update: true,
            app_auto_check: true,
            app_update_url: DEFAULT_APP_UPDATE_URL.into(),
            theme: "system".into(),
            expert: false,
            lang: "auto".into(),
            animations: true,
            window_bounds: None,
            tg_proxy: false,
            tg_proxy_port: crate::tgproxy::DEFAULT_PORT,
            tg_proxy_secret: String::new(),
            custom_tls: "--payload=tls_client_hello --lua-desync=fake:blob=tls_google:tcp_md5:repeats=6 --lua-desync=multisplit:pos=1,midsld".into(),
            custom_http: "--payload=http_req --lua-desync=fake:blob=fake_default_http:tcp_md5:repeats=6 --lua-desync=multisplit:pos=method+2".into(),
            custom_quic: "--payload=quic_initial --lua-desync=fake:blob=quic_google:repeats=6".into(),
            custom_strategies: Vec::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        let raw = fs::read_to_string(path).ok();
        let mut s: Settings = raw.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        // Простой режим появился в 2.4.1: у тех, кто обновляется, всё остаётся на месте.
        if let Some(v) = raw.as_deref().and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok()) {
            if v.is_object() && v.get("expert").is_none() {
                s.expert = true;
            }
        }
        if s.app_update_url.trim().is_empty() {
            s.app_update_url = DEFAULT_APP_UPDATE_URL.into();
        }
        if !crate::tgproxy::valid_secret(&s.tg_proxy_secret) {
            s.tg_proxy_secret = crate::tgproxy::new_secret();
        }
        if s.tg_proxy_port < 1024 {
            s.tg_proxy_port = crate::tgproxy::DEFAULT_PORT;
        }
        if s.warp_port < 1024 || s.warp_port == s.tg_proxy_port {
            s.warp_port = if s.tg_proxy_port == crate::warp::DEFAULT_PORT { 40081 } else { crate::warp::DEFAULT_PORT };
        }
        if let Ok(clean) = crate::util::normalize_domain_vec(&s.warp_custom) {
            s.warp_custom = clean;
        }
        if let Ok(clean_exes) = crate::util::normalize_exe_vec(&s.warp_exes) {
            s.warp_exes = clean_exes;
        }
        s
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| format!("Не удалось сохранить настройки: {e}"))?;
        fs::rename(&tmp, path).map_err(|e| format!("Не удалось сохранить настройки: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expert_mode_kept_for_existing_users() {
        let dir = std::env::temp_dir().join(format!("z2-settings-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        assert!(!Settings::load(&path).expert);
        fs::write(&path, r#"{"strategy":"seqovl"}"#).unwrap();
        assert!(Settings::load(&path).expert);
        let mut s = Settings::load(&path);
        s.expert = false;
        s.save(&path).unwrap();
        assert!(!Settings::load(&path).expert);
        let _ = fs::remove_dir_all(dir);
    }
}

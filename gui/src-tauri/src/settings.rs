use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

/// Адрес манифеста обновлений приложения (latest.json из GitHub Releases).
/// Его можно переопределить в настройках.
pub const DEFAULT_APP_UPDATE_URL: &str = "https://github.com/raiyaxkrash/zapret2-rust/releases/latest/download/latest.json";

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

    pub autostart: bool,
    pub start_minimized: bool,
    pub engine_on_launch: bool,
    pub close_to_tray: bool,
    pub debug_log: bool,

    pub engine_auto_update: bool,
    pub content_auto_update: bool,
    pub app_auto_check: bool,
    pub app_update_url: String,

    /// "system" | "light" | "dark"
    pub theme: String,

    pub custom_tls: String,
    pub custom_http: String,
    pub custom_quic: String,
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
            autostart: false,
            start_minimized: true,
            engine_on_launch: true,
            close_to_tray: true,
            debug_log: false,
            engine_auto_update: true,
            content_auto_update: true,
            app_auto_check: true,
            app_update_url: DEFAULT_APP_UPDATE_URL.into(),
            theme: "system".into(),
            custom_tls: "--payload=tls_client_hello --lua-desync=fake:blob=tls_google:tcp_md5:repeats=6 --lua-desync=multisplit:pos=1,midsld".into(),
            custom_http: "--payload=http_req --lua-desync=fake:blob=fake_default_http:tcp_md5:repeats=6 --lua-desync=multisplit:pos=method+2".into(),
            custom_quic: "--payload=quic_initial --lua-desync=fake:blob=quic_google:repeats=6".into(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        let mut s: Settings = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if s.app_update_url.trim().is_empty() {
            s.app_update_url = DEFAULT_APP_UPDATE_URL.into();
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

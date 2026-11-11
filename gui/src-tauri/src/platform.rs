//! Сведения о системе для оформления: версия Windows (Mica есть только в Windows 11)
//! и цвет акцента, выбранный пользователем в параметрах Windows.

use std::sync::OnceLock;

use serde::Serialize;
use tauri::{Manager, Runtime, Theme, WebviewWindow};

use crate::util::hidden_command;

#[derive(Serialize, Clone, Debug)]
pub struct Accent {
    /// Акцент для тёмной темы (AccentLight2).
    pub dark_theme: String,
    /// Акцент для светлой темы (AccentDark1).
    pub light_theme: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Platform {
    pub build: u32,
    pub mica: bool,
    pub accent: Accent,
}

fn reg_value(key: &str, value: &str) -> Option<String> {
    let out = hidden_command("reg").args(["query", key, "/v", value]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    text.lines()
        .find(|l| l.trim_start().starts_with(value))
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_string)
}

fn read_build() -> u32 {
    reg_value(r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn read_accent() -> Accent {
    let fallback = Accent { dark_theme: "#4cc2ff".into(), light_theme: "#005fb8".into() };
    let Some(hexstr) = reg_value(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent", "AccentPalette") else {
        return fallback;
    };
    let Ok(bytes) = hex::decode(hexstr.trim()) else { return fallback };
    if bytes.len() < 32 {
        return fallback;
    }
    // 8 цветов RGBA: Light3, Light2, Light1, Accent, Dark1, Dark2, Dark3, (резерв)
    let color = |i: usize| format!("#{:02x}{:02x}{:02x}", bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2]);
    Accent { dark_theme: color(1), light_theme: color(4) }
}

pub fn info() -> Platform {
    static CACHE: OnceLock<(u32, Accent)> = OnceLock::new();
    let (build, accent) = CACHE.get_or_init(|| (read_build(), read_accent())).clone();
    Platform { build, mica: build >= 22000, accent }
}

/// Применяет тему ("system" | "light" | "dark") и подложку Mica ко всем окнам.
pub fn apply_theme<R: Runtime, M: Manager<R>>(app: &M, theme: &str) {
    let t = match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    };
    for w in app.webview_windows().values() {
        apply_to(w, t);
    }
}

/// У интерфейса свой фон, поэтому системные подложки (Mica) не используются:
/// только тема окна, чтобы совпадали тень и системные элементы.
pub fn apply_to<R: Runtime>(w: &WebviewWindow<R>, theme: Option<Theme>) {
    let _ = w.set_theme(theme);
}

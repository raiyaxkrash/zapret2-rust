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
    /// Компьютер на ARM64 (x64-сборка приложения работает через эмуляцию Windows 11).
    pub arm64: bool,
    /// Включён режим тестовой подписи драйверов (нужен для ARM64-драйвера WinDivert).
    pub testsigning: bool,
    pub secure_boot: bool,
    /// Язык Windows для режима «Как в системе»: "ru" или "en".
    pub lang: &'static str,
}

pub fn is_arm64() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| {
        // Переменная процесса под эмуляцией показывает AMD64, поэтому берём системную.
        reg_value(r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment", "PROCESSOR_ARCHITECTURE")
            .map(|v| v.eq_ignore_ascii_case("ARM64"))
            .unwrap_or(false)
    })
}

/// Включён ли режим тестовой подписи. `bcdedit` показывает значение по-английски или по-русски.
pub fn testsigning_enabled() -> bool {
    let Ok(out) = hidden_command("bcdedit").args(["/enum", "{current}"]).output() else { return false };
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    text.lines()
        .find(|l| l.trim_start().starts_with("testsigning"))
        .map(|l| l.contains("yes") || l.contains("да"))
        .unwrap_or(false)
}

pub fn secure_boot_enabled() -> bool {
    reg_value(r"HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State", "UEFISecureBootEnabled")
        .map(|v| v.trim_start_matches("0x") == "1")
        .unwrap_or(false)
}

/// Включает режим тестовой подписи (только по явному запросу пользователя на ARM64).
pub fn enable_testsigning() -> Result<(), String> {
    if !is_arm64() {
        return Err("Тестовый режим нужен только на компьютерах с ARM64".into());
    }
    if secure_boot_enabled() {
        return Err("Включена безопасная загрузка (Secure Boot). Выключите её в настройках UEFI/BIOS, затем повторите.".into());
    }
    let out = hidden_command("bcdedit")
        .args(["/set", "{current}", "testsigning", "on"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("bcdedit: {}", String::from_utf8_lossy(&out.stdout).trim()));
    }
    Ok(())
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
    let arm64 = is_arm64();
    Platform {
        build,
        mica: build >= 22000,
        accent,
        arm64,
        // bcdedit и реестр опрашиваем только на ARM64: на остальных это не нужно.
        testsigning: arm64 && testsigning_enabled(),
        secure_boot: arm64 && secure_boot_enabled(),
        lang: system_lang(),
    }
}

/// Русский интерфейс для русского, украинского, белорусского и казахского языка Windows, иначе английский.
pub fn system_lang() -> &'static str {
    // SAFETY: функция без аргументов, только читает настройку пользователя.
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    match id & 0x3ff {
        0x19 | 0x22 | 0x23 | 0x3f => "ru",
        _ => "en",
    }
}

/// Английский интерфейс для этого значения настройки языка.
pub fn is_english(lang: &str) -> bool {
    match lang {
        "en" => true,
        "ru" => false,
        _ => system_lang() == "en",
    }
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

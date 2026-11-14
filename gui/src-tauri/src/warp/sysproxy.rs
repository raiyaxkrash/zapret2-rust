//! Системный PAC-файл Windows (настройки прокси WinINET): по нему браузеры и
//! приложения отправляют через WARP только выбранные сайты. Прежняя настройка
//! пользователя сохраняется на диск до изменения и возвращается при выключении WARP,
//! в том числе после аварийного завершения приложения.

use std::{fs, path::Path};

use windows_sys::Win32::Networking::WinInet::{InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED};

use crate::util::hidden_command;

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
const VALUE: &str = "AutoConfigURL";

/// Текущий адрес PAC в настройках Windows.
pub fn current() -> Option<String> {
    let out = hidden_command("reg").args(["query", KEY, "/v", VALUE]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    text.lines()
        .find(|l| l.trim_start().starts_with(VALUE))
        .and_then(|l| l.split("REG_SZ").nth(1))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Наш ли это PAC (по адресу локального прокси).
pub fn is_ours(url: &str) -> bool {
    url.starts_with("http://127.0.0.1:") && url.contains("/proxy.pac")
}

fn notify() {
    unsafe {
        InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null(), 0);
        InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_REFRESH, std::ptr::null(), 0);
    }
}

pub fn set(url: &str) -> Result<(), String> {
    let out = hidden_command("reg")
        .args(["add", KEY, "/v", VALUE, "/t", "REG_SZ", "/d", url, "/f"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("Не удалось включить системный прокси для WARP".into());
    }
    notify();
    Ok(())
}

fn backup_file(root: &Path) -> std::path::PathBuf {
    root.join("proxy-backup.txt")
}

/// Включает наш PAC. Прежняя настройка записывается на диск, если её там ещё нет:
/// файл от прошлого аварийного завершения хранит настоящую исходную настройку.
pub fn enable(root: &Path, url: &str) -> Result<(), String> {
    let file = backup_file(root);
    if !file.exists() {
        let previous = current().filter(|p| !is_ours(p)).unwrap_or_default();
        fs::write(&file, previous).map_err(|e| format!("Не удалось сохранить настройку прокси: {e}"))?;
    }
    set(url)
}

/// Возвращает настройку, сохранённую до включения WARP. Если за это время прокси
/// поменяла другая программа, её настройку не трогаем.
pub fn disable(root: &Path) {
    let file = backup_file(root);
    let saved = fs::read_to_string(&file).ok();
    if current().map(|u| is_ours(&u)).unwrap_or(false) {
        restore(saved.as_deref().map(str::trim).filter(|p| !p.is_empty()));
    }
    let _ = fs::remove_file(file);
}

/// Возвращает прежнюю настройку пользователя (или убирает PAC совсем).
pub fn restore(previous: Option<&str>) {
    match previous.filter(|p| !is_ours(p)) {
        Some(p) => {
            let _ = hidden_command("reg").args(["add", KEY, "/v", VALUE, "/t", "REG_SZ", "/d", p, "/f"]).output();
        }
        None => {
            let _ = hidden_command("reg").args(["delete", KEY, "/v", VALUE, "/f"]).output();
        }
    }
    notify();
}

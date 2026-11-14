//! Другие программы обхода: zapret-discord-youtube (winws.exe, служба «zapret»),
//! GoodbyeDPI (goodbyedpi.exe, служба «GoodbyeDPI») и другие сборки zapret.
//!
//! Вместе с обходом Zapret2 они работать не могут: каждый пакет менялся бы дважды.
//! Поэтому Zapret2 умеет: работать рядом с ними (WARP и Telegram без своего обхода),
//! отключать их службы так, чтобы они не вернулись после перезагрузки (с возможностью
//! вернуть), и переносить пользовательские списки сайтов.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{paths::Paths, util::hidden_command};

/// Службы, которые ставят другие программы обхода.
const SERVICES: &[&str] = &["zapret", "GoodbyeDPI"];
/// Процессы других программ обхода (сравнение без учёта регистра).
pub const PROCESSES: &[&str] = &["winws.exe", "winws2.exe", "goodbyedpi.exe"];
const SERVICES_KEY: &str = r"HKLM\SYSTEM\CurrentControlSet\Services";
/// Шаблонная строка из пустого списка zapret-discord-youtube.
const PLACEHOLDER: &str = "domain.example.abc";

#[derive(Serialize, Clone, Debug)]
pub struct Other {
    /// «zapret-discord-youtube», «GoodbyeDPI», «zapret» или «zapret2».
    pub product: String,
    /// Имя службы Windows, если программа установлена службой.
    pub service: Option<String>,
    /// Служба запускается вместе с Windows.
    pub autostart: bool,
    pub running: bool,
    pub folder: Option<String>,
    /// Сколько новых сайтов можно перенести в «Мои сайты» и «Исключения».
    pub import_user: usize,
    pub import_exclude: usize,
}

/// Служба, которую отключил Zapret2 (для кнопки «Вернуть»).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Disabled {
    pub service: String,
    pub product: String,
}

// ---------------------------------------------------------------- реестр и процессы

/// Значение из реестра целиком (пути с пробелами не обрезаются).
fn reg_value(key: &str, value: &str) -> Option<String> {
    let out = hidden_command("reg").args(["query", key, "/v", value]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let line = text.lines().find(|l| l.trim_start().to_lowercase().starts_with(&value.to_lowercase()))?;
    let rest = line.trim_start()[value.len()..].trim_start();
    let (_, data) = rest.split_once(char::is_whitespace)?;
    Some(data.trim().to_string())
}

fn service_exists(name: &str) -> bool {
    reg_value(&format!(r"{SERVICES_KEY}\{name}"), "ImagePath").is_some()
}

/// Путь к программе из строки запуска службы (`"C:\a b\winws.exe" --args` или без кавычек).
fn exe_from_command(cmd: &str) -> Option<PathBuf> {
    let cmd = cmd.trim();
    let path = if let Some(rest) = cmd.strip_prefix('"') {
        rest.split('"').next()?.to_string()
    } else {
        let lower = cmd.to_lowercase();
        let end = lower.find(".exe")? + 4;
        cmd[..end].to_string()
    };
    Some(PathBuf::from(path))
}

/// Аргумент `--name <value>` или `--name=<value>` из строки запуска (значение может быть в кавычках).
fn args_values(cmd: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cmd;
    while let Some(i) = rest.find(name) {
        let mut after = rest[i + name.len()..].trim_start_matches('=').trim_start();
        let value = if let Some(q) = after.strip_prefix('"') {
            let v = q.split('"').next().unwrap_or_default();
            after = &q[v.len().min(q.len())..];
            v.to_string()
        } else {
            let v = after.split_whitespace().next().unwrap_or_default();
            after = &after[v.len()..];
            v.to_string()
        };
        if !value.is_empty() {
            out.push(value);
        }
        rest = after;
    }
    out
}

/// Запущенные процессы других программ: имя и путь к файлу.
fn processes(own_pid: Option<u32>) -> Vec<(String, u32, Option<PathBuf>)> {
    let Ok(out) = hidden_command("tasklist").args(["/FO", "CSV", "/NH"]).output() else { return vec![] };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut parts = l.split("\",\"");
            let name = parts.next()?.trim_matches('"').to_string();
            let pid: u32 = parts.next()?.trim_matches('"').parse().ok()?;
            (PROCESSES.contains(&name.to_lowercase().as_str()) && Some(pid) != own_pid).then(|| (name, pid, process_path(pid)))
        })
        .collect()
}

fn process_path(pid: u32) -> Option<PathBuf> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    // SAFETY: дескриптор закрывается ниже, буфер живёт на время вызова.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

/// Папка программы: у zapret-discord-youtube и GoodbyeDPI исполняемый файл лежит во вложенной папке.
fn folder_of(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let name = dir.file_name()?.to_string_lossy().to_lowercase();
    if ["bin", "x86_64", "x86", "x64"].contains(&name.as_str()) {
        dir.parent().map(Path::to_path_buf)
    } else {
        Some(dir.to_path_buf())
    }
}

fn product_of(exe_name: &str, folder: Option<&Path>, service: Option<&str>) -> String {
    let n = exe_name.to_lowercase();
    if n == "goodbyedpi.exe" || service.map(|s| s.eq_ignore_ascii_case("GoodbyeDPI")).unwrap_or(false) {
        return "GoodbyeDPI".into();
    }
    if n == "winws2.exe" {
        return "zapret2".into();
    }
    let zdy_folder = folder.map(|f| f.join("service.bat").exists() && f.join("lists").join("list-general.txt").exists()).unwrap_or(false);
    let zdy_service = service.map(|s| reg_value(&format!(r"{SERVICES_KEY}\{s}"), "zapret-discord-youtube").is_some()).unwrap_or(false);
    if zdy_folder || zdy_service {
        "zapret-discord-youtube".into()
    } else {
        "zapret".into()
    }
}

// ---------------------------------------------------------------- списки

fn read_domains(p: &Path) -> Vec<String> {
    fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.eq_ignore_ascii_case(PLACEHOLDER))
        .filter_map(|l| crate::util::normalize_domain(l).ok())
        .collect()
}

/// Файлы списков программы: (для «Моих сайтов», для «Исключений»).
fn list_sources(product: &str, folder: &Path, command: Option<&str>) -> (Vec<PathBuf>, Vec<PathBuf>) {
    match product {
        "zapret-discord-youtube" => {
            let l = folder.join("lists");
            (vec![l.join("list-general-user.txt")], vec![l.join("list-exclude-user.txt")])
        }
        "GoodbyeDPI" => {
            // Свои списки из --blacklist; общий russia-blacklist.txt огромный и не нужен.
            let files = command
                .map(|c| args_values(c, "--blacklist"))
                .unwrap_or_default()
                .into_iter()
                .map(PathBuf::from)
                .map(|p| if p.is_absolute() { p } else { folder.join(p) })
                .filter(|p| !p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case("russia-blacklist.txt")).unwrap_or(false))
                .collect();
            (files, vec![])
        }
        _ => (vec![], vec![]),
    }
}

/// Сайты из файлов, которых ещё нет в нашем списке.
fn new_domains(files: &[PathBuf], ours: &Path) -> Vec<String> {
    let have: HashSet<String> = read_domains(ours).into_iter().collect();
    let mut seen = HashSet::new();
    files.iter().flat_map(|f| read_domains(f)).filter(|d| !have.contains(d) && seen.insert(d.clone())).collect()
}

// ---------------------------------------------------------------- поиск

struct Found {
    other: Other,
    command: Option<String>,
}

fn find(paths: &Paths, own_pid: Option<u32>) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    let procs = processes(own_pid);
    for svc in SERVICES {
        let key = format!(r"{SERVICES_KEY}\{svc}");
        let Some(cmd) = reg_value(&key, "ImagePath") else { continue };
        let exe = exe_from_command(&cmd);
        let folder = exe.as_deref().and_then(folder_of);
        let exe_name = exe.as_ref().and_then(|e| e.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let running = procs.iter().any(|(_, _, p)| p.is_some() && p.as_deref() == exe.as_deref());
        let start: u32 = reg_value(&key, "Start").and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()).unwrap_or(3);
        found.push(Found {
            other: Other {
                product: product_of(&exe_name, folder.as_deref(), Some(svc)),
                service: Some(svc.to_string()),
                autostart: start == 2,
                running,
                folder: folder.map(|f| f.to_string_lossy().to_string()),
                import_user: 0,
                import_exclude: 0,
            },
            command: Some(cmd),
        });
    }
    // Запущенные без службы (через .bat).
    for (name, _, path) in &procs {
        let folder = path.as_deref().and_then(folder_of);
        let f = folder.as_ref().map(|f| f.to_string_lossy().to_string());
        if found.iter().any(|x| x.other.folder.is_some() && x.other.folder == f) {
            continue;
        }
        found.push(Found {
            other: Other { product: product_of(name, folder.as_deref(), None), service: None, autostart: false, running: true, folder: f, import_user: 0, import_exclude: 0 },
            command: None,
        });
    }
    for x in &mut found {
        if let Some(folder) = x.other.folder.as_deref().map(PathBuf::from) {
            let (user, exclude) = list_sources(&x.other.product, &folder, x.command.as_deref());
            if let Some(ours) = paths.list_file("user") {
                x.other.import_user = new_domains(&user, &ours).len();
            }
            if let Some(ours) = paths.list_file("exclude") {
                x.other.import_exclude = new_domains(&exclude, &ours).len();
            }
        }
    }
    found
}

/// Установленные и запущенные программы обхода (кроме нашего winws2).
pub fn detect(paths: &Paths, own_pid: Option<u32>) -> Vec<Other> {
    find(paths, own_pid).into_iter().map(|f| f.other).collect()
}

/// Переносит пользовательские списки всех найденных программ. Возвращает (в «Мои сайты», в «Исключения»).
pub fn import_lists(paths: &Paths, own_pid: Option<u32>) -> Result<(usize, usize), String> {
    let mut added = (0, 0);
    for f in find(paths, own_pid) {
        let Some(folder) = f.other.folder.as_deref().map(PathBuf::from) else { continue };
        let (user, exclude) = list_sources(&f.other.product, &folder, f.command.as_deref());
        for (files, list, n) in [(user, "user", &mut added.0), (exclude, "exclude", &mut added.1)] {
            let Some(ours) = paths.list_file(list) else { continue };
            let new = new_domains(&files, &ours);
            if new.is_empty() {
                continue;
            }
            let mut text = fs::read_to_string(&ours).unwrap_or_default();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!("# Перенесено из {}\n", f.other.product));
            for d in &new {
                text.push_str(d);
                text.push('\n');
            }
            fs::write(&ours, text).map_err(|e| format!("Не удалось сохранить список: {e}"))?;
            *n += new.len();
        }
    }
    Ok(added)
}

// ---------------------------------------------------------------- службы

fn disabled_file(paths: &Paths) -> PathBuf {
    paths.root.join("disabled-services.json")
}

pub fn disabled(paths: &Paths) -> Vec<Disabled> {
    fs::read_to_string(disabled_file(paths))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<Disabled>>(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|d| service_exists(&d.service))
        .collect()
}

fn save_disabled(paths: &Paths, list: &[Disabled]) {
    let _ = fs::write(disabled_file(paths), serde_json::to_string_pretty(list).unwrap_or_default());
}

/// Останавливает другие программы обхода. Службы переводятся на ручной запуск, чтобы
/// не вернулись после перезагрузки; их можно вернуть кнопкой в настройках.
pub fn stop_all(paths: &Paths, own_pid: Option<u32>) -> usize {
    let found = find(paths, own_pid);
    let mut list = disabled(paths);
    let mut n = 0;
    for f in &found {
        let Some(svc) = &f.other.service else { continue };
        if f.other.running {
            let _ = hidden_command("sc").args(["stop", svc]).output();
            n += 1;
        }
        if f.other.autostart {
            let _ = hidden_command("sc").args(["config", svc, "start=", "demand"]).output();
            if !list.iter().any(|d| d.service.eq_ignore_ascii_case(svc)) {
                list.push(Disabled { service: svc.clone(), product: f.other.product.clone() });
            }
        }
    }
    save_disabled(paths, &list);
    // Всё, что осталось (запущено через .bat), завершаем.
    for (_, pid, _) in processes(own_pid) {
        let _ = hidden_command("taskkill").args(["/F", "/PID", &pid.to_string()]).output();
        n += 1;
    }
    n
}

/// Возвращает службу: снова запуск вместе с Windows, и запускает её сейчас.
pub fn restore(paths: &Paths, service: &str) -> Result<(), String> {
    let mut list = disabled(paths);
    let Some(i) = list.iter().position(|d| d.service.eq_ignore_ascii_case(service)) else {
        return Err("Служба не найдена".into());
    };
    let out = hidden_command("sc").args(["config", service, "start=", "auto"]).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("Не удалось вернуть службу {service}"));
    }
    let _ = hidden_command("sc").args(["start", service]).output();
    list.remove(i);
    save_disabled(paths, &list);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_service_commands() {
        let c = r#""C:\Program Files\zapret-discord-youtube\bin\winws.exe" --wf-tcp=80,443 --hostlist="C:\z\lists\list-general.txt""#;
        assert_eq!(exe_from_command(c).unwrap(), PathBuf::from(r"C:\Program Files\zapret-discord-youtube\bin\winws.exe"));
        assert_eq!(folder_of(&exe_from_command(c).unwrap()).unwrap(), PathBuf::from(r"C:\Program Files\zapret-discord-youtube"));
        let g = r#"C:\gdpi\x86_64\goodbyedpi.exe -9 --blacklist "C:\gdpi\russia-blacklist.txt" --blacklist C:\gdpi\my.txt"#;
        assert_eq!(exe_from_command(g).unwrap(), PathBuf::from(r"C:\gdpi\x86_64\goodbyedpi.exe"));
        assert_eq!(args_values(g, "--blacklist"), vec![r"C:\gdpi\russia-blacklist.txt", r"C:\gdpi\my.txt"]);
        let (user, _) = list_sources("GoodbyeDPI", Path::new(r"C:\gdpi"), Some(g));
        assert_eq!(user, vec![PathBuf::from(r"C:\gdpi\my.txt")]);
        assert_eq!(product_of("goodbyedpi.exe", None, None), "GoodbyeDPI");
        assert_eq!(product_of("winws.exe", None, None), "zapret");
    }

    /// Что найдено на этом компьютере: cargo test --lib live_detect -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_detect() {
        let paths = Paths::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("resources")).unwrap();
        println!("{:#?}", detect(&paths, None));
        println!("отключены: {:?}", disabled(&paths));
    }

    #[test]
    fn imports_only_new_real_domains() {
        let dir = std::env::temp_dir().join(format!("z2-others-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let theirs = dir.join("list-general-user.txt");
        let ours = dir.join("user.txt");
        fs::write(&theirs, "domain.example.abc\n# comment\nRutracker.org\nhttps://kinozal.tv/browse\nbad domain\n").unwrap();
        fs::write(&ours, "rutracker.org\n").unwrap();
        assert_eq!(new_domains(&[theirs], &ours), vec!["kinozal.tv"]);
        let _ = fs::remove_dir_all(dir);
    }
}

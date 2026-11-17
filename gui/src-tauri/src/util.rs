use std::{cmp::Ordering, collections::HashSet, os::windows::process::CommandExt, process::Command};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Команда без всплывающего консольного окна.
pub fn hidden_command(program: &str) -> Command {
    let mut c = Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Прямая работа с реестром Windows через WinAPI (без запуска reg.exe и зависимости от локали).
pub mod winreg {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
            RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY,
        },
    };

    pub const HKLM: HKEY = HKEY_LOCAL_MACHINE;
    pub const HKCU: HKEY = HKEY_CURRENT_USER;

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    pub fn get_sz(root: HKEY, subkey: &str, name: &str) -> Option<String> {
        let sk = wide(subkey);
        let vn = wide(name);
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_SUBKEY_WOW6464KEY;
        unsafe {
            let mut size: u32 = 0;
            if RegGetValueW(root, sk.as_ptr(), vn.as_ptr(), flags, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) != ERROR_SUCCESS || size < 2 {
                return None;
            }
            let mut buf = vec![0u16; (size as usize).div_ceil(2)];
            if RegGetValueW(
                root,
                sk.as_ptr(),
                vn.as_ptr(),
                flags,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                &mut size,
            ) != ERROR_SUCCESS
            {
                return None;
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..len]))
        }
    }

    pub fn get_dword(root: HKEY, subkey: &str, name: &str) -> Option<u32> {
        let sk = wide(subkey);
        let vn = wide(name);
        let mut val: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        unsafe {
            let rc = RegGetValueW(
                root,
                sk.as_ptr(),
                vn.as_ptr(),
                RRF_RT_REG_DWORD | RRF_SUBKEY_WOW6464KEY,
                std::ptr::null_mut(),
                &mut val as *mut u32 as *mut core::ffi::c_void,
                &mut size,
            );
            (rc == ERROR_SUCCESS).then_some(val)
        }
    }

    pub fn get_binary(root: HKEY, subkey: &str, name: &str) -> Option<Vec<u8>> {
        let sk = wide(subkey);
        let vn = wide(name);
        unsafe {
            let mut size: u32 = 0;
            if RegGetValueW(root, sk.as_ptr(), vn.as_ptr(), RRF_RT_REG_BINARY, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) != ERROR_SUCCESS || size == 0 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            if RegGetValueW(
                root,
                sk.as_ptr(),
                vn.as_ptr(),
                RRF_RT_REG_BINARY,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                &mut size,
            ) != ERROR_SUCCESS
            {
                return None;
            }
            buf.truncate(size as usize);
            Some(buf)
        }
    }

    pub fn set_sz(root: HKEY, subkey: &str, name: &str, data: &str) -> Result<(), String> {
        let sk = wide(subkey);
        let vn = wide(name);
        let val = wide(data);
        unsafe {
            let mut key: HKEY = std::ptr::null_mut();
            let rc = RegCreateKeyExW(
                root,
                sk.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_READ,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            );
            if rc != ERROR_SUCCESS {
                return Err(format!("RegCreateKeyExW: код {rc}"));
            }
            let bytes_len = (val.len() * 2) as u32;
            let rc2 = RegSetValueExW(key, vn.as_ptr(), 0, REG_SZ, val.as_ptr() as *const u8, bytes_len);
            RegCloseKey(key);
            if rc2 != ERROR_SUCCESS {
                return Err(format!("RegSetValueExW: код {rc2}"));
            }
            Ok(())
        }
    }

    pub fn delete_value(root: HKEY, subkey: &str, name: &str) {
        let sk = wide(subkey);
        let vn = wide(name);
        unsafe {
            let mut key: HKEY = std::ptr::null_mut();
            if RegOpenKeyExW(root, sk.as_ptr(), 0, KEY_SET_VALUE, &mut key) == ERROR_SUCCESS {
                let _ = RegDeleteValueW(key, vn.as_ptr());
                RegCloseKey(key);
            }
        }
    }
}

/// Быстрое перечисление процессов через Win32 ToolHelp API (без запуска tasklist.exe).
pub fn list_processes() -> Vec<(String, u32)> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        },
    };
    // SAFETY: стандартный снимок процессов Windows; дескриптор гарантированно закрывается ниже.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap == INVALID_HANDLE_VALUE {
            return list_processes_fallback();
        }
        let mut out = Vec::with_capacity(128);
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if !name.is_empty() {
                    out.push((name, entry.th32ProcessID));
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        out
    }
}

/// Уникальные пользовательские .exe-процессы для выбора программ в WARP.
pub fn running_app_names() -> Vec<String> {
    const SYSTEM_NOISE: &[&str] = &[
        "system", "registry", "smss.exe", "csrss.exe", "wininit.exe", "services.exe", "lsass.exe",
        "svchost.exe", "fontdrvhost.exe", "winlogon.exe", "dwm.exe", "conhost.exe", "sihost.exe",
        "taskhostw.exe", "explorer.exe", "ctfmon.exe", "dllhost.exe", "WmiPrvSE.exe", "spoolsv.exe",
        "SearchIndexer.exe", "RuntimeBroker.exe", "ApplicationFrameHost.exe", "SystemSettings.exe",
        "SecurityHealthService.exe", "MsMpEng.exe", "NisSrv.exe", "audiodg.exe", "winws2.exe",
        "winws.exe", "goodbyedpi.exe", "zapret2-gui.exe", "msedgewebview2.exe",
    ];
    let skip: HashSet<String> = SYSTEM_NOISE.iter().map(|s| s.to_lowercase()).collect();
    let mut seen = HashSet::new();
    let mut names: Vec<String> = list_processes()
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| n.to_lowercase().ends_with(".exe"))
        .filter(|n| !skip.contains(&n.to_lowercase()))
        .filter(|n| seen.insert(n.to_lowercase()))
        .collect();
    names.sort_by_key(|a| a.to_lowercase());
    names
}

fn list_processes_fallback() -> Vec<(String, u32)> {
    let Ok(out) = hidden_command("tasklist").args(["/FO", "CSV", "/NH"]).output() else {
        return vec![];
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut parts = l.split("\",\"");
            let name = parts.next()?.trim_matches('"').to_string();
            let pid: u32 = parts.next()?.trim_matches('"').parse().ok()?;
            Some((name, pid))
        })
        .collect()
}

/// Сравнение версий вида "1.0.5.1" / "v1.0.7".
pub fn version_cmp(a: &str, b: &str) -> Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split(['.', '-'])
            .map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (va, vb) = (parse(a), parse(b));
    for i in 0..va.len().max(vb.len()) {
        let (x, y) = (va.get(i).copied().unwrap_or(0), vb.get(i).copied().unwrap_or(0));
        match x.cmp(&y) {
            Ordering::Equal => continue,
            o => return o,
        }
    }
    Ordering::Equal
}

/// Разбивает строку параметров на аргументы, учитывая кавычки.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for ch in s.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
                has = true;
            }
            None if ch.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            None => cur.push(ch),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Командная строка для показа пользователю.
pub fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.is_empty() || a.contains(' ') || a.contains('"') {
                format!("\"{}\"", a.replace('"', "\\\""))
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Конвертирует интернационализированное доменное имя (например, «сайт.рф») в ASCII Punycode через WinAPI.
fn idn_to_ascii(domain: &str) -> Option<String> {
    use windows_sys::Win32::Globalization::IdnToAscii;
    let wide: Vec<u16> = domain.encode_utf16().collect();
    if wide.is_empty() {
        return None;
    }
    unsafe {
        let needed = IdnToAscii(0, wide.as_ptr(), wide.len() as i32, std::ptr::null_mut(), 0);
        if needed <= 0 {
            return None;
        }
        let mut out = vec![0u16; needed as usize];
        let written = IdnToAscii(0, wide.as_ptr(), wide.len() as i32, out.as_mut_ptr(), out.len() as i32);
        if written <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&out[..written as usize]).to_lowercase())
    }
}

/// Превращает ввод пользователя («https://www.site.com/page?x», «сайт.рф», «Site.com:443») в домен для хостлиста.
pub fn normalize_domain(input: &str) -> Result<String, String> {
    let mut s = input.trim().to_lowercase();
    for scheme in ["https://", "http://"] {
        if let Some(rest) = s.strip_prefix(scheme) {
            s = rest.to_string();
        }
    }
    if let Some(i) = s.find(['/', '?', '#']) {
        s.truncate(i);
    }
    if let Some(i) = s.rfind(':') {
        s.truncate(i);
    }
    let mut s = s.trim_start_matches("*.").trim_start_matches("www.").trim_end_matches('.').to_string();
    if s.is_empty() {
        return Err("Введите адрес сайта".into());
    }
    if !s.is_ascii() {
        s = idn_to_ascii(&s).ok_or_else(|| format!("«{}» не похоже на адрес сайта", input.trim()))?;
    }
    let valid = s.contains('.')
        && !s.starts_with('.')
        && !s.contains("..")
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        && s.split('.').all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-'));
    if !valid {
        return Err(format!("«{}» не похоже на адрес сайта", input.trim()));
    }
    Ok(s)
}

/// Нормализует имя исполняемого файла («C:\Games\App.exe» или «app» → «app.exe»).
pub fn normalize_exe_name(input: &str) -> Result<String, String> {
    let raw = input.trim().trim_matches('"');
    let base = raw.rsplit(['\\', '/']).next().unwrap_or(raw).trim();
    if base.is_empty() {
        return Err("Введите имя программы (.exe)".into());
    }
    let name = if base.to_lowercase().ends_with(".exe") {
        base.to_string()
    } else {
        format!("{base}.exe")
    };
    let valid = name.len() > 4
        && !name.contains(['\\', '/', ':', '*', '?', '"', '<', '>'])
        && !name.starts_with('.');
    if !valid {
        return Err(format!("«{}» не похоже на имя программы", input.trim()));
    }
    Ok(name)
}

pub fn normalize_exe_vec(items: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw in items {
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let exe = normalize_exe_name(t)?;
            if seen.insert(exe.to_lowercase()) {
                out.push(exe);
            }
        }
    }
    Ok(out)
}

/// Нормализует многострочный список доменов: сохраняет комментарии (#) и спецвыражения (!, ^),
/// приводит ссылки и домены .рф к чистым доменам и убирает повторы.
pub fn normalize_list_text(content: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut seen = HashSet::new();
    for raw in content.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            if !out.is_empty() && !out.ends_with("\n\n") {
                out.push('\n');
            }
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with('!') || trimmed.starts_with('^') {
            out.push_str(trimmed);
            out.push('\n');
            continue;
        }
        let domain = normalize_domain(trimmed)?;
        if seen.insert(domain.clone()) {
            out.push_str(&domain);
            out.push('\n');
        }
    }
    Ok(out)
}

/// Нормализует список доменов WARP (пропускает пустые строки и комментарии, убирает дубликаты).
pub fn normalize_domain_vec(items: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw in items {
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let d = normalize_domain(t)?;
            if seen.insert(d.clone()) {
                out.push(d);
            }
        }
    }
    Ok(out)
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(version_cmp("1.0.7", "1.0.6").is_gt());
        assert!(version_cmp("v1.0.5.1", "1.0.5").is_gt());
        assert!(version_cmp("1.0.7", "v1.0.7").is_eq());
        assert!(version_cmp("1.0.10", "1.0.9").is_gt());
    }

    #[test]
    fn domains_and_punycode() {
        assert_eq!(normalize_domain("https://www.Example.com/watch?v=1").unwrap(), "example.com");
        assert_eq!(normalize_domain("  rutracker.org:443 ").unwrap(), "rutracker.org");
        assert_eq!(normalize_domain("*.cdn.site.net").unwrap(), "cdn.site.net");
        assert_eq!(normalize_domain("sub.domain.co.uk.").unwrap(), "sub.domain.co.uk");
        assert_eq!(normalize_domain("https://сайт.рф/страница").unwrap(), "xn--80aswg.xn--p1ai");
        assert!(normalize_domain("").is_err());
        assert!(normalize_domain("localhost").is_err());
        assert!(normalize_domain("bad..com").is_err());
        assert!(normalize_domain("-x.com").is_err());
    }

    #[test]
    fn exe_names() {
        assert_eq!(normalize_exe_name(r#" "C:\Program Files\App\RobloxPlayerBeta.exe" "#).unwrap(), "RobloxPlayerBeta.exe");
        assert_eq!(normalize_exe_name("spotify").unwrap(), "spotify.exe");
        assert!(normalize_exe_name("").is_err());
    }

    #[test]
    fn winreg_reads_windows_build() {
        let build = winreg::get_sz(winreg::HKLM, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber");
        assert!(build.and_then(|b| b.parse::<u32>().ok()).unwrap_or(0) >= 10240);
    }

    #[test]
    fn list_normalization() {
        let src = "# Мои сайты\r\nhttps://www.Example.com/page\r\nEXAMPLE.COM\r\nrutracker.org:443\r\nсайт.рф\r\n^special$\r\n";
        let norm = normalize_list_text(src).unwrap();
        assert_eq!(norm, "# Мои сайты\nexample.com\nrutracker.org\nxn--80aswg.xn--p1ai\n^special$\n");
        assert!(normalize_list_text("not a domain").is_err());

        let warp = normalize_domain_vec(&["# comment".into(), "https://www.Site.org/a".into(), "site.org".into()]).unwrap();
        assert_eq!(warp, vec!["site.org"]);
    }

    #[test]
    fn process_snapshot_works() {
        let procs = list_processes();
        assert!(!procs.is_empty());
        assert!(procs.iter().any(|(_, pid)| *pid == std::process::id()));
    }

    #[test]
    fn args() {
        assert_eq!(split_args("--a=1  --b=\"x y\" c"), vec!["--a=1", "--b=x y", "c"]);
        assert!(split_args("   ").is_empty());
    }
}

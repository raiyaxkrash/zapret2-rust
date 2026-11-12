use std::{cmp::Ordering, os::windows::process::CommandExt, process::Command};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Команда без всплывающего консольного окна.
pub fn hidden_command(program: &str) -> Command {
    let mut c = Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Сравнение версий вида "1.0.5.1" / "v1.0.7".
pub fn version_cmp(a: &str, b: &str) -> Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split(|c: char| c == '.' || c == '-')
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

/// Превращает ввод пользователя («https://www.site.com/page?x», «Site.com:443») в домен для хостлиста.
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
    let s = s.trim_start_matches("*.").trim_start_matches("www.").trim_end_matches('.').to_string();
    if s.is_empty() {
        return Err("Введите адрес сайта".into());
    }
    if !s.is_ascii() {
        return Err("Введите домен латиницей. Для доменов .рф используйте punycode (xn--…)".into());
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
    fn domains() {
        assert_eq!(normalize_domain("https://www.Example.com/watch?v=1").unwrap(), "example.com");
        assert_eq!(normalize_domain("  rutracker.org:443 ").unwrap(), "rutracker.org");
        assert_eq!(normalize_domain("*.cdn.site.net").unwrap(), "cdn.site.net");
        assert_eq!(normalize_domain("sub.domain.co.uk.").unwrap(), "sub.domain.co.uk");
        assert!(normalize_domain("").is_err());
        assert!(normalize_domain("localhost").is_err());
        assert!(normalize_domain("bad..com").is_err());
        assert!(normalize_domain("-x.com").is_err());
        assert!(normalize_domain("сайт.рф").is_err());
    }

    #[test]
    fn args() {
        assert_eq!(split_args("--a=1  --b=\"x y\" c"), vec!["--a=1", "--b=x y", "c"]);
        assert!(split_args("   ").is_empty());
    }
}

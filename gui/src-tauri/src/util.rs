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
    fn args() {
        assert_eq!(split_args("--a=1  --b=\"x y\" c"), vec!["--a=1", "--b=x y", "c"]);
        assert!(split_args("   ").is_empty());
    }
}

//! Расположение рабочих файлов и синхронизация с ресурсами из установщика.
//!
//! Установщик кладёт zapret2 (winws2, Lua, фейки) и списки в каталог ресурсов
//! (Program Files, только чтение). При запуске всё нужное копируется в
//! %LOCALAPPDATA%\Zapret2GUI, где файлы можно обновлять и редактировать.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::util::{hidden_command, version_cmp};

pub const BUILTIN_LISTS: &[&str] = &["youtube", "discord"];
pub const USER_LISTS: &[&str] = &["user", "exclude"];

#[derive(Clone, Debug)]
pub struct Paths {
    /// %LOCALAPPDATA%\Zapret2GUI — рабочий каталог winws2 (все пути в аргументах относительные).
    pub root: PathBuf,
    pub engine: PathBuf,
    pub lists: PathBuf,
    /// Каталог в LocalLow: winws2 после старта понижает себе уровень целостности до low
    /// и писать (автосписок) может только сюда.
    pub low: PathBuf,
    pub resources: PathBuf,
    /// Стратегии и встроенные списки, загруженные из репозитория на GitHub.
    pub remote: PathBuf,
}

impl Paths {
    pub fn new(resources: PathBuf) -> Result<Self, String> {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("Не найдена переменная LOCALAPPDATA")?;
        let root = local.join("Zapret2GUI");
        let low = local
            .parent()
            .map(|p| p.join("LocalLow").join("Zapret2GUI"))
            .unwrap_or_else(|| root.join("low"));
        let p = Paths {
            engine: root.join("engine"),
            lists: root.join("lists"),
            remote: root.join("remote"),
            root,
            low,
            resources,
        };
        for d in [&p.root, &p.engine, &p.lists, &p.low] {
            fs::create_dir_all(d).map_err(|e| format!("Не удалось создать {}: {e}", d.display()))?;
        }
        Ok(p)
    }

    pub fn winws(&self) -> PathBuf {
        self.engine.join("bin").join("winws2.exe")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// Откуда брать стратегии и встроенные списки: из загруженной с GitHub копии,
    /// если она новее встроенной в установщик, иначе из ресурсов приложения.
    pub fn content_dir(&self) -> PathBuf {
        let v = |d: &Path| {
            fs::read_to_string(d.join("presets.json"))
                .map(|t| crate::content::version_of(&t))
                .unwrap_or(0)
        };
        if v(&self.remote) > v(&self.resources) {
            self.remote.clone()
        } else {
            self.resources.clone()
        }
    }

    pub fn content_version(&self) -> u64 {
        fs::read_to_string(self.presets_file())
            .map(|t| crate::content::version_of(&t))
            .unwrap_or(0)
    }

    pub fn presets_file(&self) -> PathBuf {
        self.content_dir().join("presets.json")
    }

    /// Обновляет встроенные списки (YouTube, Discord) в рабочем каталоге.
    pub fn sync_builtin_lists(&self) -> Result<(), String> {
        let src_dir = self.content_dir().join("lists");
        for name in BUILTIN_LISTS {
            let src = src_dir.join(format!("{name}.txt"));
            if src.exists() {
                fs::copy(&src, self.lists.join(format!("{name}.txt")))
                    .map_err(|e| format!("Не удалось скопировать список {name}: {e}"))?;
            }
        }
        Ok(())
    }

    pub fn auto_list(&self) -> PathBuf {
        self.low.join("autohostlist.txt")
    }

    pub fn list_file(&self, name: &str) -> Option<PathBuf> {
        if name == "auto" {
            return Some(self.auto_list());
        }
        if BUILTIN_LISTS.contains(&name) || USER_LISTS.contains(&name) {
            return Some(self.lists.join(format!("{name}.txt")));
        }
        None
    }

    pub fn engine_version(&self) -> String {
        read_version(&self.engine.join("version.txt"))
    }

    pub fn bundled_engine_version(&self) -> String {
        read_version(&self.resources.join("engine").join("version.txt"))
    }

    /// Копирует ресурсы из установщика в рабочий каталог.
    pub fn sync_resources(&self) -> Result<(), String> {
        let bundled = self.resources.join("engine");
        let installed_ver = self.engine_version();
        let bundled_ver = self.bundled_engine_version();
        let missing = !self.winws().exists() || installed_ver.is_empty();
        if bundled.exists() && (missing || version_cmp(&bundled_ver, &installed_ver).is_gt()) {
            release_driver();
            copy_dir(&bundled, &self.engine)?;
        }

        self.sync_builtin_lists()?;
        let res_lists = self.resources.join("lists");
        for name in USER_LISTS {
            let dst = self.lists.join(format!("{name}.txt"));
            let src = res_lists.join(format!("{name}.txt"));
            if !dst.exists() && src.exists() {
                let _ = fs::copy(&src, &dst);
            }
        }
        if !self.auto_list().exists() {
            let _ = fs::write(self.auto_list(), "");
        }
        Ok(())
    }
}

fn read_version(p: &Path) -> String {
    fs::read_to_string(p)
        .map(|s| s.trim().trim_start_matches('v').to_string())
        .unwrap_or_default()
}

pub fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| format!("Не удалось скопировать {}: {e}", to.display()))?;
        }
    }
    Ok(())
}

/// Выгружает драйвер WinDivert, чтобы его файл можно было заменить.
pub fn release_driver() {
    for svc in ["WinDivert", "WinDivert14"] {
        let _ = hidden_command("sc").args(["stop", svc]).output();
    }
}

/// Количество доменов (непустых строк без комментариев) в списке.
pub fn count_domains(p: &Path) -> usize {
    fs::read_to_string(p).map(|s| count_domains_text(&s)).unwrap_or(0)
}

pub fn count_domains_text(s: &str) -> usize {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .count()
}

pub fn first_domains(p: &Path, n: usize) -> Vec<String> {
    fs::read_to_string(p)
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!') && !l.starts_with('^'))
                .take(n)
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default()
}

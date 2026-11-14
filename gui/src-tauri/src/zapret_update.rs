//! Загрузка и обновление самого zapret2 (winws2, Lua-скрипты, фейки) из релизов bol-van/zapret2.
//! В установщик zapret2 не входит: при первом запуске приложение скачивает его само.

use std::{
    collections::HashMap,
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    paths::{copy_dir, release_driver, Paths},
    util::version_cmp,
};

const API_LATEST: &str = "https://api.github.com/repos/bol-van/zapret2/releases/latest";

/// Запасной вариант, если GitHub API недоступен (лимит запросов, сеть): проверенная версия
/// с известной контрольной суммой архива.
const PINNED_VERSION: &str = "1.0.7";
const PINNED_ZIP: &str = "https://github.com/bol-van/zapret2/releases/download/v1.0.7/zapret2-v1.0.7.zip";
const PINNED_SHA256: &str = "35e14e4e1f345b870c3b87269fa45353efd597241a25304d38375c46b9fd2967";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    published_at: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// SHA-256 файла от GitHub: "sha256:…".
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct EngineUpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub url: String,
    pub notes: String,
    pub published_at: String,
    #[serde(skip)]
    zip_url: String,
    #[serde(skip)]
    sums_url: Option<String>,
    /// Известная заранее контрольная сумма архива (для закреплённой версии).
    #[serde(skip)]
    sha256: Option<String>,
}

/// Закреплённая версия zapret2: используется, если узнать последнюю не удалось.
pub fn pinned(paths: &Paths) -> EngineUpdateInfo {
    EngineUpdateInfo {
        current: paths.engine_version(),
        latest: PINNED_VERSION.into(),
        available: true,
        url: "https://github.com/bol-van/zapret2/releases/tag/v1.0.7".into(),
        notes: String::new(),
        published_at: String::new(),
        zip_url: PINNED_ZIP.into(),
        sums_url: None,
        sha256: Some(PINNED_SHA256.into()),
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Zapret2-GUI-updater")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())
}

pub async fn check(paths: &Paths) -> Result<EngineUpdateInfo, String> {
    let rel: Release = client()?
        .get(API_LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("GitHub недоступен: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Некорректный ответ GitHub: {e}"))?;

    let latest = rel.tag_name.trim_start_matches('v').to_string();
    let zip = rel
        .assets
        .iter()
        .find(|a| a.name.ends_with(".zip") && a.name.starts_with("zapret2-v") && !a.name.contains("openwrt"))
        .ok_or("В релизе нет zip-архива")?;
    let sums = rel.assets.iter().find(|a| a.name == "sha256sum.txt");
    let current = paths.engine_version();
    Ok(EngineUpdateInfo {
        available: version_cmp(&latest, &current).is_gt(),
        current,
        latest,
        url: rel.html_url,
        notes: rel.body.chars().take(4000).collect(),
        published_at: rel.published_at,
        zip_url: zip.browser_download_url.clone(),
        sums_url: sums.map(|a| a.browser_download_url.clone()),
        sha256: zip.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
    })
}

/// «<sha256>  <путь>» → путь → sha256 (путь с прямыми слэшами, без «*» и «./»).
fn parse_sums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let hash = it.next()?;
            let path = it.next()?.trim_start_matches('*').trim_start_matches("./").replace('\\', "/");
            (hash.len() == 64).then(|| (path, hash.to_lowercase()))
        })
        .collect()
}

/// Куда положить файл из архива (путь внутри архива без корневой папки).
fn target_for(rel: &Path) -> Option<PathBuf> {
    let s = rel.to_string_lossy().replace('\\', "/");
    let file = rel.file_name()?.to_string_lossy().to_string();
    let map = [
        ("binaries/windows-x86_64/", "bin"),
        ("lua/", "lua"),
        ("files/fake/", "fake"),
        ("init.d/windivert.filter.examples/", "filters"),
    ];
    for (prefix, dir) in map {
        if let Some(rest) = s.strip_prefix(prefix) {
            if !rest.contains('/') && !rest.is_empty() {
                return Some(PathBuf::from(dir).join(file));
            }
        }
    }
    (s == "docs/LICENSE.txt").then(|| PathBuf::from("LICENSE.txt"))
}

/// Скачивает и распаковывает новую версию в engine.new. Возвращает путь к ней.
/// `progress(загружено, всего)` вызывается по мере загрузки.
pub async fn download(paths: &Paths, info: &EngineUpdateInfo, progress: impl Fn(u64, Option<u64>)) -> Result<PathBuf, String> {
    let c = client()?;
    // Несколько попыток: загрузка с GitHub иногда обрывается на установке соединения.
    let mut attempt = 0;
    let mut resp = loop {
        attempt += 1;
        match c.get(&info.zip_url).send().await.and_then(|r| r.error_for_status()) {
            Ok(r) => break r,
            Err(e) if attempt >= 3 => return Err(format!("Ошибка загрузки: {e}")),
            Err(_) => tokio::time::sleep(Duration::from_secs(2 * attempt)).await,
        }
    };
    let total = resp.content_length();
    let mut data: Vec<u8> = Vec::with_capacity(total.unwrap_or(10 << 20) as usize);
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("Ошибка загрузки: {e}"))? {
        data.extend_from_slice(&chunk);
        progress(data.len() as u64, total);
    }

    let digest = hex::encode(Sha256::digest(&data));
    if let Some(expected) = &info.sha256 {
        if !expected.eq_ignore_ascii_case(&digest) {
            return Err("Контрольная сумма архива не совпадает, загрузка отменена".into());
        }
    }
    // sha256sum.txt в релизе содержит суммы отдельных файлов внутри архива
    // («<sha256>  zapret2-vX.Y.Z/binaries/windows-x86_64/winws2.exe»): ими проверяется
    // каждый распакованный файл.
    let mut file_sums: HashMap<String, String> = HashMap::new();
    if let Some(url) = &info.sums_url {
        let sums = c
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("Не удалось получить контрольные суммы: {e}"))?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        file_sums = parse_sums(&sums);
    }
    if info.sha256.is_none() && file_sums.is_empty() {
        return Err("Нет контрольных сумм для проверки zapret2, загрузка отменена".into());
    }

    let staging = paths.root.join("engine.new");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let mut archive = zip::ZipArchive::new(Cursor::new(data.as_slice())).map_err(|e| format!("Повреждённый архив: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let Some(name) = entry.enclosed_name() else { continue };
        // Отбрасываем корневую папку zapret2-vX.Y.Z/
        let rel: PathBuf = name.components().skip(1).collect();
        let Some(target) = target_for(&rel) else { continue };
        let dst = staging.join(target);
        if let Some(p) = dst.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        if !file_sums.is_empty() {
            let key = name.to_string_lossy().replace('\\', "/");
            match file_sums.get(&key) {
                Some(expected) if expected.eq_ignore_ascii_case(&hex::encode(Sha256::digest(&buf))) => {}
                Some(_) => {
                    let _ = fs::remove_dir_all(&staging);
                    return Err(format!("Контрольная сумма {key} не совпадает, загрузка отменена"));
                }
                // Исполняемые файлы и драйвер обязаны быть в списке сумм.
                None if key.ends_with(".exe") || key.ends_with(".sys") || key.ends_with(".dll") => {
                    let _ = fs::remove_dir_all(&staging);
                    return Err(format!("Для {key} нет контрольной суммы, загрузка отменена"));
                }
                None => {}
            }
        }
        fs::write(&dst, buf).map_err(|e| e.to_string())?;
    }
    if !staging.join("bin").join("winws2.exe").exists() || !staging.join("lua").join("zapret-lib.lua").exists() {
        let _ = fs::remove_dir_all(&staging);
        return Err("В архиве не найдены winws2.exe или Lua-скрипты".into());
    }
    fs::write(staging.join("version.txt"), format!("{}\n", info.latest)).map_err(|e| e.to_string())?;
    Ok(staging)
}

/// Заменяет текущий движок новым. winws2 к этому моменту должен быть остановлен.
pub fn install(paths: &Paths, staging: &Path) -> Result<(), String> {
    release_driver();
    let old = paths.root.join("engine.old");
    let _ = fs::remove_dir_all(&old);
    if fs::rename(&paths.engine, &old).is_ok() {
        if let Err(e) = fs::rename(staging, &paths.engine) {
            let _ = fs::rename(&old, &paths.engine);
            return Err(format!("Не удалось установить обновление: {e}"));
        }
        let _ = fs::remove_dir_all(&old);
    } else {
        // Каталог занят (например, загруженным драйвером) — копируем поверх.
        copy_dir(staging, &paths.engine)?;
        let _ = fs::remove_dir_all(staging);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Живая проверка: cargo test --lib live_download -- --ignored --nocapture
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_download() {
        let root = std::env::temp_dir().join(format!("z2-dl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let paths = Paths {
            engine: root.join("engine"),
            lists: root.join("lists"),
            low: root.join("low"),
            resources: root.join("res"),
            remote: root.join("remote"),
            root: root.clone(),
        };
        for info in [check(&paths).await.unwrap(), pinned(&paths)] {
            let staging = download(&paths, &info, |_, _| {}).await.unwrap();
            for f in ["bin/winws2.exe", "bin/WinDivert64.sys", "bin/cygwin1.dll", "lua/zapret-lib.lua", "lua/zapret-antidpi.lua", "fake/tls_clienthello_www_google_com.bin", "filters/windivert_part.discord_media.txt", "LICENSE.txt", "version.txt"] {
                assert!(staging.join(f).exists(), "{} нет {f}", info.latest);
            }
            install(&paths, &staging).unwrap();
            assert!(paths.winws().exists());
            println!("zapret2 {} скачан и установлен", info.latest);
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sums() {
        let m = parse_sums("d7b7c17965b63647d89974686696d073b6776196c79fc510568339197f62470c  zapret2-v1.0.7/binaries/windows-x86_64/winws2.exe
bad line
");
        assert_eq!(m.get("zapret2-v1.0.7/binaries/windows-x86_64/winws2.exe").map(|s| &s[..6]), Some("d7b7c1"));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn mapping() {
        assert_eq!(target_for(Path::new("binaries/windows-x86_64/winws2.exe")), Some(PathBuf::from("bin/winws2.exe")));
        assert_eq!(target_for(Path::new("lua/zapret-lib.lua")), Some(PathBuf::from("lua/zapret-lib.lua")));
        assert_eq!(target_for(Path::new("binaries/linux-x86_64/nfqws2")), None);
        assert_eq!(target_for(Path::new("files/fake/sub/x.bin")), None);
    }
}

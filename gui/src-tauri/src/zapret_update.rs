//! Обновление самого zapret2 (winws2, Lua-скрипты, фейки) из релизов bol-van/zapret2.

use std::{
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
    zip_name: String,
    #[serde(skip)]
    sums_url: Option<String>,
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
        zip_name: zip.name.clone(),
        sums_url: sums.map(|a| a.browser_download_url.clone()),
    })
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
pub async fn download(paths: &Paths, info: &EngineUpdateInfo) -> Result<PathBuf, String> {
    let c = client()?;
    let data = c
        .get(&info.zip_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Ошибка загрузки: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Ошибка загрузки: {e}"))?;

    let digest = hex::encode(Sha256::digest(&data));
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
        let expected = sums
            .lines()
            .find(|l| l.split_whitespace().last().map(|n| n.trim_start_matches('*')) == Some(info.zip_name.as_str()))
            .and_then(|l| l.split_whitespace().next())
            .ok_or("В sha256sum.txt нет записи для архива")?;
        if !expected.eq_ignore_ascii_case(&digest) {
            return Err("Контрольная сумма архива не совпадает, обновление отменено".into());
        }
    }

    let staging = paths.root.join("engine.new");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let mut archive = zip::ZipArchive::new(Cursor::new(data.as_ref())).map_err(|e| format!("Повреждённый архив: {e}"))?;
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

    #[test]
    fn mapping() {
        assert_eq!(target_for(Path::new("binaries/windows-x86_64/winws2.exe")), Some(PathBuf::from("bin/winws2.exe")));
        assert_eq!(target_for(Path::new("lua/zapret-lib.lua")), Some(PathBuf::from("lua/zapret-lib.lua")));
        assert_eq!(target_for(Path::new("binaries/linux-x86_64/nfqws2")), None);
        assert_eq!(target_for(Path::new("files/fake/sub/x.bin")), None);
    }
}

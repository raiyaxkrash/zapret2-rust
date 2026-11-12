//! Обновление стратегий (presets.json) и встроенных списков сайтов прямо из репозитория,
//! без выпуска новой версии приложения. Чтобы установленные копии получили изменения,
//! достаточно поднять поле "version" в gui/resources/presets.json и отправить коммит в main.

use std::{fs, time::Duration};

use serde::Serialize;

use crate::{
    paths::{Paths, BUILTIN_LISTS},
    presets::Presets,
};

const RAW_BASE: &str = "https://raw.githubusercontent.com/raiyaxkrash/zapret2-rust/main/gui/resources/";

#[derive(Serialize, Clone, Debug)]
pub struct ContentInfo {
    pub current: u64,
    pub latest: u64,
    pub available: bool,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Zapret2-GUI-content")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())
}

async fn fetch(c: &reqwest::Client, rel: &str) -> Result<String, String> {
    c.get(format!("{RAW_BASE}{rel}"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Не удалось загрузить {rel}: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Не удалось загрузить {rel}: {e}"))
}

pub fn version_of(text: &str) -> u64 {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v.get("version").and_then(|x| x.as_u64()))
        .unwrap_or(0)
}

pub async fn check(paths: &Paths) -> Result<ContentInfo, String> {
    let text = fetch(&client()?, "presets.json").await?;
    let latest = version_of(&text);
    let current = paths.content_version();
    Ok(ContentInfo { current, latest, available: latest > current })
}

/// Скачивает новые стратегии и списки. Возвращает новую версию или None, если обновлений нет.
pub async fn update(paths: &Paths) -> Result<Option<u64>, String> {
    let c = client()?;
    let presets_text = fetch(&c, "presets.json").await?;
    let latest = version_of(&presets_text);
    if latest <= paths.content_version() {
        return Ok(None);
    }
    let presets: Presets =
        serde_json::from_str(&presets_text).map_err(|e| format!("Ошибка в загруженном presets.json: {e}"))?;

    // Встроенные списки и наборы сайтов: относительный путь внутри lists/ и текст.
    let mut lists: Vec<(String, String)> = Vec::new();
    for name in BUILTIN_LISTS {
        let rel = format!("{name}.txt");
        let text = fetch(&c, &format!("lists/{rel}")).await?;
        if crate::paths::count_domains_text(&text) == 0 {
            return Err(format!("Загруженный список {name} пуст"));
        }
        lists.push((rel, text));
    }
    for pack in presets.packs.iter().filter(|p| crate::presets::valid_pack_id(&p.id)) {
        let rel = format!("packs/{}.txt", pack.id);
        let text = fetch(&c, &format!("lists/{rel}")).await?;
        lists.push((rel, text));
    }

    let staging = paths.root.join("remote.new");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(staging.join("lists").join("packs")).map_err(|e| e.to_string())?;
    fs::write(staging.join("presets.json"), &presets_text).map_err(|e| e.to_string())?;
    for (rel, text) in &lists {
        fs::write(staging.join("lists").join(rel), text).map_err(|e| e.to_string())?;
    }
    let _ = fs::remove_dir_all(&paths.remote);
    fs::rename(&staging, &paths.remote).map_err(|e| format!("Не удалось сохранить обновление: {e}"))?;
    paths.sync_builtin_lists()?;
    Ok(Some(latest))
}

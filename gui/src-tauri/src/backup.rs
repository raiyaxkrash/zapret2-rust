//! Экспорт и импорт резервной копии настроек и пользовательских списков.

use std::fs;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{paths::Paths, settings::{self, Settings}, tgproxy, util, AppState};

#[derive(Serialize, Deserialize)]
pub struct BackupBundle {
    pub version: u32,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub exported_at: u64,
    pub settings: Settings,
    #[serde(default)]
    pub user_list: String,
    #[serde(default)]
    pub exclude_list: String,
}

pub fn build_bundle(paths: &Paths, settings: Settings, app_version: String) -> BackupBundle {
    let user_list = paths.list_file("user").and_then(|p| fs::read_to_string(p).ok()).unwrap_or_default();
    let exclude_list = paths.list_file("exclude").and_then(|p| fs::read_to_string(p).ok()).unwrap_or_default();
    BackupBundle {
        version: 1,
        app_version,
        exported_at: util::now_unix(),
        settings,
        user_list,
        exclude_list,
    }
}

pub fn export(app: &AppHandle, state: &AppState) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    let bundle = build_bundle(&state.paths, settings, app.package_info().version.to_string());
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    let downloads = std::env::var_os("USERPROFILE")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("Downloads"))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| state.paths.root.clone());
    let filename = format!("zapret2-backup-{}.json", util::now_unix());
    let dst = downloads.join(&filename);
    fs::write(&dst, json).map_err(|e| format!("Не удалось сохранить резервную копию: {e}"))?;
    let _ = util::hidden_command("explorer").arg(format!("/select,{}", dst.display())).spawn();
    Ok(format!("Резервная копия сохранена: {filename}"))
}

pub fn parse_and_write_lists(app: &AppHandle, json: &str) -> Result<Settings, String> {
    let bundle: BackupBundle = serde_json::from_str(json)
        .map_err(|_| "Файл резервной копии повреждён или не подходит".to_string())?;
    let state = app.state::<AppState>();
    let user_norm = util::normalize_list_text(&bundle.user_list)?;
    let excl_norm = util::normalize_list_text(&bundle.exclude_list)?;
    if let Some(p) = state.paths.list_file("user") {
        fs::write(p, user_norm).map_err(|e| format!("Не удалось сохранить список: {e}"))?;
    }
    if let Some(p) = state.paths.list_file("exclude") {
        fs::write(p, excl_norm).map_err(|e| format!("Не удалось сохранить список: {e}"))?;
    }
    let mut next = bundle.settings;
    let current_secret = state.settings.lock().unwrap().tg_proxy_secret.clone();
    if !tgproxy::valid_secret(&next.tg_proxy_secret) {
        next.tg_proxy_secret = current_secret;
    }
    if next.app_update_url.trim().is_empty() {
        next.app_update_url = settings::DEFAULT_APP_UPDATE_URL.into();
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_roundtrip_preserves_settings_and_lists() {
        let s = Settings {
            doh_provider: "quad9".into(),
            custom_strategies: vec![settings::CustomStrategy {
                id: "user_home".into(),
                name: "Дома".into(),
                tls: "--payload=tls_client_hello".into(),
                http: "--payload=http_req".into(),
                quic: String::new(),
            }],
            ..Default::default()
        };
        let bundle = BackupBundle {
            version: 1,
            app_version: "2.7.0".into(),
            exported_at: 1_700_000_000,
            settings: s,
            user_list: "example.com\n".into(),
            exclude_list: "local.test\n".into(),
        };
        let raw = serde_json::to_string(&bundle).unwrap();
        let parsed: BackupBundle = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.settings.doh_provider, "quad9");
        assert_eq!(parsed.settings.custom_strategies.len(), 1);
        assert_eq!(parsed.settings.custom_strategies[0].name, "Дома");
        assert_eq!(parsed.user_list, "example.com\n");
    }
}

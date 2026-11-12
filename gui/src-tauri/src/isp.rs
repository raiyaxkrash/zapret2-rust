//! Определение провайдера (автономной системы) и рекомендация стратегии для него.
//!
//! Провайдер определяется через открытый сервис RIPEstat: внешний IP → номер AS → название.
//! Рекомендации лежат в presets.json (поле "isp") и пополняются по результатам,
//! которыми пользователи делятся кнопкой «Поделиться результатом».

use std::time::Duration;

use serde::{Deserialize, Serialize};

const RIPESTAT: &str = "https://stat.ripe.net/data";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Isp {
    pub asn: u32,
    /// Название из реестра без служебного префикса (например, «PJSC Rostelecom»).
    pub name: String,
}

/// Рекомендация из presets.json.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IspHint {
    pub asn: Vec<u32>,
    /// Понятное название провайдера («Ростелеком»).
    pub name: String,
    pub strategy: String,
    #[serde(default)]
    pub youtube_strategy: String,
}

/// Убирает из названия AS служебный идентификатор («ROSTELECOM-AS PJSC Rostelecom» → «PJSC Rostelecom»).
pub fn clean_holder(holder: &str) -> String {
    let holder = holder.trim();
    match holder.split_once(' ') {
        Some((first, rest))
            if !rest.trim().is_empty()
                && first.len() > 2
                && first.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '_') =>
        {
            rest.trim().to_string()
        }
        _ => holder.to_string(),
    }
}

pub fn find_hint(hints: &[IspHint], asn: u32) -> Option<&IspHint> {
    hints.iter().find(|h| h.asn.contains(&asn))
}

async fn get_json(c: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    c.get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())
}

pub async fn detect() -> Result<Isp, String> {
    let c = reqwest::Client::builder()
        .user_agent("zapret2-gui")
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let fail = |what: &str| format!("Не удалось определить провайдера ({what})");
    let me = get_json(&c, &format!("{RIPESTAT}/whats-my-ip/data.json?sourceapp=zapret2-gui")).await.map_err(|_| fail("адрес"))?;
    let ip = me["data"]["ip"].as_str().ok_or_else(|| fail("адрес"))?.to_string();
    let net = get_json(&c, &format!("{RIPESTAT}/network-info/data.json?resource={ip}&sourceapp=zapret2-gui"))
        .await
        .map_err(|_| fail("сеть"))?;
    let asn: u32 = net["data"]["asns"][0]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| net["data"]["asns"][0].as_u64().map(|n| n as u32))
        .ok_or_else(|| fail("сеть"))?;
    let name = match get_json(&c, &format!("{RIPESTAT}/as-overview/data.json?resource=AS{asn}&sourceapp=zapret2-gui")).await {
        Ok(v) => clean_holder(v["data"]["holder"].as_str().unwrap_or_default()),
        Err(_) => String::new(),
    };
    Ok(Isp { asn, name })
}

/// Ссылка на новый issue с результатом подбора: из них пополняется таблица рекомендаций.
/// Открывает шаблон .github/ISSUE_TEMPLATE/isp-strategy.yml и заполняет его поля
/// (обычные issue без шаблона в репозитории отключены).
pub fn share_url(isp: &Isp, strategy: &str, youtube_strategy: &str, app_version: &str) -> String {
    let title = format!("Стратегия для AS{} {}", isp.asn, isp.name);
    let yt = if youtube_strategy.is_empty() { "как основная" } else { youtube_strategy };
    let fields = [
        ("template", "isp-strategy.yml".to_string()),
        ("title", title),
        ("provider", isp.name.clone()),
        ("asn", format!("AS{}", isp.asn)),
        ("strategy", strategy.to_string()),
        ("youtube_strategy", yt.to_string()),
        ("version", app_version.to_string()),
    ];
    let query: Vec<String> = fields.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect();
    format!("https://github.com/raiyaxkrash/zapret2-rust/issues/new?{}", query.join("&"))
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holder_names() {
        assert_eq!(clean_holder("ROSTELECOM-AS PJSC Rostelecom"), "PJSC Rostelecom");
        assert_eq!(clean_holder("MTS PJSC MTS"), "PJSC MTS");
        assert_eq!(clean_holder("Interhive-AS Interhive OU"), "Interhive-AS Interhive OU");
        assert_eq!(clean_holder("CLOUDFLARENET"), "CLOUDFLARENET");
    }

    #[test]
    fn hints_and_share() {
        let hints = vec![IspHint { asn: vec![12389, 42610], name: "Ростелеком".into(), strategy: "seqovl".into(), youtube_strategy: String::new() }];
        assert_eq!(find_hint(&hints, 42610).map(|h| h.name.as_str()), Some("Ростелеком"));
        assert!(find_hint(&hints, 1).is_none());
        let url = share_url(&Isp { asn: 12389, name: "PJSC Rostelecom".into() }, "seqovl", "", "1.6.0");
        assert!(url.starts_with("https://github.com/raiyaxkrash/zapret2-rust/issues/new?"));
        assert!(url.contains("template=isp-strategy.yml") && url.contains("asn=AS12389") && !url.contains(' '));
    }
}

//! Регистрация бесплатного аккаунта Cloudflare WARP (тот же API, что у мобильного клиента 1.1.1.1).

use std::time::Duration;

use base64::Engine as _;
use boringtun::x25519::{PublicKey, StaticSecret};
use serde::{Deserialize, Serialize};

const API: &str = "https://api.cloudflareclient.com/v0a2158";
const USER_AGENT: &str = "okhttp/3.12.1";
const CLIENT_VERSION: &str = "a-6.30-2158";

/// Всё, что нужно для подключения. Хранится в %LOCALAPPDATA%\Zapret2GUI\warp.json.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub id: String,
    pub token: String,
    /// Закрытый ключ WireGuard (base64).
    pub private_key: String,
    /// Открытый ключ сервера WARP (base64).
    pub peer_public_key: String,
    /// IPv4-адрес клиента внутри туннеля, например 172.16.0.2.
    pub address_v4: String,
    /// client_id в base64: 3 байта, которые WARP ждёт в «зарезервированных» байтах пакетов.
    #[serde(default)]
    pub client_id: String,
    /// Точка подключения по умолчанию (host:port).
    pub endpoint: String,
}

impl Account {
    pub fn secret(&self) -> Result<StaticSecret, String> {
        let b = b64(&self.private_key)?;
        let arr: [u8; 32] = b.try_into().map_err(|_| "Некорректный ключ WARP".to_string())?;
        Ok(StaticSecret::from(arr))
    }

    pub fn peer(&self) -> Result<PublicKey, String> {
        let b = b64(&self.peer_public_key)?;
        let arr: [u8; 32] = b.try_into().map_err(|_| "Некорректный ключ сервера WARP".to_string())?;
        Ok(PublicKey::from(arr))
    }

    /// Три «зарезервированных» байта для заголовка пакетов WireGuard.
    pub fn reserved(&self) -> [u8; 3] {
        match b64(&self.client_id) {
            Ok(b) if b.len() >= 3 => [b[0], b[1], b[2]],
            _ => [0; 3],
        }
    }
}

fn b64(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD.decode(s.trim()).map_err(|e| format!("base64: {e}"))
}

fn b64e(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

#[derive(Deserialize)]
struct RegResponse {
    id: String,
    token: String,
    config: RegConfig,
}

#[derive(Deserialize)]
struct RegConfig {
    #[serde(default)]
    client_id: String,
    peers: Vec<RegPeer>,
    interface: RegInterface,
}

#[derive(Deserialize)]
struct RegPeer {
    public_key: String,
    endpoint: RegEndpoint,
}

#[derive(Deserialize)]
struct RegEndpoint {
    #[serde(default)]
    host: String,
    #[serde(default)]
    v4: String,
}

#[derive(Deserialize)]
struct RegInterface {
    addresses: RegAddresses,
}

#[derive(Deserialize)]
struct RegAddresses {
    v4: String,
}

/// Регистрирует новое устройство и возвращает данные для подключения.
pub async fn register() -> Result<Account, String> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| e.to_string())?;
    let secret = StaticSecret::from(seed);
    let public = PublicKey::from(&secret);

    let tos = chrono_like_now();
    let body = serde_json::json!({
        "install_id": "",
        "fcm_token": "",
        "tos": tos,
        "key": b64e(public.as_bytes()),
        "type": "Android",
        "model": "PC",
        "locale": "en_US",
        "warp_enabled": true,
    });
    let c = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = c
        .post(format!("{API}/reg"))
        .header("CF-Client-Version", CLIENT_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Сервер WARP недоступен: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Регистрация WARP не удалась: HTTP {status} {}", text.chars().take(200).collect::<String>()));
    }
    let reg: RegResponse = resp.json().await.map_err(|e| format!("Некорректный ответ WARP: {e}"))?;
    let peer = reg.config.peers.into_iter().next().ok_or("WARP не вернул сервер для подключения")?;
    let endpoint = if !peer.endpoint.host.is_empty() {
        peer.endpoint.host
    } else {
        // "162.159.192.1:0" — порт не указан, у WARP стандартный 2408.
        format!("{}:2408", peer.endpoint.v4.split(':').next().unwrap_or("162.159.192.1"))
    };
    Ok(Account {
        id: reg.id,
        token: reg.token,
        private_key: b64e(&seed),
        peer_public_key: peer.public_key,
        address_v4: reg.config.interface.addresses.v4,
        client_id: reg.config.client_id,
        endpoint,
    })
}

/// Время в формате RFC 3339 (как ждёт API), без зависимости от chrono.
fn chrono_like_now() -> String {
    let secs = crate::util::now_unix() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Перевод дней с 1970-01-01 в дату (алгоритм Хиннанта).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339() {
        let s = chrono_like_now();
        assert_eq!(s.len(), 24);
        assert!(s.starts_with("20") && s.ends_with(".000Z") && &s[10..11] == "T");
    }
}

//! Стратегии обхода и сборка командной строки winws2.

use std::fs;

use serde::{Deserialize, Serialize};

use crate::{
    paths::{count_domains, Paths},
    settings::Settings,
    util::split_args,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Strategy {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub tls: Vec<String>,
    pub http: Vec<String>,
    #[serde(default)]
    pub quic: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Presets {
    pub quic_default: Vec<String>,
    pub discord_voice: Vec<String>,
    pub strategies: Vec<Strategy>,
}

impl Presets {
    pub fn load(paths: &Paths) -> Result<Presets, String> {
        let text = fs::read_to_string(paths.presets_file())
            .map_err(|e| format!("Не найден presets.json: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("Ошибка в presets.json: {e}"))
    }

    pub fn custom(settings: &Settings) -> Strategy {
        let quic = split_args(&settings.custom_quic);
        Strategy {
            id: "custom".into(),
            name: "Свои параметры".into(),
            description: "Параметры, заданные вручную.".into(),
            tls: split_args(&settings.custom_tls),
            http: split_args(&settings.custom_http),
            quic: if quic.is_empty() { None } else { Some(quic) },
        }
    }

    /// Все стратегии, включая пользовательскую.
    pub fn all(&self, settings: &Settings) -> Vec<Strategy> {
        let mut v = self.strategies.clone();
        v.push(Self::custom(settings));
        v
    }

    pub fn find(&self, settings: &Settings, id: &str) -> Option<Strategy> {
        self.all(settings).into_iter().find(|s| s.id == id)
    }
}

const YT_TCP: &str = "80,443";
const DISCORD_TCP: &str = "443,2053,2083,2087,2096,8443";
const DISCORD_VOICE_UDP: &str = "19294-19344,50000-50100";

/// Собирает аргументы winws2. Все пути относительные: рабочий каталог — `paths.root`.
pub fn build_args(settings: &Settings, presets: &Presets, strategy: &Strategy, paths: &Paths) -> Result<Vec<String>, String> {
    let quic_args = strategy.quic.clone().unwrap_or_else(|| presets.quic_default.clone());
    let mut tcp_ports: Vec<&str> = Vec::new();
    let mut need_quic = false;
    let mut profiles: Vec<Vec<String>> = Vec::new();

    let tcp_profile = |ports: &str, filters: &[String]| -> Vec<String> {
        let mut p = vec![format!("--filter-tcp={ports}"), "--filter-l7=tls,http".to_string()];
        p.extend_from_slice(filters);
        p.extend(strategy.tls.iter().cloned());
        p.extend(strategy.http.iter().cloned());
        p
    };
    let quic_profile = |filters: &[String]| -> Vec<String> {
        let mut p = vec!["--filter-udp=443".to_string(), "--filter-l7=quic".to_string()];
        p.extend_from_slice(filters);
        p.extend(quic_args.iter().cloned());
        p
    };

    if settings.youtube {
        let f = vec!["--hostlist=lists/youtube.txt".to_string()];
        tcp_ports.push(YT_TCP);
        profiles.push(tcp_profile(YT_TCP, &f));
        if settings.quic {
            need_quic = true;
            profiles.push(quic_profile(&f));
        }
    }
    if settings.discord {
        let f = vec!["--hostlist=lists/discord.txt".to_string()];
        tcp_ports.push(DISCORD_TCP);
        profiles.push(tcp_profile(DISCORD_TCP, &f));
        if settings.quic {
            need_quic = true;
            profiles.push(quic_profile(&f));
        }
    }
    if settings.discord_voice {
        let mut p = vec![
            format!("--filter-udp={DISCORD_VOICE_UDP}"),
            "--filter-l7=discord,stun".to_string(),
        ];
        p.extend(presets.discord_voice.iter().cloned());
        profiles.push(p);
    }
    if settings.custom {
        let user_count = count_domains(&paths.lists.join("user.txt"));
        let exclude = "--hostlist-exclude=lists/exclude.txt".to_string();
        let auto = paths.auto_list();
        let auto_arg = auto.to_string_lossy().replace('\\', "/");
        match settings.custom_mode.as_str() {
            "all" => {
                tcp_ports.push(YT_TCP);
                profiles.push(tcp_profile(YT_TCP, &[exclude.clone()]));
                if settings.quic {
                    need_quic = true;
                    profiles.push(quic_profile(&[exclude]));
                }
            }
            "auto" => {
                tcp_ports.push(YT_TCP);
                profiles.push(tcp_profile(
                    YT_TCP,
                    &[
                        "--hostlist=lists/user.txt".to_string(),
                        exclude.clone(),
                        format!("--hostlist-auto={auto_arg}"),
                    ],
                ));
                // Для QUIC автосписок только читается (писать его может лишь один профиль).
                if settings.quic && (user_count > 0 || count_domains(&auto) > 0) {
                    need_quic = true;
                    profiles.push(quic_profile(&[
                        "--hostlist=lists/user.txt".to_string(),
                        format!("--hostlist={auto_arg}"),
                        exclude,
                    ]));
                }
            }
            _ => {
                // Пустой хостлист в zapret означает «все сайты», поэтому без доменов профиль не добавляем.
                if user_count > 0 {
                    let f = vec!["--hostlist=lists/user.txt".to_string(), exclude];
                    tcp_ports.push(YT_TCP);
                    profiles.push(tcp_profile(YT_TCP, &f));
                    if settings.quic {
                        need_quic = true;
                        profiles.push(quic_profile(&f));
                    }
                }
            }
        }
    }

    if profiles.is_empty() {
        return Err(if settings.custom && !settings.youtube && !settings.discord && !settings.discord_voice {
            "Список «Мои сайты» пуст. Добавьте домены или выберите режим «Все сайты».".into()
        } else {
            "Не выбрано ни одного сервиса для обхода.".into()
        });
    }

    let mut ports: Vec<String> = Vec::new();
    for group in tcp_ports {
        for p in group.split(',') {
            if !ports.iter().any(|x| x == p) {
                ports.push(p.to_string());
            }
        }
    }

    let mut args: Vec<String> = Vec::new();
    if !ports.is_empty() {
        args.push(format!("--wf-tcp-out={}", ports.join(",")));
    }
    if need_quic {
        args.push("--wf-udp-out=443".into());
    }
    if settings.discord_voice {
        args.push("--wf-raw-part=@engine/filters/windivert_part.discord_media.txt".into());
        args.push("--wf-raw-part=@engine/filters/windivert_part.stun.txt".into());
    }
    args.extend(
        [
            "--lua-init=@engine/lua/zapret-lib.lua",
            "--lua-init=@engine/lua/zapret-antidpi.lua",
            "--blob=tls_google:@engine/fake/tls_clienthello_www_google_com.bin",
            "--blob=quic_google:@engine/fake/quic_initial_www_google_com.bin",
        ]
        .map(String::from),
    );
    if settings.debug_log {
        args.push("--debug=1".into());
    }
    for (i, p) in profiles.into_iter().enumerate() {
        if i > 0 {
            args.push("--new".into());
        }
        args.extend(p);
    }
    Ok(args)
}

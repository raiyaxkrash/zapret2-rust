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

/// Готовый набор сайтов (Telegram, Instagram…): список в lists/packs/<id>.txt.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    /// "" — список доменов, "ipset" — список подсетей (блокировка по IP, например зарубежных хостингов).
    #[serde(default)]
    pub kind: String,
    /// Адреса для проверки доступности (быстрая проверка и автоподбор).
    #[serde(default)]
    pub check: Vec<String>,
}

impl Pack {
    pub fn is_ipset(&self) -> bool {
        self.kind == "ipset"
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Presets {
    pub quic_default: Vec<String>,
    pub discord_voice: Vec<String>,
    pub strategies: Vec<Strategy>,
    #[serde(default)]
    pub packs: Vec<Pack>,
    /// Рекомендуемые стратегии для провайдеров (по номеру AS).
    #[serde(default)]
    pub isp: Vec<crate::isp::IspHint>,
}

/// id набора — только латиница, цифры, «-» и «_» (используется как имя файла).
pub fn valid_pack_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
/// `yt` — стратегия для YouTube (может совпадать с основной `strategy`).
pub fn build_args(settings: &Settings, presets: &Presets, strategy: &Strategy, yt: &Strategy, paths: &Paths) -> Result<Vec<String>, String> {
    let mut tcp_ports: Vec<&str> = Vec::new();
    let mut need_quic = false;
    let mut profiles: Vec<Vec<String>> = Vec::new();

    let tcp_profile = |s: &Strategy, ports: &str, filters: &[String]| -> Vec<String> {
        let mut p = vec![format!("--filter-tcp={ports}"), "--filter-l7=tls,http".to_string()];
        p.extend_from_slice(filters);
        p.extend(s.tls.iter().cloned());
        p.extend(s.http.iter().cloned());
        p
    };
    let quic_profile = |s: &Strategy, filters: &[String]| -> Vec<String> {
        let mut p = vec!["--filter-udp=443".to_string(), "--filter-l7=quic".to_string()];
        p.extend_from_slice(filters);
        p.extend(s.quic.clone().unwrap_or_else(|| presets.quic_default.clone()));
        p
    };
    // QUIC выключен: блокируем его, чтобы браузер сразу перешёл на TCP, где работает обход.
    let quic_block = |filters: &[String]| -> Vec<String> {
        let mut p = vec!["--filter-udp=443".to_string(), "--filter-l7=quic".to_string()];
        p.extend_from_slice(filters);
        p.push("--payload=quic_initial".to_string());
        p.push("--lua-desync=drop".to_string());
        p
    };

    // Проверка конкретного сайта: временный профиль только для него, идёт первым.
    if !settings.test_domains.is_empty() {
        let f = vec![format!("--hostlist-domains={}", settings.test_domains.join(","))];
        tcp_ports.push(YT_TCP);
        profiles.push(tcp_profile(strategy, YT_TCP, &f));
        need_quic = true;
        profiles.push(if settings.quic { quic_profile(strategy, &f) } else { quic_block(&f) });
    }

    for (enabled, list, ports, s) in [
        (settings.youtube, "youtube", YT_TCP, yt),
        (settings.discord, "discord", DISCORD_TCP, strategy),
    ] {
        if !enabled {
            continue;
        }
        let f = vec![format!("--hostlist=lists/{list}.txt")];
        tcp_ports.push(ports);
        profiles.push(tcp_profile(s, ports, &f));
        need_quic = true;
        profiles.push(if settings.quic { quic_profile(s, &f) } else { quic_block(&f) });
    }
    // Готовые наборы сайтов: один профиль со всеми включёнными списками доменов
    // и отдельный — со списками подсетей.
    let enabled_packs: Vec<&String> = settings
        .packs
        .iter()
        .filter(|id| valid_pack_id(id))
        .filter(|id| count_domains(&paths.lists.join("packs").join(format!("{id}.txt"))) > 0)
        .collect();
    let is_ipset = |id: &str| presets.packs.iter().any(|p| p.id == id && p.is_ipset());
    let pack_lists: Vec<String> = enabled_packs
        .iter()
        .filter(|id| !is_ipset(id))
        .map(|id| format!("--hostlist=lists/packs/{id}.txt"))
        .collect();
    let ipset_lists: Vec<String> = enabled_packs
        .iter()
        .filter(|id| is_ipset(id))
        .map(|id| format!("--ipset=lists/packs/{id}.txt"))
        .collect();
    if !pack_lists.is_empty() {
        let mut f = pack_lists;
        f.push("--hostlist-exclude=lists/exclude.txt".to_string());
        tcp_ports.push(YT_TCP);
        profiles.push(tcp_profile(strategy, YT_TCP, &f));
        need_quic = true;
        profiles.push(if settings.quic { quic_profile(strategy, &f) } else { quic_block(&f) });
    }
    // Блокировка по IP (зарубежные хостинги и CDN): только TLS на 443, QUIC не трогаем,
    // чтобы не ломать сайты, у которых с ним всё в порядке.
    if !ipset_lists.is_empty() {
        let mut p = vec!["--filter-tcp=443".to_string(), "--filter-l7=tls".to_string()];
        p.extend(ipset_lists);
        p.push("--hostlist-exclude=lists/exclude.txt".to_string());
        p.extend(strategy.tls.iter().cloned());
        tcp_ports.push("443");
        profiles.push(p);
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
                profiles.push(tcp_profile(strategy, YT_TCP, &[exclude.clone()]));
                if settings.quic {
                    need_quic = true;
                    profiles.push(quic_profile(strategy, &[exclude]));
                }
            }
            "auto" => {
                tcp_ports.push(YT_TCP);
                profiles.push(tcp_profile(
                    strategy,
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
                    profiles.push(quic_profile(strategy, &[
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
                    profiles.push(tcp_profile(strategy, YT_TCP, &f));
                    if settings.quic {
                        need_quic = true;
                        profiles.push(quic_profile(strategy, &f));
                    }
                }
            }
        }
    }

    if profiles.is_empty() {
        return Err(if settings.custom && !settings.youtube && !settings.discord && !settings.discord_voice && settings.packs.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn s(id: &str) -> Strategy {
        Strategy {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            tls: vec![format!("--lua-desync=tls_{id}")],
            http: vec![format!("--lua-desync=http_{id}")],
            quic: None,
        }
    }

    fn setup(user_list: &str) -> (Paths, Presets) {
        // Своя папка для каждого вызова: тесты идут параллельно и не должны делить файлы.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("z2gui-test-{}-{n}", std::process::id()));
        let _ = fs::create_dir_all(root.join("lists"));
        fs::write(root.join("lists").join("user.txt"), user_list).unwrap();
        let paths = Paths {
            engine: root.join("engine"),
            lists: root.join("lists"),
            low: root.join("low"),
            resources: PathBuf::from("."),
            remote: root.join("remote"),
            root,
        };
        let presets = Presets {
            quic_default: vec!["--lua-desync=quic_default".into()],
            discord_voice: vec!["--lua-desync=voice".into()],
            strategies: vec![s("main"), s("yt")],
            packs: vec![Pack {
                id: "hosting".into(),
                name: "Хостинги".into(),
                description: String::new(),
                icon: String::new(),
                kind: "ipset".into(),
                check: vec![],
            }],
            isp: vec![],
        };
        (paths, presets)
    }

    fn base() -> Settings {
        Settings { youtube: true, discord: true, discord_voice: false, custom: false, quic: true, ..Settings::default() }
    }

    /// Профили между `--new`.
    fn profiles(args: &[String]) -> Vec<Vec<String>> {
        let start = args.iter().position(|a| a.starts_with("--filter-")).unwrap();
        args[start..].split(|a| a == "--new").map(|p| p.to_vec()).collect()
    }

    #[test]
    fn youtube_gets_own_strategy() {
        let (paths, presets) = setup("");
        let args = build_args(&base(), &presets, &s("main"), &s("yt"), &paths).unwrap();
        let p = profiles(&args);
        let yt_tcp = p.iter().find(|p| p.contains(&"--hostlist=lists/youtube.txt".into()) && p[0].starts_with("--filter-tcp")).unwrap();
        let dc_tcp = p.iter().find(|p| p.contains(&"--hostlist=lists/discord.txt".into()) && p[0].starts_with("--filter-tcp")).unwrap();
        assert!(yt_tcp.contains(&"--lua-desync=tls_yt".into()));
        assert!(dc_tcp.contains(&"--lua-desync=tls_main".into()));
        assert!(args.contains(&"--wf-tcp-out=80,443,2053,2083,2087,2096,8443".into()));
    }

    #[test]
    fn quic_off_blocks_quic() {
        let (paths, presets) = setup("");
        let settings = Settings { quic: false, ..base() };
        let args = build_args(&settings, &presets, &s("main"), &s("main"), &paths).unwrap();
        let quic: Vec<_> = profiles(&args).into_iter().filter(|p| p[0] == "--filter-udp=443").collect();
        assert_eq!(quic.len(), 2);
        assert!(quic.iter().all(|p| p.last() == Some(&"--lua-desync=drop".to_string())));
        assert!(args.contains(&"--wf-udp-out=443".into()));
    }

    #[test]
    fn empty_user_list_is_not_all_sites() {
        // Пустой хостлист в zapret означает «все сайты», поэтому такой профиль добавлять нельзя.
        let (paths, presets) = setup("# только комментарий\n");
        let settings = Settings { youtube: false, discord: false, custom: true, custom_mode: "list".into(), ..base() };
        assert!(build_args(&settings, &presets, &s("main"), &s("main"), &paths).is_err());
    }

    #[test]
    fn user_list_profile() {
        let (paths, presets) = setup("rutracker.org\n");
        let settings = Settings { youtube: false, discord: false, custom: true, custom_mode: "list".into(), ..base() };
        let args = build_args(&settings, &presets, &s("main"), &s("main"), &paths).unwrap();
        assert!(args.contains(&"--hostlist=lists/user.txt".into()));
        assert!(args.contains(&"--hostlist-exclude=lists/exclude.txt".into()));
    }

    #[test]
    fn voice_uses_raw_filters() {
        let (paths, presets) = setup("");
        let settings = Settings { youtube: false, discord: false, discord_voice: true, ..base() };
        let args = build_args(&settings, &presets, &s("main"), &s("main"), &paths).unwrap();
        assert!(args.iter().any(|a| a.contains("windivert_part.discord_media.txt")));
        assert!(args.contains(&"--filter-l7=discord,stun".into()));
        assert!(!args.iter().any(|a| a.starts_with("--wf-tcp-out")));
    }

    #[test]
    fn packs_and_site_check() {
        let (paths, presets) = setup("x");
        let _ = fs::create_dir_all(paths.lists.join("packs"));
        fs::write(paths.lists.join("packs").join("telegram.txt"), "t.me\n").unwrap();
        fs::write(paths.lists.join("packs").join("empty.txt"), "# пусто\n").unwrap();
        let settings = Settings {
            youtube: false,
            discord: false,
            // Пустой набор пропускается, «плохой» id отбрасывается.
            packs: vec!["telegram".into(), "empty".into(), "../evil".into()],
            test_domains: vec!["example.com".into()],
            ..base()
        };
        let args = build_args(&settings, &presets, &s("main"), &s("main"), &paths).unwrap();
        assert!(args.contains(&"--hostlist=lists/packs/telegram.txt".into()));
        assert!(!args.iter().any(|a| a.contains("empty.txt") || a.contains("evil")));
        // Профиль проверяемого сайта идёт первым.
        let first = &profiles(&args)[0];
        assert!(first.contains(&"--hostlist-domains=example.com".into()));
    }

    #[test]
    fn ipset_pack_gets_tls_only_profile() {
        let (paths, presets) = setup("y");
        let _ = fs::create_dir_all(paths.lists.join("packs"));
        fs::write(paths.lists.join("packs").join("hosting.txt"), "# comment\n1.1.1.0/24\n2a06:98c0::/29\n").unwrap();
        fs::write(paths.lists.join("packs").join("telegram.txt"), "t.me\n").unwrap();
        let settings = Settings { youtube: false, discord: false, packs: vec!["hosting".into(), "telegram".into()], ..base() };
        let args = build_args(&settings, &presets, &s("main"), &s("main"), &paths).unwrap();
        let p = profiles(&args);
        let ip = p.iter().find(|p| p.contains(&"--ipset=lists/packs/hosting.txt".into())).unwrap();
        assert_eq!(ip[0], "--filter-tcp=443");
        assert!(ip.contains(&"--filter-l7=tls".into()));
        assert!(ip.contains(&"--lua-desync=tls_main".into()));
        assert!(!ip.contains(&"--lua-desync=http_main".into()));
        // Набор подсетей не попадает в профиль доменов, и для него нет QUIC-профиля.
        assert!(!args.contains(&"--hostlist=lists/packs/hosting.txt".into()));
        assert!(args.contains(&"--hostlist=lists/packs/telegram.txt".into()));
        assert_eq!(p.iter().filter(|p| p[0] == "--filter-udp=443").count(), 1);
    }
}
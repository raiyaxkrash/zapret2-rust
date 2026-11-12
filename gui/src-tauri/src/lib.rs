mod autostart;
mod content;
mod engine;
mod isp;
mod paths;
mod platform;
mod presets;
mod settings;
mod tester;
mod tgproxy;
mod traffic;
mod util;
mod zapret_update;

use std::{
    fs,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, Manager, PhysicalPosition, Rect, State, WebviewUrl, WebviewWindowBuilder, WindowEvent, Wry,
};
use tauri_plugin_updater::UpdaterExt;

use engine::{Conflict, Engine, EngineStatus};
use paths::Paths;
use presets::{build_args, Presets, Strategy};
use settings::Settings;
use tester::TestReport;
use tgproxy::{TgProxy, TgProxyStatus};
use zapret_update::EngineUpdateInfo;

pub struct AppState {
    pub paths: Paths,
    pub settings: Mutex<Settings>,
    pub engine: Arc<Engine>,
    pub tg: Arc<TgProxy>,
    pub testing: AtomicBool,
    pub cancel_test: AtomicBool,
    pub last_report: Mutex<Option<TestReport>>,
    pub busy: tokio::sync::Mutex<()>,
    tray_toggle: Mutex<Option<MenuItem<Wry>>>,
    /// Когда мини-окно в трее скрылось из-за потери фокуса: щелчок по значку
    /// сначала снимает фокус, и без этой отметки окно тут же открылось бы снова.
    flyout_hidden_at: Mutex<Option<Instant>>,
    /// Найденное обновление приложения (для значка в интерфейсе).
    app_update: Mutex<Option<AppUpdateInfo>>,
    /// Провайдер, определённый за этот запуск.
    isp: Mutex<Option<isp::Isp>>,
}

type CmdResult<T> = Result<T, String>;

// ---------------------------------------------------------------- engine control

/// Аргументы winws2. `youtube_id` — стратегия для YouTube; None — из настроек
/// (пустая настройка означает «как основная»).
pub fn args_for(paths: &Paths, settings: &Settings, strategy_id: &str, youtube_id: Option<&str>) -> CmdResult<Vec<String>> {
    let presets = Presets::load(paths)?;
    let strategy = presets
        .find(settings, strategy_id)
        .ok_or_else(|| format!("Стратегия «{strategy_id}» не найдена"))?;
    let yt_id = youtube_id.unwrap_or(&settings.youtube_strategy);
    let yt = if yt_id.is_empty() { None } else { presets.find(settings, yt_id) }.unwrap_or_else(|| strategy.clone());
    build_args(settings, &presets, &strategy, &yt, paths)
}

/// Запускает winws2 с указанной стратегией и проверяет, что он не упал сразу.
pub async fn start_with(app: &AppHandle, settings: &Settings, strategy_id: &str, youtube_id: Option<&str>) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let args = args_for(&state.paths, settings, strategy_id, youtube_id)?;
    let conflicts = state.engine.conflicts();
    if !conflicts.is_empty() {
        let names: Vec<_> = conflicts.iter().map(|c| c.name.clone()).collect();
        return Err(format!(
            "Уже запущены другие программы обхода ({}). Закройте их кнопкой «Завершить» на главном экране.",
            names.join(", ")
        ));
    }
    state.engine.start(app, &state.paths.winws(), &state.paths.root, &args)?;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let st = state.engine.status();
    if !st.running {
        let tail: Vec<String> = state.engine.logs().into_iter().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(format!(
            "winws2 не запустился. {}",
            if tail.is_empty() { String::new() } else { format!("Последние строки журнала:\n{}", tail.join("\n")) }
        ));
    }
    Ok(())
}

async fn start_current(app: &AppHandle) -> CmdResult<()> {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let r = start_with(app, &settings, &settings.strategy, None).await;
    if let Err(e) = &r {
        let state = app.state::<AppState>();
        state.engine.log(app, format!("[app] Ошибка: {e}"));
        state.engine.stop(app);
    }
    update_tray(app);
    r
}

async fn restart_if_running(app: &AppHandle) -> CmdResult<()> {
    if app.state::<AppState>().engine.is_running() {
        start_current(app).await
    } else {
        Ok(())
    }
}

fn update_tray(app: &AppHandle) {
    let state = app.state::<AppState>();
    let running = state.engine.is_running();
    if let Some(item) = state.tray_toggle.lock().unwrap().as_ref() {
        let _ = item.set_text(if running { "Выключить обход" } else { "Включить обход" });
    }
    if let Some(tray) = app.tray_by_id("main") {
        let update = state.app_update.lock().unwrap().as_ref().map(|u| u.latest.clone());
        let mut tip = String::from(if running { "Zapret2 — обход включён" } else { "Zapret2 — обход выключен" });
        if let Some(v) = update {
            tip.push_str(&format!("\nДоступно обновление {v}"));
        }
        let _ = tray.set_tooltip(Some(tip));
        if let Some(icon) = tray_icon(app, running) {
            let _ = tray.set_icon(Some(icon));
        }
    }
}

/// Значок в трее: цветной, когда обход включён, и серый полупрозрачный, когда выключен.
fn tray_icon(app: &AppHandle, running: bool) -> Option<tauri::image::Image<'static>> {
    static OFF: std::sync::OnceLock<Option<(Vec<u8>, u32, u32)>> = std::sync::OnceLock::new();
    let base = app.default_window_icon()?;
    if running {
        return Some(base.clone().to_owned());
    }
    let (rgba, w, h) = OFF
        .get_or_init(|| {
            let mut px = base.rgba().to_vec();
            for p in px.chunks_exact_mut(4) {
                let y = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
                // Светло-серый, чтобы значок был виден и на тёмной, и на светлой панели задач.
                let g = 96 + y / 2;
                p[0] = g;
                p[1] = g;
                p[2] = g;
                p[3] = (p[3] as u16 * 200 / 255) as u8;
            }
            Some((px, base.width(), base.height()))
        })
        .clone()?;
    Some(tauri::image::Image::new_owned(rgba, w, h))
}

/// Автоперезапуск winws2, если он упал во время работы (не сразу после запуска).
/// Не больше трёх попыток за 5 минут, чтобы не крутиться в цикле.
fn on_engine_crash(app: AppHandle) {
    static CRASHES: Mutex<Vec<Instant>> = Mutex::new(Vec::new());
    let allowed = {
        let mut c = CRASHES.lock().unwrap();
        c.retain(|t| t.elapsed() < Duration::from_secs(300));
        c.push(Instant::now());
        c.len() <= 3
    };
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if !allowed {
            state.engine.log(&app, "[app] winws2 падает слишком часто, автоперезапуск остановлен");
            let _ = app.emit("toast", "winws2 несколько раз неожиданно завершился. Обход выключен, подробности в журнале.");
            return;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        let _guard = state.busy.lock().await;
        if state.engine.is_running() || state.testing.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        state.engine.log(&app, "[app] winws2 неожиданно завершился, перезапуск…");
        match start_current(&app).await {
            Ok(()) => state.engine.log(&app, "[app] Обход снова работает"),
            Err(e) => state.engine.log(&app, format!("[app] Перезапуск не удался: {e}")),
        }
    });
}

// ---------------------------------------------------------------- telegram proxy

fn tg_status(state: &AppState) -> TgProxyStatus {
    let s = state.settings.lock().unwrap();
    state.tg.status(s.tg_proxy_port, &s.tg_proxy_secret)
}

/// Приводит прокси для Telegram в соответствие с настройками: запускает,
/// перезапускает (если сменился порт или секрет) или останавливает.
async fn tg_apply(app: &AppHandle, restart: bool) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let (on, port, secret) = {
        let s = state.settings.lock().unwrap();
        (s.tg_proxy, s.tg_proxy_port, s.tg_proxy_secret.clone())
    };
    if !on {
        state.tg.stop(app);
        state.tg.clear_error();
        return Ok(());
    }
    if state.tg.is_running() && !restart {
        return Ok(());
    }
    state.tg.start(app, port, &secret).await
}

fn stop_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.engine.stop(app);
    state.tg.stop(app);
}

/// Показывает или прячет окно и сообщает ему об этом: скрытое окно
/// останавливает анимации и таймеры, чтобы не нагружать видеокарту и процессор.
fn set_shown(app: &AppHandle, label: &str, shown: bool) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = if shown { w.show() } else { w.hide() };
        let _ = app.emit_to(label, "visibility", shown);
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        set_shown(app, "main", true);
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

// ---------------------------------------------------------------- tray flyout

const FLYOUT_W: f64 = 360.0;
const FLYOUT_H: f64 = 392.0;

fn create_flyout(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, "tray", WebviewUrl::App("tray.html".into()))
        .title("Zapret2")
        .inner_size(FLYOUT_W, FLYOUT_H)
        .resizable(false)
        .decorations(false)
        .shadow(true)
        .background_color(tauri::window::Color(10, 10, 10, 255))
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .focused(true)
        .build()?;
    let theme = app.state::<AppState>().settings.lock().unwrap().theme.clone();
    platform::apply_to(&w, match theme.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    });
    Ok(())
}

/// Показывает или прячет мини-окно над значком в трее.
fn toggle_flyout(app: &AppHandle, icon: Rect) {
    let Some(w) = app.get_webview_window("tray") else { return };
    if w.is_visible().unwrap_or(false) {
        set_shown(&app, "tray", false);
        return;
    }
    let state = app.state::<AppState>();
    if let Some(t) = *state.flyout_hidden_at.lock().unwrap() {
        if t.elapsed() < Duration::from_millis(300) {
            return;
        }
    }

    let pos = icon.position.to_physical::<f64>(1.0);
    let size = icon.size.to_physical::<f64>(1.0);
    let (cx, cy) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    if let Ok(Some(m)) = app.monitor_from_point(cx, cy) {
        let scale = m.scale_factor();
        let (fw, fh) = (FLYOUT_W * scale, FLYOUT_H * scale);
        let wa = m.work_area();
        let (ax, ay) = (wa.position.x as f64, wa.position.y as f64);
        let (aw, ah) = (wa.size.width as f64, wa.size.height as f64);
        let gap = 12.0 * scale;
        let x = (cx - fw / 2.0).clamp(ax + gap, ax + aw - fw - gap);
        // Панель задач снизу (обычно) — окно над значком, иначе под ним.
        let y = if cy > ay + ah / 2.0 { ay + ah - fh - gap } else { ay + gap };
        let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    }
    set_shown(app, "tray", true);
    let _ = w.set_focus();
    let _ = app.emit_to("tray", "flyout-shown", ());
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
struct AppInfo {
    status: EngineStatus,
    settings: Settings,
    engine_version: String,
    app_version: String,
    conflicts: Vec<Conflict>,
    testing: bool,
    data_dir: String,
    user_domains: usize,
    auto_domains: usize,
    autostart: bool,
    content_version: u64,
    platform: platform::Platform,
    app_update: Option<AppUpdateInfo>,
    tg_proxy: TgProxyStatus,
}

#[tauri::command]
fn get_state(app: AppHandle, state: State<'_, AppState>) -> AppInfo {
    let settings = state.settings.lock().unwrap().clone();
    AppInfo {
        status: state.engine.status(),
        settings,
        engine_version: state.paths.engine_version(),
        app_version: app.package_info().version.to_string(),
        conflicts: state.engine.conflicts(),
        testing: state.testing.load(std::sync::atomic::Ordering::SeqCst),
        data_dir: state.paths.root.to_string_lossy().to_string(),
        user_domains: paths::count_domains(&state.paths.lists.join("user.txt")),
        auto_domains: paths::count_domains(&state.paths.auto_list()),
        autostart: autostart::is_enabled(),
        content_version: state.paths.content_version(),
        app_update: state.app_update.lock().unwrap().clone(),
        platform: platform::info(),
        tg_proxy: tg_status(&state),
    }
}

#[tauri::command]
fn open_main(app: AppHandle, page: Option<String>) {
    set_shown(&app, "tray", false);
    show_main(&app);
    if let Some(p) = page {
        let _ = app.emit_to("main", "navigate", p);
    }
}

#[tauri::command]
fn hide_flyout(app: AppHandle) {
    set_shown(&app, "tray", false);
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    stop_all(&app);
    app.exit(0);
}

#[tauri::command]
async fn check_content_update(app: AppHandle) -> CmdResult<content::ContentInfo> {
    let paths = app.state::<AppState>().paths.clone();
    content::check(&paths).await
}

#[tauri::command]
async fn update_content(app: AppHandle) -> CmdResult<String> {
    update_content_inner(&app).await
}

async fn update_content_inner(app: &AppHandle) -> CmdResult<String> {
    let state = app.state::<AppState>();
    let paths = state.paths.clone();
    match content::update(&paths).await? {
        None => Ok(format!("Стратегии и списки актуальны (версия {})", paths.content_version())),
        Some(v) => {
            let msg = format!("Стратегии и списки обновлены до версии {v}");
            state.engine.log(app, format!("[app] {msg}"));
            {
                let _guard = state.busy.lock().await;
                if !state.testing.load(std::sync::atomic::Ordering::SeqCst) {
                    let _ = restart_if_running(app).await;
                }
            }
            let _ = app.emit("content-updated", v);
            Ok(msg)
        }
    }
}

#[tauri::command]
async fn start_engine(app: AppHandle) -> CmdResult<EngineStatus> {
    let state = app.state::<AppState>();
    let _guard = state.busy.lock().await;
    if state.testing.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Дождитесь окончания подбора стратегии".into());
    }
    start_current(&app).await?;
    Ok(app.state::<AppState>().engine.status())
}

#[tauri::command]
async fn stop_engine(app: AppHandle) -> CmdResult<EngineStatus> {
    let state = app.state::<AppState>();
    let _guard = state.busy.lock().await;
    state.engine.stop(&app);
    update_tray(&app);
    Ok(state.engine.status())
}

#[tauri::command]
fn get_strategies(state: State<'_, AppState>) -> CmdResult<Vec<Strategy>> {
    let settings = state.settings.lock().unwrap().clone();
    Ok(Presets::load(&state.paths)?.all(&settings))
}

#[derive(Serialize)]
struct PackInfo {
    #[serde(flatten)]
    pack: presets::Pack,
    domains: usize,
}

#[tauri::command]
fn get_packs(state: State<'_, AppState>) -> CmdResult<Vec<PackInfo>> {
    let presets = Presets::load(&state.paths)?;
    Ok(presets
        .packs
        .into_iter()
        .filter(|p| presets::valid_pack_id(&p.id))
        .map(|p| PackInfo {
            domains: paths::count_domains(&state.paths.lists.join("packs").join(format!("{}.txt", p.id))),
            pack: p,
        })
        .collect())
}

#[tauri::command]
async fn save_settings(app: AppHandle, new: Settings) -> CmdResult<Settings> {
    let state = app.state::<AppState>();
    let _guard = state.busy.lock().await;
    let old = state.settings.lock().unwrap().clone();
    let mut new = new;
    if !tgproxy::valid_secret(&new.tg_proxy_secret) {
        new.tg_proxy_secret = old.tg_proxy_secret.clone();
    }
    if new.tg_proxy_port < 1024 {
        return Err("Порт прокси для Telegram должен быть от 1024 до 65535".into());
    }
    if new.autostart != old.autostart || new.autostart != autostart::is_enabled() {
        autostart::set(new.autostart)?;
    }
    new.save(&state.paths.settings_file())?;
    *state.settings.lock().unwrap() = new.clone();
    if old.theme != new.theme {
        platform::apply_theme(&app, &new.theme);
    }

    // Перезапускаем обход только если поменялось то, что влияет на winws2.
    let engine_changed = old.strategy != new.strategy
        || old.youtube_strategy != new.youtube_strategy
        || old.youtube != new.youtube
        || old.discord != new.discord
        || old.discord_voice != new.discord_voice
        || old.custom != new.custom
        || old.custom_mode != new.custom_mode
        || old.quic != new.quic
        || old.packs != new.packs
        || old.debug_log != new.debug_log
        || (new.strategy == "custom"
            && (old.custom_tls != new.custom_tls || old.custom_http != new.custom_http || old.custom_quic != new.custom_quic));
    if engine_changed {
        restart_if_running(&app).await?;
    }
    let _ = app.emit("settings-changed", new.clone());
    let tg_changed = old.tg_proxy_port != new.tg_proxy_port || old.tg_proxy_secret != new.tg_proxy_secret;
    if old.tg_proxy != new.tg_proxy || (new.tg_proxy && tg_changed) {
        tg_apply(&app, tg_changed).await?;
    }
    Ok(new)
}

#[tauri::command]
fn get_command_line(state: State<'_, AppState>) -> CmdResult<String> {
    let settings = state.settings.lock().unwrap().clone();
    let args = args_for(&state.paths, &settings, &settings.strategy, None)?;
    Ok(format!("cd /d \"{}\"\nengine\\bin\\winws2.exe {}", state.paths.root.display(), util::join_args(&args)))
}

#[tauri::command]
fn read_list(state: State<'_, AppState>, name: String) -> CmdResult<String> {
    let p = state.paths.list_file(&name).ok_or("Неизвестный список")?;
    Ok(fs::read_to_string(p).unwrap_or_default())
}

#[tauri::command]
async fn write_list(app: AppHandle, name: String, content: String) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if paths::BUILTIN_LISTS.contains(&name.as_str()) || name.starts_with("pack:") {
        return Err("Встроенные списки обновляются вместе с приложением. Добавляйте сайты в «Мои сайты».".into());
    }
    let p = state.paths.list_file(&name).ok_or("Неизвестный список")?;
    let normalized: String = content.replace("\r\n", "\n").lines().map(|l| format!("{}\n", l.trim())).collect();
    fs::write(&p, normalized).map_err(|e| format!("Не удалось сохранить список: {e}"))?;
    // Списки читаются winws2 только при старте.
    let _guard = state.busy.lock().await;
    restart_if_running(&app).await
}

/// Добавляет сайт в «Мои сайты» (или в исключения). Принимает и домен, и ссылку целиком.
#[tauri::command]
async fn add_domain(app: AppHandle, input: String, list: Option<String>) -> CmdResult<String> {
    let state = app.state::<AppState>();
    let list = list.unwrap_or_else(|| "user".into());
    if !paths::USER_LISTS.contains(&list.as_str()) {
        return Err("Неизвестный список".into());
    }
    let domain = util::normalize_domain(&input)?;
    let p = state.paths.list_file(&list).ok_or("Неизвестный список")?;
    let text = fs::read_to_string(&p).unwrap_or_default();
    if text.lines().any(|l| l.trim().eq_ignore_ascii_case(&domain)) {
        return Ok(format!("{domain} уже есть в списке"));
    }
    let mut new_text = text;
    if !new_text.is_empty() && !new_text.ends_with('\n') {
        new_text.push('\n');
    }
    new_text.push_str(&domain);
    new_text.push('\n');
    fs::write(&p, new_text).map_err(|e| format!("Не удалось сохранить список: {e}"))?;

    let _guard = state.busy.lock().await;
    let mut msg = format!("{domain} добавлен");
    // Чтобы сайт сразу заработал, включаем сервис «Мои сайты».
    if list == "user" {
        let mut s = state.settings.lock().unwrap().clone();
        if !s.custom {
            s.custom = true;
            s.save(&state.paths.settings_file())?;
            *state.settings.lock().unwrap() = s.clone();
            let _ = app.emit("settings-changed", s);
            msg.push_str(", сервис «Мои сайты» включён");
        }
    }
    restart_if_running(&app).await?;
    Ok(msg)
}

/// Включает тестовый режим подписи драйверов (только ARM64, только по кнопке с подтверждением).
#[tauri::command]
fn enable_testsigning() -> CmdResult<String> {
    platform::enable_testsigning()?;
    Ok("Тестовый режим включён. Перезагрузите компьютер, после этого обход заработает.".into())
}

/// Быстрая проверка доступности сервисов с текущими настройками (без перезапуска обхода).
#[tauri::command]
async fn quick_check(app: AppHandle) -> CmdResult<tester::StrategyResult> {
    let state = app.state::<AppState>();
    if state.testing.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Идёт подбор стратегии".into());
    }
    let settings = state.settings.lock().unwrap().clone();
    let urls = tester::targets(&settings, &state);
    Ok(tester::run_checks("now", "Сейчас", &urls).await)
}

#[tauri::command]
fn get_logs(state: State<'_, AppState>) -> Vec<String> {
    state.engine.logs()
}

#[tauri::command]
fn clear_logs(state: State<'_, AppState>) {
    state.engine.clear_logs();
}

#[tauri::command]
async fn run_strategy_test(app: AppHandle, site: Option<String>) -> CmdResult<TestReport> {
    // Проверка конкретного сайта: принимаем и ссылку, и домен.
    let site = match site.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        Some(s) => Some(util::normalize_domain(&s)?),
        None => None,
    };
    {
        let state = app.state::<AppState>();
        let _guard = state.busy.lock().await;
    }
    let r = tester::run(app.clone(), site).await;
    update_tray(&app);
    r
}

#[tauri::command]
fn cancel_strategy_test(state: State<'_, AppState>) {
    state.cancel_test.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
fn get_last_report(state: State<'_, AppState>) -> Option<TestReport> {
    state.last_report.lock().unwrap().clone()
}

#[tauri::command]
fn kill_conflicts(state: State<'_, AppState>) -> usize {
    state.engine.kill_conflicts()
}

#[tauri::command]
fn open_folder(state: State<'_, AppState>, which: String) -> CmdResult<()> {
    let p = match which.as_str() {
        "lists" => state.paths.lists.clone(),
        "engine" => state.paths.engine.clone(),
        _ => state.paths.root.clone(),
    };
    util::hidden_command("explorer").arg(p).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_url(url: String) -> CmdResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Недопустимая ссылка".into());
    }
    util::hidden_command("rundll32").args(["url.dll,FileProtocolHandler", &url]).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn tg_proxy_start(app: AppHandle) -> CmdResult<TgProxyStatus> {
    tg_apply(&app, true).await?;
    Ok(tg_status(&app.state::<AppState>()))
}

/// Открывает ссылку tg://proxy — Telegram Desktop сам предложит подключиться к прокси.
#[tauri::command]
fn tg_proxy_open(state: State<'_, AppState>) -> CmdResult<()> {
    let link = tg_status(&state).link;
    if link.is_empty() {
        return Err("Ссылка на прокси ещё не готова".into());
    }
    util::hidden_command("rundll32")
        .args(["url.dll,FileProtocolHandler", &link])
        .spawn()
        .map_err(|e| format!("Не удалось открыть Telegram: {e}"))?;
    Ok(())
}

/// Суммарные счётчики трафика физических адаптеров (для графика на главной).
#[tauri::command]
fn net_traffic() -> traffic::Totals {
    traffic::totals()
}

// ---------------------------------------------------------------- провайдер

#[derive(Serialize)]
struct IspInfo {
    isp: isp::Isp,
    /// Рекомендуемая стратегия для этого провайдера, если она известна.
    hint: Option<isp::IspHint>,
}

#[tauri::command]
async fn get_isp(app: AppHandle) -> CmdResult<IspInfo> {
    let state = app.state::<AppState>();
    let cached = state.isp.lock().unwrap().clone();
    let isp = match cached {
        Some(i) => i,
        None => {
            let i = isp::detect().await?;
            *state.isp.lock().unwrap() = Some(i.clone());
            i
        }
    };
    let hint = Presets::load(&state.paths).ok().and_then(|p| isp::find_hint(&p.isp, isp.asn).cloned());
    Ok(IspInfo { isp, hint })
}

/// Открывает заполненный issue на GitHub с провайдером и выбранными стратегиями.
#[tauri::command]
async fn share_isp_result(app: AppHandle) -> CmdResult<()> {
    let info = get_isp(app.clone()).await?;
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    let presets = Presets::load(&state.paths)?;
    let name = |id: &str| presets.find(&settings, id).map(|s| format!("{} ({id})", s.name)).unwrap_or_default();
    let url = isp::share_url(&info.isp, &name(&settings.strategy), &name(&settings.youtube_strategy), &app.package_info().version.to_string());
    open_url(url)
}

// ---------------------------------------------------------------- zapret2 updates

#[tauri::command]
async fn check_engine_update(app: AppHandle) -> CmdResult<EngineUpdateInfo> {
    let paths = app.state::<AppState>().paths.clone();
    zapret_update::check(&paths).await
}

#[tauri::command]
async fn install_engine_update(app: AppHandle) -> CmdResult<String> {
    install_engine_update_inner(&app).await
}

async fn install_engine_update_inner(app: &AppHandle) -> CmdResult<String> {
    let state = app.state::<AppState>();
    let paths = state.paths.clone();
    let info = zapret_update::check(&paths).await?;
    if !info.available {
        return Ok(format!("Установлена последняя версия zapret2 ({})", info.current));
    }
    let _ = app.emit("engine-update-progress", "Загрузка…");
    state.engine.log(app, format!("[app] Загрузка zapret2 {}…", info.latest));
    let staging = zapret_update::download(&paths, &info).await?;

    let _guard = state.busy.lock().await;
    if state.testing.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Сейчас идёт подбор стратегии, попробуйте позже".into());
    }
    let was_running = state.engine.is_running();
    state.engine.stop(app);
    let _ = app.emit("engine-update-progress", "Установка…");
    let res = zapret_update::install(&paths, &staging).and_then(|_| paths.apply_arm64_driver());
    if was_running {
        let _ = start_current(app).await;
    }
    res?;
    let msg = format!("zapret2 обновлён до версии {}", info.latest);
    state.engine.log(app, format!("[app] {msg}"));
    let _ = app.emit("engine-updated", info.latest.clone());
    Ok(msg)
}

// ---------------------------------------------------------------- app updates

#[derive(Serialize, Clone)]
struct AppUpdateInfo {
    configured: bool,
    available: bool,
    current: String,
    latest: String,
    notes: String,
}

async fn find_app_update(app: &AppHandle) -> CmdResult<(AppUpdateInfo, Option<tauri_plugin_updater::Update>)> {
    let url = app.state::<AppState>().settings.lock().unwrap().app_update_url.trim().to_string();
    let current = app.package_info().version.to_string();
    if url.is_empty() {
        return Ok((
            AppUpdateInfo { configured: false, available: false, current: current.clone(), latest: current, notes: String::new() },
            None,
        ));
    }
    let endpoint = url.parse().map_err(|_| "Некорректный адрес источника обновлений".to_string())?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| format!("Не удалось проверить обновления: {e}"))?;
    let result = match update {
        Some(u) => (
            AppUpdateInfo {
                configured: true,
                available: true,
                current,
                latest: u.version.clone(),
                notes: u.body.clone().unwrap_or_default(),
            },
            Some(u),
        ),
        None => (AppUpdateInfo { configured: true, available: false, current: current.clone(), latest: current, notes: String::new() }, None),
    };
    // Запоминаем для значка «Доступно обновление» в интерфейсе.
    *app.state::<AppState>().app_update.lock().unwrap() = result.0.available.then(|| result.0.clone());
    Ok(result)
}

#[tauri::command]
async fn check_app_update(app: AppHandle) -> CmdResult<AppUpdateInfo> {
    Ok(find_app_update(&app).await?.0)
}

#[tauri::command]
async fn install_app_update(app: AppHandle) -> CmdResult<()> {
    let (_, update) = find_app_update(&app).await?;
    let update = update.ok_or("Обновлений нет")?;
    let mut downloaded = 0usize;
    let handle = app.clone();
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk;
                let _ = handle.emit("app-update-progress", (downloaded, total));
            },
            || {},
        )
        .await
        .map_err(|e| format!("Ошибка загрузки обновления: {e}"))?;
    // Перед установкой останавливаем winws2 (его файлы будут заменены) и прокси для Telegram.
    stop_all(&app);
    update.install(bytes).map_err(|e| format!("Ошибка установки: {e}"))?;
    app.restart();
}

// ---------------------------------------------------------------- background tasks

fn spawn_background(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(8)).await;
        loop {
            let settings = app.state::<AppState>().settings.lock().unwrap().clone();
            if settings.engine_auto_update {
                let paths = app.state::<AppState>().paths.clone();
                match zapret_update::check(&paths).await {
                    Ok(info) if info.available => {
                        if let Err(e) = install_engine_update_inner(&app).await {
                            app.state::<AppState>().engine.log(&app, format!("[app] Автообновление zapret2: {e}"));
                        }
                    }
                    Ok(_) => {}
                    Err(e) => app.state::<AppState>().engine.log(&app, format!("[app] Проверка обновлений zapret2: {e}")),
                }
            }
            if settings.content_auto_update {
                if let Err(e) = update_content_inner(&app).await {
                    app.state::<AppState>().engine.log(&app, format!("[app] Обновление стратегий и списков: {e}"));
                }
            }
            if settings.app_auto_check && !settings.app_update_url.trim().is_empty() {
                if let Ok((info, Some(_))) = find_app_update(&app).await {
                    update_tray(&app);
                    let _ = app.emit("app-update-available", info);
                }
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Открыть Zapret2", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Включить обход", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &toggle, &sep, &quit])?;
    *app.state::<AppState>().tray_toggle.lock().unwrap() = Some(toggle);

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("no icon"))
        .tooltip("Zapret2")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "toggle" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let _guard = state.busy.lock().await;
                    if state.engine.is_running() {
                        state.engine.stop(&app);
                        update_tray(&app);
                    } else if let Err(e) = start_current(&app).await {
                        let _ = app.emit("toast", format!("Не удалось включить обход: {e}"));
                        show_main(&app);
                    }
                });
            }
            "quit" => {
                stop_all(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } => {
                toggle_flyout(tray.app_handle(), rect);
            }
            TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                open_main(tray.app_handle().clone(), None);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    let autostarted = std::env::args().any(|a| a == "--autostart");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let handle = app.handle().clone();
            let resources = app.path().resource_dir()?;
            let paths = Paths::new(resources).map_err(|e| e.to_string())?;

            // Остатки прошлого запуска (например, после аварийного завершения).
            let _ = util::hidden_command("taskkill").args(["/F", "/IM", "winws2.exe"]).output();
            if let Err(e) = paths.sync_resources() {
                eprintln!("sync resources: {e}");
            }
            let settings = Settings::load(&paths.settings_file());
            let _ = settings.save(&paths.settings_file());

            app.manage(AppState {
                paths,
                settings: Mutex::new(settings.clone()),
                engine: Arc::new(Engine::default()),
                tg: Arc::new(TgProxy::default()),
                testing: AtomicBool::new(false),
                cancel_test: AtomicBool::new(false),
                last_report: Mutex::new(None),
                busy: tokio::sync::Mutex::new(()),
                tray_toggle: Mutex::new(None),
                flyout_hidden_at: Mutex::new(None),
                app_update: Mutex::new(None),
                isp: Mutex::new(None),
            });

            platform::apply_theme(&handle, &settings.theme);
            create_flyout(&handle)?;
            build_tray(&handle)?;
            update_tray(&handle);
            {
                let h = handle.clone();
                handle.listen_any("engine-state", move |_| update_tray(&h));
                let h = handle.clone();
                handle.listen_any("engine-crashed", move |_| on_engine_crash(h.clone()));
            }

            if !(autostarted && settings.start_minimized) {
                show_main(&handle);
            }
            if settings.engine_on_launch {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state = h.state::<AppState>();
                    let _guard = state.busy.lock().await;
                    if let Err(e) = start_current(&h).await {
                        let _ = h.emit("toast", format!("Не удалось включить обход: {e}"));
                    }
                });
            }
            if settings.tg_proxy {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = tg_apply(&h, false).await {
                        let _ = h.emit("toast", format!("Прокси для Telegram: {e}"));
                    }
                });
            }
            spawn_background(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.app_handle();
            if window.label() == "tray" {
                match event {
                    WindowEvent::Focused(false) => {
                        if window.is_visible().unwrap_or(false) {
                            *app.state::<AppState>().flyout_hidden_at.lock().unwrap() = Some(Instant::now());
                            set_shown(app, window.label(), false);
                        }
                    }
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        set_shown(app, window.label(), false);
                    }
                    _ => {}
                }
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = app.state::<AppState>().settings.lock().unwrap().close_to_tray;
                if to_tray {
                    api.prevent_close();
                    set_shown(app, window.label(), false);
                } else {
                    stop_all(app);
                    app.exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            start_engine,
            stop_engine,
            get_strategies,
            save_settings,
            get_command_line,
            read_list,
            write_list,
            get_logs,
            clear_logs,
            run_strategy_test,
            cancel_strategy_test,
            get_last_report,
            kill_conflicts,
            open_folder,
            open_url,
            check_engine_update,
            install_engine_update,
            check_app_update,
            install_app_update,
            open_main,
            hide_flyout,
            add_domain,
            quick_check,
            enable_testsigning,
            get_packs,
            quit_app,
            check_content_update,
            update_content,
            tg_proxy_start,
            tg_proxy_open,
            get_isp,
            net_traffic,
            share_isp_result,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.engine.stop(app);
                    state.tg.stop(app);
                }
            }
        });
}

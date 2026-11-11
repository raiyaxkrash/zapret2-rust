mod autostart;
mod engine;
mod paths;
mod presets;
mod settings;
mod tester;
mod util;
mod zapret_update;

use std::{
    fs,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent, Wry,
};
use tauri_plugin_updater::UpdaterExt;

use engine::{Conflict, Engine, EngineStatus};
use paths::Paths;
use presets::{build_args, Presets, Strategy};
use settings::Settings;
use tester::TestReport;
use zapret_update::EngineUpdateInfo;

pub struct AppState {
    pub paths: Paths,
    pub settings: Mutex<Settings>,
    pub engine: Arc<Engine>,
    pub testing: AtomicBool,
    pub cancel_test: AtomicBool,
    pub last_report: Mutex<Option<TestReport>>,
    pub busy: tokio::sync::Mutex<()>,
    tray_toggle: Mutex<Option<MenuItem<Wry>>>,
}

type CmdResult<T> = Result<T, String>;

// ---------------------------------------------------------------- engine control

/// Запускает winws2 с указанной стратегией и проверяет, что он не упал сразу.
pub async fn start_with(app: &AppHandle, settings: &Settings, strategy_id: &str) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let presets = Presets::load(&state.paths)?;
    let strategy = presets
        .find(settings, strategy_id)
        .ok_or_else(|| format!("Стратегия «{strategy_id}» не найдена"))?;
    let args = build_args(settings, &presets, &strategy, &state.paths)?;
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
    let r = start_with(app, &settings, &settings.strategy).await;
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
        let _ = tray.set_tooltip(Some(if running { "Zapret2 — обход включён" } else { "Zapret2 — обход выключен" }));
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
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

#[tauri::command]
async fn save_settings(app: AppHandle, new: Settings) -> CmdResult<Settings> {
    let state = app.state::<AppState>();
    let _guard = state.busy.lock().await;
    let old = state.settings.lock().unwrap().clone();
    if new.autostart != old.autostart || new.autostart != autostart::is_enabled() {
        autostart::set(new.autostart)?;
    }
    new.save(&state.paths.settings_file())?;
    *state.settings.lock().unwrap() = new.clone();

    // Перезапускаем обход только если поменялось то, что влияет на winws2.
    let engine_changed = old.strategy != new.strategy
        || old.youtube != new.youtube
        || old.discord != new.discord
        || old.discord_voice != new.discord_voice
        || old.custom != new.custom
        || old.custom_mode != new.custom_mode
        || old.quic != new.quic
        || old.debug_log != new.debug_log
        || (new.strategy == "custom"
            && (old.custom_tls != new.custom_tls || old.custom_http != new.custom_http || old.custom_quic != new.custom_quic));
    if engine_changed {
        restart_if_running(&app).await?;
    }
    let _ = app.emit("settings-changed", new.clone());
    Ok(new)
}

#[tauri::command]
fn get_command_line(state: State<'_, AppState>) -> CmdResult<String> {
    let settings = state.settings.lock().unwrap().clone();
    let presets = Presets::load(&state.paths)?;
    let strategy = presets.find(&settings, &settings.strategy).ok_or("Стратегия не найдена")?;
    let args = build_args(&settings, &presets, &strategy, &state.paths)?;
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
    if paths::BUILTIN_LISTS.contains(&name.as_str()) {
        return Err("Встроенные списки обновляются вместе с приложением. Добавляйте сайты в «Мои сайты».".into());
    }
    let p = state.paths.list_file(&name).ok_or("Неизвестный список")?;
    let normalized: String = content.replace("\r\n", "\n").lines().map(|l| format!("{}\n", l.trim())).collect();
    fs::write(&p, normalized).map_err(|e| format!("Не удалось сохранить список: {e}"))?;
    // Списки читаются winws2 только при старте.
    let _guard = state.busy.lock().await;
    restart_if_running(&app).await
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
async fn run_strategy_test(app: AppHandle) -> CmdResult<TestReport> {
    {
        let state = app.state::<AppState>();
        let _guard = state.busy.lock().await;
    }
    let r = tester::run(app.clone()).await;
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
    let res = zapret_update::install(&paths, &staging);
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
    Ok(match update {
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
    })
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
    // Перед установкой останавливаем winws2.
    app.state::<AppState>().engine.stop(&app);
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
            if settings.app_auto_check && !settings.app_update_url.trim().is_empty() {
                if let Ok((info, Some(_))) = find_app_update(&app).await {
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
                app.state::<AppState>().engine.stop(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
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
                testing: AtomicBool::new(false),
                cancel_test: AtomicBool::new(false),
                last_report: Mutex::new(None),
                busy: tokio::sync::Mutex::new(()),
                tray_toggle: Mutex::new(None),
            });

            build_tray(&handle)?;

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
            spawn_background(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = window.app_handle().state::<AppState>().settings.lock().unwrap().close_to_tray;
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.engine.stop(app);
                }
            }
        });
}

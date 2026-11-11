//! Запуск и остановка winws2, сбор его вывода.

use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Child, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::util::{hidden_command, join_args, now_unix};

const MAX_LOG: usize = 3000;
/// Процессы, которые тоже используют WinDivert и мешают работе.
const CONFLICTING: &[&str] = &["winws.exe", "winws2.exe", "goodbyedpi.exe"];

#[derive(Default)]
struct Inner {
    child: Option<Child>,
    since: Option<u64>,
    stopping: bool,
    generation: u64,
    last_error: Option<String>,
    command_line: String,
}

#[derive(Default)]
pub struct Engine {
    inner: Mutex<Inner>,
    logs: Mutex<VecDeque<String>>,
}

#[derive(Serialize, Clone, Debug)]
pub struct EngineStatus {
    pub running: bool,
    pub since: Option<u64>,
    pub last_error: Option<String>,
    pub command_line: String,
    pub pid: Option<u32>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Conflict {
    pub name: String,
    pub pid: u32,
}

impl Engine {
    pub fn status(&self) -> EngineStatus {
        let mut g = self.inner.lock().unwrap();
        let mut running = false;
        let mut pid = None;
        if let Some(c) = g.child.as_mut() {
            if matches!(c.try_wait(), Ok(None)) {
                running = true;
                pid = Some(c.id());
            }
        }
        EngineStatus {
            running,
            since: if running { g.since } else { None },
            last_error: g.last_error.clone(),
            command_line: g.command_line.clone(),
            pid,
        }
    }

    pub fn is_running(&self) -> bool {
        self.status().running
    }

    pub fn log(&self, app: &AppHandle, line: impl Into<String>) {
        let line = line.into();
        {
            let mut l = self.logs.lock().unwrap();
            if l.len() >= MAX_LOG {
                l.pop_front();
            }
            l.push_back(line.clone());
        }
        let _ = app.emit("log", line);
    }

    pub fn logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear_logs(&self) {
        self.logs.lock().unwrap().clear();
    }

    fn emit_state(&self, app: &AppHandle) {
        let _ = app.emit("engine-state", self.status());
    }

    /// Запускает winws2. Возвращается сразу после старта процесса;
    /// проверку того, что он не упал, делает вызывающий код.
    pub fn start(self: &Arc<Self>, app: &AppHandle, exe: &Path, cwd: &Path, args: &[String]) -> Result<(), String> {
        self.stop(app);
        if !exe.exists() {
            return Err(format!("Не найден {}", exe.display()));
        }
        let cmdline = format!("winws2.exe {}", join_args(args));
        self.log(app, format!("[app] Запуск: {cmdline}"));

        let mut child = hidden_command(&exe.to_string_lossy())
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Не удалось запустить winws2: {e}"))?;

        if let Some(out) = child.stdout.take() {
            self.spawn_reader(app.clone(), out);
        }
        if let Some(err) = child.stderr.take() {
            self.spawn_reader(app.clone(), err);
        }

        let generation = {
            let mut g = self.inner.lock().unwrap();
            g.child = Some(child);
            g.since = Some(now_unix());
            g.stopping = false;
            g.last_error = None;
            g.command_line = cmdline;
            g.generation += 1;
            g.generation
        };
        self.spawn_monitor(app.clone(), generation);
        self.emit_state(app);
        Ok(())
    }

    fn spawn_reader<R: Read + Send + 'static>(self: &Arc<Self>, app: AppHandle, stream: R) {
        let me = Arc::clone(self);
        thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                        if !line.is_empty() {
                            me.log(&app, line);
                        }
                    }
                }
            }
        });
    }

    fn spawn_monitor(self: &Arc<Self>, app: AppHandle, generation: u64) {
        let me = Arc::clone(self);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(400));
            let exited = {
                let mut g = me.inner.lock().unwrap();
                if g.generation != generation {
                    return;
                }
                let status = match g.child.as_mut() {
                    Some(c) => c.try_wait(),
                    None => return,
                };
                match status {
                    Ok(Some(st)) => {
                        g.child = None;
                        g.since = None;
                        if !g.stopping {
                            let msg = format!("winws2 неожиданно завершился (код {}). Подробности в журнале.", st.code().unwrap_or(-1));
                            g.last_error = Some(msg);
                        }
                        true
                    }
                    Ok(None) => false,
                    Err(_) => false,
                }
            };
            if exited {
                if let Some(e) = me.status().last_error {
                    me.log(&app, format!("[app] {e}"));
                }
                me.emit_state(&app);
                return;
            }
        });
    }

    pub fn stop(&self, app: &AppHandle) {
        let had = {
            let mut g = self.inner.lock().unwrap();
            g.generation += 1;
            g.since = None;
            match g.child.take() {
                Some(mut c) => {
                    g.stopping = true;
                    let _ = c.kill();
                    let _ = c.wait();
                    g.stopping = false;
                    true
                }
                None => false,
            }
        };
        if had {
            self.log(app, "[app] Обход остановлен");
            self.emit_state(app);
        }
    }

    /// Сторонние процессы, использующие WinDivert (кроме нашего).
    pub fn conflicts(&self) -> Vec<Conflict> {
        let own = self.status().pid;
        let out = match hidden_command("tasklist").args(["/FO", "CSV", "/NH"]).output() {
            Ok(o) => o,
            Err(_) => return vec![],
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let mut parts = l.split("\",\"");
                let name = parts.next()?.trim_matches('"').to_string();
                let pid: u32 = parts.next()?.trim_matches('"').parse().ok()?;
                let lname = name.to_lowercase();
                (CONFLICTING.contains(&lname.as_str()) && Some(pid) != own).then_some(Conflict { name, pid })
            })
            .collect()
    }

    pub fn kill_conflicts(&self) -> usize {
        let list = self.conflicts();
        for c in &list {
            let _ = hidden_command("taskkill").args(["/F", "/PID", &c.pid.to_string()]).output();
        }
        list.len()
    }
}

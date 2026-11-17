//! Глобальная горячая клавиша Ctrl+Shift+Z для быстрого включения и выключения обхода
//! поверх любого окна или полноэкранной игры (через WinAPI RegisterHotKey, без опроса).

use std::{
    sync::Mutex,
    thread,
};

use windows_sys::Win32::{
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
        },
        WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT},
    },
};

const HOTKEY_ID: i32 = 0x5A32; // 'Z2'
const VK_Z: u32 = 0x5A;

#[derive(Default)]
pub struct GlobalHotkey {
    thread_id: Mutex<Option<u32>>,
}

impl GlobalHotkey {
    pub fn apply(&self, enabled: bool, on_press: impl Fn() + Send + 'static) {
        self.stop();
        if !enabled {
            return;
        }
        let (tx, rx) = std::sync::mpsc::sync_channel::<Option<u32>>(1);
        thread::spawn(move || unsafe {
            let tid = GetCurrentThreadId();
            let mods = MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT;
            if RegisterHotKey(std::ptr::null_mut(), HOTKEY_ID, mods, VK_Z) == 0 {
                let _ = tx.send(None);
                return;
            }
            let _ = tx.send(Some(tid));
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY && msg.wParam as i32 == HOTKEY_ID {
                    on_press();
                }
            }
            let _ = UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
        });
        if let Ok(Some(tid)) = rx.recv() {
            *self.thread_id.lock().unwrap() = Some(tid);
        }
    }

    pub fn stop(&self) {
        if let Some(tid) = self.thread_id.lock().unwrap().take() {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, 0, 0);
            }
        }
    }
}

impl Drop for GlobalHotkey {
    fn drop(&mut self) {
        self.stop();
    }
}

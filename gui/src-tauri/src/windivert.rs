//! Общая обёртка над WinDivert.dll, таблица приоритетов фильтров и расчёт контрольных сумм.
//!
//! Приоритеты всех WinDivert-слоёв в приложении заданы здесь единым списком,
//! чтобы `DnsGuard`, `warp::apps` и `winws2.exe` не мешали друг другу:
//!
//! - `1100` (`PRIORITY_DNS_INJECT_IN`)   — впрыск чистого DoH-ответа выше WARP, чтобы WARP видел реальные IP
//! - `1000` (`PRIORITY_WARP_APPS`)       — перехват DNS-ответов и трафика выбранных доменов/`.exe` в WARP
//! - `950`  (`PRIORITY_DNS_CAPTURE`)     — перехват исходящих UDP:53 (IPv4 + IPv6) в `DnsGuard`
//! - `900`  (`PRIORITY_DNS_PASSTHROUGH`) — пропуск локальных/неразрешённых DNS-запросов ниже перехвата
//! - `0`    (по умолчанию)               — процесс `winws2.exe` и SOCKET-sniff `warp::apps`

use std::{
    collections::HashMap,
    ffi::c_void,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub const PRIORITY_DNS_INJECT_IN: i16 = 1100;
pub const PRIORITY_WARP_APPS: i16 = 1000;
pub const PRIORITY_DNS_CAPTURE: i16 = 950;
pub const PRIORITY_DNS_PASSTHROUGH: i16 = 900;

pub const LAYER_NETWORK: i32 = 0;
pub const LAYER_SOCKET: i32 = 3;

pub const FLAG_DIVERT: u64 = 0;
pub const FLAG_SNIFF_RECV_ONLY: u64 = 1 | 4;
pub const FLAG_SEND_ONLY: u64 = 0x8;
pub const SHUTDOWN_BOTH: i32 = 3;

pub const ADDR_OUTBOUND: u32 = 1 << 17;
pub const ADDR_LOOPBACK: u32 = 1 << 18;
pub const ADDR_IPV6: u32 = 1 << 20;
pub const ADDR_CHECKSUMS_OK: u32 = (1 << 21) | (1 << 22) | (1 << 23);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Addr {
    pub timestamp: i64,
    pub flags: u32,
    pub reserved: u32,
    pub if_idx: u32,
    pub sub_if_idx: u32,
    pub rest: [u8; 56],
}

impl Default for Addr {
    fn default() -> Self {
        Self {
            timestamp: 0,
            flags: 0,
            reserved: 0,
            if_idx: 0,
            sub_if_idx: 0,
            rest: [0; 56],
        }
    }
}

type Handle = *mut c_void;
type Proc = unsafe extern "system" fn() -> isize;
type OpenFn = unsafe extern "C" fn(*const u8, i32, i16, u64) -> Handle;
type RecvFn = unsafe extern "C" fn(Handle, *mut u8, u32, *mut u32, *mut Addr) -> i32;
type SendFn = unsafe extern "C" fn(Handle, *const u8, u32, *mut u32, *const Addr) -> i32;
type CloseFn = unsafe extern "C" fn(Handle) -> i32;
type ShutdownFn = unsafe extern "C" fn(Handle, i32) -> i32;

#[derive(Clone, Copy)]
pub struct Dll {
    open: OpenFn,
    recv: RecvFn,
    send: SendFn,
    close: CloseFn,
    shutdown: ShutdownFn,
}

pub struct HandleGuard {
    h: Handle,
    close: CloseFn,
}

impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe { (self.close)(self.h) };
    }
}
unsafe impl Send for HandleGuard {}
unsafe impl Sync for HandleGuard {}

impl Dll {
    /// Загружает WinDivert.dll (с кэшированием по пути, чтобы не плодить счётчик LoadLibraryW).
    pub fn load(path: &Path) -> Result<Dll, String> {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, Dll>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(g) = cache.lock() {
            if let Some(dll) = g.get(path) {
                return Ok(*dll);
            }
        }
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let lib = unsafe { LoadLibraryW(wide.as_ptr()) };
        if lib.is_null() {
            return Err(format!("Не удалось загрузить {}", path.display()));
        }
        let sym = |name: &[u8]| {
            unsafe { GetProcAddress(lib, name.as_ptr()) }.ok_or_else(|| "WinDivert.dll: нет нужной функции".to_string())
        };
        let dll = unsafe {
            Dll {
                open: std::mem::transmute::<Proc, OpenFn>(sym(b"WinDivertOpen\0")?),
                recv: std::mem::transmute::<Proc, RecvFn>(sym(b"WinDivertRecv\0")?),
                send: std::mem::transmute::<Proc, SendFn>(sym(b"WinDivertSend\0")?),
                close: std::mem::transmute::<Proc, CloseFn>(sym(b"WinDivertClose\0")?),
                shutdown: std::mem::transmute::<Proc, ShutdownFn>(sym(b"WinDivertShutdown\0")?),
            }
        };
        if let Ok(mut g) = cache.lock() {
            g.insert(path.to_path_buf(), dll);
        }
        Ok(dll)
    }

    pub fn open_layer(&self, filter: &str, layer: i32, priority: i16, flags: u64, tag: &str) -> Result<HandleGuard, String> {
        let c = format!("{filter}\0");
        let h = unsafe { (self.open)(c.as_ptr(), layer, priority, flags) };
        if h.is_null() || h as isize == -1 {
            return Err(format!("{tag}: ошибка {}", std::io::Error::last_os_error()));
        }
        Ok(HandleGuard { h, close: self.close })
    }

    pub fn open(&self, filter: &str, priority: i16, flags: u64, tag: &str) -> Result<HandleGuard, String> {
        self.open_layer(filter, LAYER_NETWORK, priority, flags, tag)
    }

    pub fn recv(&self, h: &HandleGuard, buf: &mut [u8], addr: &mut Addr) -> Option<usize> {
        let mut n = 0u32;
        let ok = unsafe { (self.recv)(h.h, buf.as_mut_ptr(), buf.len() as u32, &mut n, addr) };
        (ok != 0).then_some(n as usize)
    }

    pub fn send(&self, h: &HandleGuard, pkt: &[u8], addr: &Addr) -> bool {
        let mut n = 0u32;
        unsafe { (self.send)(h.h, pkt.as_ptr(), pkt.len() as u32, &mut n, addr) != 0 }
    }

    pub fn shutdown(&self, h: &HandleGuard) {
        unsafe { (self.shutdown)(h.h, SHUTDOWN_BOTH) };
    }
}

pub fn sum16(data: &[u8], mut acc: u32) -> u32 {
    let (pairs, rest) = data.as_chunks::<2>();
    for c in pairs {
        acc += u16::from_be_bytes(*c) as u32;
    }
    if let [b] = rest {
        acc += (*b as u32) << 8;
    }
    acc
}

pub fn fold(mut acc: u32) -> u16 {
    while acc >> 16 != 0 {
        acc = (acc & 0xffff) + (acc >> 16);
    }
    !(acc as u16)
}

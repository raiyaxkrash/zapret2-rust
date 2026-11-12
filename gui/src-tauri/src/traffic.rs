//! Счётчики трафика сетевых адаптеров для графика на главной.
//! Суммируются только физические адаптеры в рабочем состоянии (без loopback,
//! виртуальных адаптеров Hyper-V, VPN и т. п.), чтобы трафик не считался дважды.

use serde::Serialize;
use windows_sys::Win32::{
    Foundation::NO_ERROR,
    NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2},
};

#[derive(Serialize, Clone, Copy, Debug, Default)]
pub struct Totals {
    /// Принято байт с момента загрузки системы.
    pub rx: u64,
    /// Отправлено байт.
    pub tx: u64,
}

const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
const IF_OPER_STATUS_UP: i32 = 1;

pub fn totals() -> Totals {
    let mut out = Totals::default();
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table) != NO_ERROR || table.is_null() {
            return out;
        }
        let n = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
        for r in rows {
            // Бит 0 — HardwareInterface: физический адаптер.
            let hardware = r.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
            if hardware && r.Type != IF_TYPE_SOFTWARE_LOOPBACK && r.OperStatus == IF_OPER_STATUS_UP {
                out.rx = out.rx.wrapping_add(r.InOctets);
                out.tx = out.tx.wrapping_add(r.OutOctets);
            }
        }
        FreeMibTable(table as *const core::ffi::c_void);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_counters() {
        // На машине с сетью хотя бы один счётчик ненулевой; главное — вызов не падает.
        let t = super::totals();
        println!("rx={} tx={}", t.rx, t.tx);
    }
}

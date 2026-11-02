// System readings for the System Monitor page and the status indicators:
// CPU (GetSystemTimes deltas), RAM (GlobalMemoryStatusEx), GPU (PDH "GPU
// Engine" 3D utilisation), network rates (GetIfTable2 octet deltas), system
// drive use, Wi-Fi SSID (WLAN API) and connected Bluetooth devices. Nothing
// here runs unless the page is open or an indicator poll asks for it.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Devices::Bluetooth::*;
use windows_sys::Win32::Foundation::{FILETIME, HANDLE};
use windows_sys::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows_sys::Win32::NetworkManagement::WiFi::*;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::Performance::*;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::GetSystemTimes;

#[derive(Default, Clone)]
pub struct Reading {
    pub cpu: f64,          // %
    pub ram_used: u64,     // bytes
    pub ram_total: u64,
    pub gpu: Option<f64>,  // %
    pub up: f64,           // bytes / s
    pub down: f64,
    pub disk_used: u64,
    pub disk_total: u64,
}

struct State {
    cpu_prev: Option<(u64, u64)>, // (idle, total)
    net_prev: Option<(u64, u64, u64)>, // (in, out, tick ms)
    pdh: Option<(isize, isize)>, // (query, counter)
    pdh_failed: bool,
    last: Reading,
    wlan: HANDLE,
}

static mut S: State = State { cpu_prev: None, net_prev: None, pdh: None, pdh_failed: false, last: Reading { cpu: 0.0, ram_used: 0, ram_total: 0, gpu: None, up: 0.0, down: 0.0, disk_used: 0, disk_total: 0 }, wlan: null_mut() };

fn st() -> &'static mut State {
    unsafe { &mut *(&raw mut S) }
}

fn ft(f: &FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn gpu_read() -> Option<f64> {
    let s = st();
    if s.pdh_failed {
        return None;
    }
    if s.pdh.is_none() {
        let mut q: isize = 0;
        if PdhOpenQueryW(null(), 0, &mut q) != 0 {
            s.pdh_failed = true;
            return None;
        }
        let mut c: isize = 0;
        if PdhAddEnglishCounterW(q, wide("\\GPU Engine(*engtype_3D)\\Utilization Percentage").as_ptr(), 0, &mut c) != 0 {
            PdhCloseQuery(q);
            s.pdh_failed = true;
            return None;
        }
        PdhCollectQueryData(q);
        s.pdh = Some((q, c));
        return None;
    }
    let (q, c) = s.pdh.unwrap();
    if PdhCollectQueryData(q) != 0 {
        return None;
    }
    let mut size: u32 = 0;
    let mut count: u32 = 0;
    PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, null_mut());
    if size == 0 {
        return Some(0.0);
    }
    let mut buf = vec![0u8; size as usize];
    let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
    if PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, items) != 0 {
        return None;
    }
    let mut sum = 0.0;
    for i in 0..count as usize {
        let it = &*items.add(i);
        if it.FmtValue.CStatus == 0 {
            sum += it.FmtValue.Anonymous.doubleValue;
        }
    }
    Some(sum.clamp(0.0, 100.0))
}

/// One reading; CPU, GPU and network are deltas since the previous call.
pub fn poll() -> Reading {
    unsafe {
        let s = st();
        let mut r = s.last.clone();
        // CPU
        let (mut idle, mut kern, mut user): (FILETIME, FILETIME, FILETIME) = std::mem::zeroed();
        if GetSystemTimes(&mut idle, &mut kern, &mut user) != 0 {
            let (i, t) = (ft(&idle), ft(&kern) + ft(&user));
            if let Some((pi, pt)) = s.cpu_prev {
                if t > pt {
                    r.cpu = (100.0 * (1.0 - (i - pi) as f64 / (t - pt) as f64)).clamp(0.0, 100.0);
                }
            }
            s.cpu_prev = Some((i, t));
        }
        // RAM
        let mut m: MEMORYSTATUSEX = std::mem::zeroed();
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut m) != 0 {
            r.ram_total = m.ullTotalPhys;
            r.ram_used = m.ullTotalPhys - m.ullAvailPhys;
        }
        r.gpu = gpu_read();
        // network: physical-ish interfaces that are up (Ethernet, Wi-Fi)
        let mut table: *mut MIB_IF_TABLE2 = null_mut();
        if GetIfTable2(&mut table) == 0 && !table.is_null() {
            let n = (*table).NumEntries as usize;
            let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
            let (mut rx, mut tx) = (0u64, 0u64);
            for row in rows {
                let hw = (row.InterfaceAndOperStatusFlags._bitfield & 1) != 0;
                if hw && row.OperStatus == 1 && (row.Type == 6 || row.Type == 71) {
                    rx += row.InOctets;
                    tx += row.OutOctets;
                }
            }
            FreeMibTable(table as *const _);
            let now = windows_sys::Win32::System::SystemInformation::GetTickCount64();
            if let Some((pr, pt, pms)) = s.net_prev {
                let dt = (now.saturating_sub(pms)) as f64 / 1000.0;
                if dt > 0.2 {
                    r.down = rx.saturating_sub(pr) as f64 / dt;
                    r.up = tx.saturating_sub(pt) as f64 / dt;
                }
            }
            s.net_prev = Some((rx, tx, now));
        }
        // system drive
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\";
        let (mut free, mut total, mut tfree) = (0u64, 0u64, 0u64);
        if GetDiskFreeSpaceExW(wide(&drive).as_ptr(), &mut free, &mut total, &mut tfree) != 0 {
            r.disk_total = total;
            r.disk_used = total.saturating_sub(tfree);
        }
        s.last = r.clone();
        r
    }
}

pub fn fmt_rate(b: f64) -> String {
    if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", b / 1048576.0)
    } else if b >= 1024.0 {
        format!("{:.0} KB/s", b / 1024.0)
    } else {
        format!("{:.0} B/s", b)
    }
}

/// SSID of the connected Wi-Fi network; Some("") when Wi-Fi exists but is
/// not connected; None when the PC has no Wi-Fi adapter.
pub fn wifi() -> Option<String> {
    unsafe {
        let s = st();
        if s.wlan.is_null() {
            let mut ver = 0u32;
            let mut h: HANDLE = null_mut();
            if WlanOpenHandle(2, null(), &mut ver, &mut h) != 0 {
                return None;
            }
            s.wlan = h;
        }
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
        if WlanEnumInterfaces(s.wlan, null(), &mut list) != 0 || list.is_null() {
            return None;
        }
        let n = (*list).dwNumberOfItems as usize;
        let infos = std::slice::from_raw_parts((*list).InterfaceInfo.as_ptr(), n);
        let mut out = if n == 0 { None } else { Some(String::new()) };
        for info in infos {
            if info.isState != wlan_interface_state_connected {
                continue;
            }
            let mut size = 0u32;
            let mut data: *mut core::ffi::c_void = null_mut();
            if WlanQueryInterface(s.wlan, &info.InterfaceGuid, wlan_intf_opcode_current_connection, null(), &mut size, &mut data, null_mut()) == 0 && !data.is_null() {
                let ca = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
                let ssid = &ca.wlanAssociationAttributes.dot11Ssid;
                let len = (ssid.uSSIDLength as usize).min(32);
                out = Some(String::from_utf8_lossy(&ssid.ucSSID[..len]).to_string());
                WlanFreeMemory(data);
                break;
            }
        }
        WlanFreeMemory(list as *const _);
        out
    }
}

/// Names of connected (classic) Bluetooth devices; None without a radio.
pub fn bluetooth() -> Option<Vec<String>> {
    unsafe {
        let mut rp = BLUETOOTH_FIND_RADIO_PARAMS { dwSize: std::mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32 };
        let mut radio: HANDLE = null_mut();
        let f = BluetoothFindFirstRadio(&mut rp, &mut radio);
        if f.is_null() {
            return None;
        }
        BluetoothFindRadioClose(f);
        windows_sys::Win32::Foundation::CloseHandle(radio);
        let mut sp: BLUETOOTH_DEVICE_SEARCH_PARAMS = std::mem::zeroed();
        sp.dwSize = std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32;
        sp.fReturnConnected = 1;
        let mut di: BLUETOOTH_DEVICE_INFO = std::mem::zeroed();
        di.dwSize = std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
        let mut out = Vec::new();
        let h = BluetoothFindFirstDevice(&sp, &mut di);
        if h.is_null() {
            return Some(out);
        }
        loop {
            if di.fConnected != 0 {
                let end = di.szName.iter().position(|c| *c == 0).unwrap_or(di.szName.len());
                out.push(String::from_utf16_lossy(&di.szName[..end]));
            }
            if BluetoothFindNextDevice(h, &mut di) == 0 {
                break;
            }
        }
        BluetoothFindDeviceClose(h);
        Some(out)
    }
}

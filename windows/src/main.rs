// Isle for Windows — native Rust v0.5.0
// The full app on the spike's skeleton: one top-edge window, one UI thread,
// GDI drawing (double-buffered), real functionality per page:
//   Music    — WinRT GSMTC (media.rs) + LRCLIB synced lyrics split-pane
//   Pomodoro — real countdown, Focus 25 / Short Break 5
//   Water    — 250 ml per Drink against a 1.5 L goal, persisted per day
//   Shelf    — real drag-in (WM_DROPFILES), click to open, persisted
//   Todo     — add (inline edit) / toggle done / edit-delete, persisted
// Data: %LOCALAPPDATA%\Isle\data.txt (store.rs). No web engine anywhere.
// v0.3.0: settings moved to their own window (sidebar + sections), the
// Lyrics page shows only lyrics, the minimized notch becomes a mini player
// with the live lyric line, and every size change of the notch animates.
// v0.5.0: the notch follows the "Nusa Notch" design — a floating pill with
// clock + battery ring (or the live lyric line), opening into a rounded card
// with six pages: Music, Lyrics, Focus timer, Water, Shelf, To-do.

#![windows_subsystem = "windows"]

mod lyrics;
mod media;
mod store;

use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SYSTEMTIME, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE,
};
use windows_sys::Win32::UI::Shell::{
    DefSubclassProc, DragAcceptFiles, DragFinish, DragQueryFileW, SHQueryUserNotificationState,
    SetWindowSubclass, ShellExecuteW, HDROP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use store::Store;

// ---------- helpers ----------
fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}
const fn cr(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
const WHITE: u32 = cr(255, 255, 255);
const DIM: u32 = cr(157, 157, 166);
const FAINT: u32 = cr(99, 99, 108);
const CARD: u32 = cr(28, 28, 31);
const CARD2: u32 = cr(38, 38, 42);
const GREEN: u32 = cr(52, 199, 89);
const RED: u32 = cr(255, 59, 48);
const BLUE: u32 = cr(10, 132, 255);
const CYAN: u32 = cr(50, 173, 230);
const BLACK: u32 = cr(0, 0, 0);

const TIMER_ID: usize = 1;
const TIMER_ANIM: usize = 2; // ~60 fps while the notch is resizing
const TIMER_WAVE: usize = 3; // mini player wave + live lyric line
const WM_MOUSELEAVE_MSG: u32 = 675;
const EDIT_ID: i32 = 1001;

// action codes
const A_PREV_PAGE: i32 = 1;
const A_NEXT_PAGE: i32 = 2;
const A_DOT: i32 = 3;
const A_PLAYPAUSE: i32 = 10;
const A_NEXT_TRACK: i32 = 11;
const A_PREV_TRACK: i32 = 12;
const A_SEEK: i32 = 13;
const A_LYRICS: i32 = 14;
const A_LYRIC_LINE: i32 = 15;
const A_VOL_BAR: i32 = 17;
const A_VOL_MUTE: i32 = 18;
const A_SETTINGS: i32 = 5;
const A_SET_SCALE: i32 = 6;
const A_AUTOSTART: i32 = 7;
const A_SOUNDS: i32 = 8;
const A_HIDE_FS: i32 = 9;
const A_POMO_PLAY: i32 = 20;
const A_POMO_RESET: i32 = 21;
const A_POMO_MODE: i32 = 22;
const A_DRINK: i32 = 30;
const A_WATER_BELL: i32 = 31;
const A_SHELF_OPEN: i32 = 40;
const A_SHELF_REMOVE: i32 = 41;
const A_TODO_ADD: i32 = 50;
const A_TODO_EDIT: i32 = 51;
const A_TODO_TOGGLE: i32 = 52;
const A_TODO_DELETE: i32 = 53;
// settings window
const A_MINI_PLAYER: i32 = 60;
const A_MINI_LYRICS: i32 = 61;
const A_SHELF_CLEAR: i32 = 62;
const A_TODO_CLEAR_DONE: i32 = 63;
const A_OPEN_REPO: i32 = 64;
const A_WATER_RESET: i32 = 65;
const S_CLOSE: i32 = 100;
const S_SECTION: i32 = 101;

const REPO_URL: &str = "https://github.com/Sree-Ram-MS/Isle-Dynamic-Island-Extension";

const A_MONITOR: i32 = 70;
const A_MINIMAL: i32 = 71;
const A_OPEN_MODE: i32 = 72;
const A_PAGE_ICONS: i32 = 73;
const A_ANIM: i32 = 74;
const A_SHOW_LYRICS: i32 = 75;
const A_POMO_FOCUS: i32 = 76;
const A_POMO_SHORT: i32 = 77;
const A_POMO_LONG: i32 = 78;
const A_WATER_GOAL: i32 = 79;
const A_DRINK_ML: i32 = 80;
const A_ACTIVE_FROM: i32 = 81;
const A_ACTIVE_UNTIL: i32 = 82;
const A_EMPTY_SHELF: i32 = 83;
const A_OPEN_LRCLIB: i32 = 84;
const S_MINIMIZE: i32 = 102;

// Settings window (Nusa Settings design), in design units (96-dpi pixels).
const SET_W: i32 = 820;
const SET_H: i32 = 640;
const SIDEBAR_W: i32 = 228;
// (name, icon glyph, tile colour); Water's droplet is drawn by hand.
// About is pinned to the bottom of the sidebar.
const SECTIONS: [(&str, &str, u32); 9] = [
    ("General", "\u{e713}", cr(142, 142, 147)),
    ("Display", "\u{e7f4}", cr(94, 92, 230)),
    ("Appearance", "\u{e790}", cr(58, 58, 60)),
    ("Music and lyrics", "\u{e8d6}", cr(255, 55, 95)),
    ("To-do", "\u{e73e}", cr(48, 209, 88)),
    ("Pomodoro", "\u{e916}", cr(255, 159, 10)),
    ("Water", "", cr(10, 132, 255)),
    ("Shelf", "\u{e7b8}", cr(255, 176, 0)),
    ("About", "\u{e946}", cr(99, 99, 102)),
];

const PAGES: usize = 6; // 0 Music, 1 Lyrics, 2 Focus timer, 3 Water, 4 Shelf, 5 To-do


// The opened notch has the same height on every page.
const EXPANDED_H: i32 = 232;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "Isle";

struct ShelfItem {
    path: String,
    name: String,
    size: String,
}

struct App {
    hwnd: HWND,
    expanded: bool,
    page: usize,
    // pointer over this action (code, param): hover looks in the notch
    hover: (i32, i32),
    seek_bar: (i32, i32),
    // design -> device pixel scale of the paint in progress (for clipping)
    paint_k: (f32, f32),
    // "Charging" popup in the minimized notch
    alert_until: u64,
    last_ac: i8,
    autostart: bool,
    // separate settings window
    settings_hwnd: HWND,
    settings_section: usize,
    set_actions: Vec<(RECT, i32, i32)>,
    painting_settings: bool,
    // notch resize animation (window pixels)
    cur_px: (i32, i32),
    animating: bool,
    anim_from: (i32, i32),
    anim_to: (i32, i32),
    anim_start: u64,
    anim_dur: u64,
    open_level: f32, // 0 = minimized look, 1 = opened notch
    open_from: f32,
    open_to: f32,
    wave_on: bool,
    thumb_gen: u64,
    thumb_bmp: HBITMAP,
    thumb_size: i32,
    tracking: bool,
    fonts: Vec<(String, i32, i32, isize)>,
    actions: Vec<(RECT, i32, i32)>,
    // pomodoro
    pomo_total: u32,
    pomo_left: u32,
    pomo_running: bool,
    pomo_label: String,
    pomo_mode: u8, // 0 focus, 1 short break, 2 long break
    pomo_count: u32, // focus sessions finished (every 4th break is long)
    // water reminders / popups
    last_water_tick: u64,
    popup_until: u64,
    // settings window extras
    settings_hover: i32,
    reset_flash_until: u64,
    // volume
    vol_level: f32,
    vol_muted: bool,
    vol_drag: bool,
    // bottom page dots / icons
    dots_hover: bool,
    // mouse wheel / touchpad swipe between pages
    wheel_acc: i32,
    last_wheel_tick: u64,
    last_page_tick: u64,
    // data
    store: Store,
    shelf_items: Vec<ShelfItem>,
    editing: bool,
    edit_hwnd: HWND,
}

static mut APP: Option<App> = None;
fn app() -> &'static mut App {
    unsafe { (*(&raw mut APP)).as_mut().unwrap() }
}

fn today_string() -> String {
    unsafe {
        let mut t: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut t);
        format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay)
    }
}

fn fmt_mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn file_size_str(path: &str) -> String {
    match std::fs::metadata(path) {
        Ok(m) => {
            let b = m.len();
            if b >= 1_048_576 {
                format!("{:.1} MB", b as f64 / 1_048_576.0)
            } else if b >= 1024 {
                format!("{:.1} KB", b as f64 / 1024.0)
            } else {
                format!("{} B", b)
            }
        }
        Err(_) => String::new(),
    }
}

fn build_shelf_items(st: &Store) -> Vec<ShelfItem> {
    st.shelf
        .iter()
        .map(|p| {
            let name = p.rsplit(['\\', '/']).next().unwrap_or(p.as_str()).to_string();
            ShelfItem { path: p.clone(), size: file_size_str(p), name }
        })
        .collect()
}

// ---------- GDI drawing helpers (UI thread only) ----------
fn get_font(name: &str, size: i32, weight: i32) -> HFONT {
    let a = app();
    for (n, s, w, h) in &a.fonts {
        if n == name && *s == size && *w == weight {
            return *h as HFONT;
        }
    }
    let h = unsafe {
        CreateFontW(
            -size, 0, 0, 0, weight, 0, 0, 0, 0, 0, 0, 5, 0,
            wide(name).as_ptr(),
        )
    };
    a.fonts.push((name.to_string(), size, weight, h as isize));
    h
}

unsafe fn draw_text(hdc: HDC, s: &str, x: i32, y: i32, w: i32, h: i32, font_name: &str, size: i32, weight: i32, color: u32, flags: u32) {
    let font = get_font(font_name, size, weight);
    let old = SelectObject(hdc, font as HGDIOBJ);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, color);
    let mut rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
    let mut ws = wide(s);
    DrawTextW(hdc, ws.as_mut_ptr(), -1, &mut rect, flags | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, old);
}

unsafe fn text_width(s: &str, font_name: &str, size: i32, weight: i32, hdc: HDC) -> i32 {
    let font = get_font(font_name, size, weight);
    let old = SelectObject(hdc, font as HGDIOBJ);
    let mut sz: windows_sys::Win32::Foundation::SIZE = std::mem::zeroed();
    let ws = wide(s);
    GetTextExtentPoint32W(hdc, ws.as_ptr(), (ws.len() - 1) as i32, &mut sz);
    SelectObject(hdc, old);
    sz.cx
}

unsafe fn fill_rect_c(hdc: HDC, x: i32, y: i32, w: i32, h: i32, color: u32) {
    let brush = CreateSolidBrush(color);
    let r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    FillRect(hdc, &r, brush);
    DeleteObject(brush as HGDIOBJ);
}

unsafe fn fill_round(hdc: HDC, x: i32, y: i32, w: i32, h: i32, rad: i32, color: u32) {
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_NULL as i32, 0, 0);
    let ob = SelectObject(hdc, brush as HGDIOBJ);
    let op = SelectObject(hdc, pen as HGDIOBJ);
    RoundRect(hdc, x, y, x + w, y + h, rad, rad);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    DeleteObject(brush as HGDIOBJ);
    DeleteObject(pen as HGDIOBJ);
}

unsafe fn fill_circle(hdc: HDC, cx: i32, cy: i32, r: i32, color: u32) {
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_NULL as i32, 0, 0);
    let ob = SelectObject(hdc, brush as HGDIOBJ);
    let op = SelectObject(hdc, pen as HGDIOBJ);
    Ellipse(hdc, cx - r, cy - r, cx + r, cy + r);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    DeleteObject(brush as HGDIOBJ);
    DeleteObject(pen as HGDIOBJ);
}

unsafe fn draw_arc(hdc: HDC, cx: i32, cy: i32, r: i32, from_deg: f64, sweep_deg: f64, width: i32, color: u32) {
    if sweep_deg <= 0.0 {
        return;
    }
    let pen = CreatePen(PS_SOLID as i32, width, color);
    let op = SelectObject(hdc, pen as HGDIOBJ);
    let steps = (sweep_deg as i32).max(2);
    for i in 0..steps {
        let a1 = (from_deg + sweep_deg * i as f64 / steps as f64).to_radians();
        let a2 = (from_deg + sweep_deg * (i + 1) as f64 / steps as f64).to_radians();
        MoveToEx(hdc, cx + (r as f64 * a1.cos()) as i32, cy + (r as f64 * a1.sin()) as i32, null_mut());
        LineTo(hdc, cx + (r as f64 * a2.cos()) as i32, cy + (r as f64 * a2.sin()) as i32);
    }
    SelectObject(hdc, op);
    DeleteObject(pen as HGDIOBJ);
}

fn add_action(x: i32, y: i32, w: i32, h: i32, code: i32, param: i32) {
    let a = app();
    let r = (RECT { left: x, top: y, right: x + w, bottom: y + h }, code, param);
    if a.painting_settings {
        a.set_actions.push(r);
    } else {
        a.actions.push(r);
    }
}

// ---------- sizing ----------
fn collapsed_size() -> (i32, i32) {
    // Design units (Nusa Notch design): a floating pill.
    match mini_mode() {
        Mini::Alert => (330, 62),
        Mini::Tab => (120, 16),
        Mini::Lyric => (360, 34),
        Mini::Wide => (240, 34),
        Mini::Clock => (176, 34),
    }
}

fn expanded_size() -> (i32, i32) {
    (560, EXPANDED_H)
}

// Size of the notch in percent: the Settings choice, or for Auto the
// Windows display scaling (clamped to the offered range).
fn expanded_scale() -> i32 {
    match app().store.scale {
        0 => unsafe { (GetDpiForSystem() as i32 * 100 / 96).clamp(100, 150) },
        s => s,
    }
}

// Volume slider (design units), left of the speaker button on the controls
// row; it only shows while the pointer is on the speaker or the slider.
fn vol_bar_geom() -> (i32, i32) {
    let w = expanded_size().0;
    let right = w - 48 - 28 - 6;
    (right - 70, 70)
}

// Hover zone of the bottom dots (small) / page icon pills (wider once shown).
fn dots_zone(w: i32, h: i32, hover: bool) -> RECT {
    let (zw, zh) = if hover { (280, 34) } else { (130, 26) };
    RECT { left: (w - zw) / 2, top: h - 8 - zh, right: (w + zw) / 2, bottom: h - 2 }
}

fn cur_design() -> (i32, i32) {
    if app().expanded {
        expanded_size()
    } else {
        collapsed_size()
    }
}

fn px_of(design: (i32, i32), scale: i32) -> (i32, i32) {
    ((design.0 * scale + 50) / 100, (design.1 * scale + 50) / 100)
}

// The window's real pixels for the current state (DPI-aware, so these are
// true device pixels, not Windows' blurry bitmap scaling).
fn cur_size() -> (i32, i32) {
    if app().expanded {
        px_of(expanded_size(), expanded_scale())
    } else {
        px_of(collapsed_size(), expanded_scale())
    }
}

// Corner radius (px) of the minimized look for the current state.
fn collapsed_radius() -> i32 {
    let (_, h) = px_of(collapsed_size(), expanded_scale());
    h / 2
}

// The notch floats just below the top edge, rounded on every corner.
unsafe fn apply_region(hwnd: HWND, w: i32, h: i32, rad: i32) {
    let round = CreateRoundRectRgn(0, 0, w + 1, h + 1, rad * 2, rad * 2);
    SetWindowRgn(hwnd, round, 1);
}

fn expanded_radius() -> i32 {
    34 * expanded_scale() / 100
}

// Gap between the screen's top edge and the notch, in pixels.
fn top_gap() -> i32 {
    8 * expanded_scale() / 100
}

fn lerp_i(a: i32, b: i32, t: f32) -> i32 {
    (a as f32 + (b - a) as f32 * t).round() as i32
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

// Slight overshoot when opening, like a spring settling.
fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.15f32;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

// Displays in enumeration order: (screen rect, is main display).
unsafe extern "system" fn monitor_enum_cb(hmon: HMONITOR, _hdc: HDC, _r: *mut RECT, data: LPARAM) -> i32 {
    let list = &mut *(data as *mut Vec<(RECT, bool)>);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(hmon, &mut mi) != 0 {
        list.push((mi.rcMonitor, mi.dwFlags & 1 != 0));
    }
    1
}

fn monitors() -> Vec<(RECT, bool)> {
    let mut list: Vec<(RECT, bool)> = Vec::new();
    unsafe {
        EnumDisplayMonitors(null_mut(), null(), Some(monitor_enum_cb), &mut list as *mut _ as LPARAM);
    }
    list
}

// Screen rect of the display chosen in Settings > Display (main display by
// default, or when the chosen one is unplugged).
fn notch_monitor() -> RECT {
    let list = monitors();
    let want = app().store.monitor;
    if want >= 0 && (want as usize) < list.len() {
        return list[want as usize].0;
    }
    if let Some((r, _)) = list.iter().find(|(_, main)| *main) {
        return *r;
    }
    unsafe { RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) } }
}

unsafe fn place_window(hwnd: HWND, w: i32, h: i32) {
    let m = notch_monitor();
    SetWindowPos(hwnd, HWND_TOPMOST, m.left + (m.right - m.left - w) / 2, m.top + top_gap(), w, h, SWP_NOACTIVATE);
}

// Move the notch towards the size of its current state. Size changes
// (open, close, mini player appearing) animate instead of jumping.
unsafe fn apply_size(hwnd: HWND) {
    let a = app();
    let target = cur_size();
    let open_to = if a.expanded { 1.0 } else { 0.0 };
    if target == a.cur_px && !a.animating {
        a.open_level = open_to;
        let rad = if a.expanded { expanded_radius() } else { collapsed_radius() };
        apply_region(hwnd, target.0, target.1, rad);
        InvalidateRect(hwnd, null(), 0);
        return;
    }
    a.anim_from = a.cur_px;
    a.anim_to = target;
    a.open_from = a.open_level;
    a.open_to = open_to;
    a.anim_start = GetTickCount64();
    a.anim_dur = if open_to > a.open_from { 260 } else { 190 };
    if a.store.anim_fast {
        a.anim_dur = a.anim_dur * 3 / 5;
    }
    a.animating = true;
    SetTimer(hwnd, TIMER_ANIM, 10, None);
    anim_step(hwnd);
}

unsafe fn anim_step(hwnd: HWND) {
    let a = app();
    if !a.animating {
        KillTimer(hwnd, TIMER_ANIM);
        return;
    }
    let t = ((GetTickCount64().saturating_sub(a.anim_start)) as f32 / a.anim_dur as f32).clamp(0.0, 1.0);
    let opening = a.open_to > a.open_from || (a.anim_to.0 * a.anim_to.1 > a.anim_from.0 * a.anim_from.1);
    let e_size = if opening { ease_out_back(t) } else { ease_out_cubic(t) };
    let e_open = ease_out_cubic(t);
    let w = lerp_i(a.anim_from.0, a.anim_to.0, e_size).max(8);
    let h = lerp_i(a.anim_from.1, a.anim_to.1, e_size).max(4);
    a.open_level = a.open_from + (a.open_to - a.open_from) * e_open;
    let rad = lerp_i(collapsed_radius(), expanded_radius(), a.open_level);
    place_window(hwnd, w, h);
    apply_region(hwnd, w, h, rad);
    a.cur_px = (w, h);
    if t >= 1.0 {
        a.animating = false;
        a.cur_px = a.anim_to;
        a.open_level = a.open_to;
        KillTimer(hwnd, TIMER_ANIM);
        place_window(hwnd, a.cur_px.0, a.cur_px.1);
        let rad = if a.expanded { expanded_radius() } else { collapsed_radius() };
        apply_region(hwnd, a.cur_px.0, a.cur_px.1, rad);
    }
    InvalidateRect(hwnd, null(), 0);
    UpdateWindow(hwnd);
}

// ---------- painting ----------
// Layout follows the "Nusa Notch" design (windows/design/notch-design.html):
// a floating black pill (clock, battery ring, sound wave, timer ring, or the
// live lyric line) that opens into a 560 x 232 rounded card with a header
// (page name in the page colour, clock + date), round chevrons, a page area
// (y 44..198, x padding 48) and page dots with a gear at the bottom.

// time as ("12:19", "PM") and the date as "Sun, Nov 1"
fn time_parts() -> (String, String, String) {
    unsafe {
        let mut t: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut t);
        let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let mons = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let h = if t.wHour % 12 == 0 { 12 } else { t.wHour % 12 };
        (
            format!("{}:{:02}", h, t.wMinute),
            if t.wHour >= 12 { "PM".into() } else { "AM".into() },
            format!("{}, {} {}", days[t.wDayOfWeek as usize], mons[(t.wMonth - 1) as usize], t.wDay),
        )
    }
}

// (percent, plugged in) when the PC has a battery.
fn battery() -> Option<(u8, bool)> {
    unsafe {
        let mut s: SYSTEM_POWER_STATUS = std::mem::zeroed();
        if GetSystemPowerStatus(&mut s) == 0 {
            return None;
        }
        if s.BatteryFlag == 128 || s.BatteryFlag == 255 || s.BatteryLifePercent > 100 {
            return None;
        }
        Some((s.BatteryLifePercent, s.ACLineStatus == 1))
    }
}

fn page_color(p: usize) -> u32 {
    match p {
        0 => cr(255, 55, 95),
        1 => cr(191, 90, 242),
        2 => cr(255, 159, 10),
        3 => cr(10, 132, 255),
        4 => cr(255, 176, 0),
        _ => cr(48, 209, 88),
    }
}

const PAGE_NAMES: [&str; PAGES] = ["Music", "Lyrics", "Focus timer", "Water", "Shelf", "To-do"];
// Page icons shown on the bottom dots while hovering them ("" = droplet).
const PAGE_ICONS: [&str; PAGES] = ["\u{e8d6}", "\u{e8e4}", "\u{e916}", "", "\u{e7b8}", "\u{e73e}"];

// Pages reachable with the chevrons, dots, wheel and swipe (Lyrics only
// when lyrics are turned on in Settings).
fn visible_pages() -> Vec<usize> {
    (0..PAGES).filter(|p| *p != 1 || app().store.show_lyrics).collect()
}

fn mix(a: u32, b: u32, t: f64) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xFF) as f64;
    let l = |s: u32| (ch(a, s) + (ch(b, s) - ch(a, s)) * t.clamp(0.0, 1.0)).round() as u32;
    l(0) | (l(8) << 8) | (l(16) << 16)
}

// Clip the following drawing to a rounded rectangle (design units). GDI
// clip regions are in device pixels, so the current paint scale is used.
unsafe fn clip_round(hdc: HDC, x: i32, y: i32, w: i32, h: i32, rad: i32) {
    let (kx, ky) = app().paint_k;
    let r = CreateRoundRectRgn(
        (x as f32 * kx) as i32,
        (y as f32 * ky) as i32,
        ((x + w) as f32 * kx) as i32 + 1,
        ((y + h) as f32 * ky) as i32 + 1,
        (rad as f32 * 2.0 * kx) as i32,
        (rad as f32 * 2.0 * ky) as i32,
    );
    SelectClipRgn(hdc, r);
    DeleteObject(r as HGDIOBJ);
}

unsafe fn unclip(hdc: HDC) {
    SelectClipRgn(hdc, null_mut());
}

unsafe fn polygon(hdc: HDC, pts: &[POINT], col: u32) {
    let brush = CreateSolidBrush(col);
    let pen = CreatePen(PS_NULL as i32, 0, 0);
    let ob = SelectObject(hdc, brush as HGDIOBJ);
    let op = SelectObject(hdc, pen as HGDIOBJ);
    Polygon(hdc, pts.as_ptr(), pts.len() as i32);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    DeleteObject(brush as HGDIOBJ);
    DeleteObject(pen as HGDIOBJ);
}

// Stroked polyline with round caps (chevrons, check marks).
unsafe fn stroke(hdc: HDC, pts: &[POINT], width: i32, col: u32) {
    let lb = LOGBRUSH { lbStyle: BS_SOLID, lbColor: col, lbHatch: 0 };
    let pen = ExtCreatePen((PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND | PS_JOIN_ROUND) as u32, width as u32, &lb, 0, null());
    let op = SelectObject(hdc, pen as HGDIOBJ);
    Polyline(hdc, pts.as_ptr(), pts.len() as i32);
    SelectObject(hdc, op);
    DeleteObject(pen as HGDIOBJ);
}

unsafe fn draw_droplet(hdc: HDC, dx: i32, dy: i32, col: u32) {
    fill_circle(hdc, dx, dy + 2, 5, col);
    polygon(hdc, &[POINT { x: dx, y: dy - 7 }, POINT { x: dx - 4, y: dy - 1 }, POINT { x: dx + 4, y: dy - 1 }], col);
}

// Ring progress (pomodoro, battery): track + arc from 12 o'clock clockwise.
unsafe fn ring(hdc: HDC, cx: i32, cy: i32, r: i32, width: i32, frac: f64, track: u32, col: u32) {
    draw_arc(hdc, cx, cy, r, 0.0, 360.0, width, track);
    if frac > 0.0 {
        draw_arc(hdc, cx, cy, r, -90.0, 360.0 * frac.min(1.0), width, col);
    }
}

// Battery ring (or a glowing green dot on PCs without a battery).
unsafe fn draw_indicator(hdc: HDC, cx: i32, cy: i32) {
    match battery() {
        Some((pct, plugged)) => {
            let low = pct <= 20 && !plugged;
            ring(hdc, cx, cy, 9, 2, pct as f64 / 100.0, cr(31, 58, 40), if low { cr(255, 69, 58) } else { cr(48, 209, 88) });
            if plugged {
                polygon(hdc, &[
                    POINT { x: cx + 1, y: cy - 6 },
                    POINT { x: cx - 4, y: cy + 1 },
                    POINT { x: cx - 1, y: cy + 1 },
                    POINT { x: cx - 2, y: cy + 6 },
                    POINT { x: cx + 4, y: cy - 1 },
                    POINT { x: cx + 1, y: cy - 1 },
                ], cr(48, 209, 88));
            }
        }
        None => {
            fill_circle(hdc, cx, cy, 6, cr(12, 52, 24));
            fill_circle(hdc, cx, cy, 4, cr(48, 209, 88));
        }
    }
}

// Battery ring + "12:19 PM" (+ date), centred on cx.
unsafe fn draw_clock(hdc: HDC, cx: i32, cy: i32, with_date: bool) {
    let (t, ap, date) = time_parts();
    let tw = text_width(&t, "Segoe UI", 15, 600, hdc);
    let aw = text_width(&ap, "Segoe UI", 11, 600, hdc);
    let dw = if with_date { 10 + text_width(&date, "Segoe UI", 12, 400, hdc) } else { 0 };
    let total = 22 + 9 + tw + 2 + aw + dw;
    let x = cx - total / 2;
    draw_indicator(hdc, x + 11, cy);
    let mut tx = x + 31;
    draw_text(hdc, &t, tx, cy - 11, tw + 4, 22, "Segoe UI", 15, 600, WHITE, DT_LEFT);
    tx += tw + 2;
    draw_text(hdc, &ap, tx, cy - 9, aw + 4, 20, "Segoe UI", 11, 600, cr(161, 161, 166), DT_LEFT);
    if with_date {
        tx += aw + 10;
        draw_text(hdc, &date, tx, cy - 10, dw, 20, "Segoe UI", 12, 400, cr(161, 161, 166), DT_LEFT);
    }
}

// Four red bars bouncing like the design's wave.
unsafe fn draw_wave(hdc: HDC, x: i32, cy: i32) {
    let t = GetTickCount64() as f64 / 1000.0;
    let periods = [0.9, 0.7, 1.1, 0.8];
    let offs = [0.0, 0.3, 0.6, 0.15];
    for i in 0..4 {
        let ph = ((t + offs[i]) / periods[i]) * std::f64::consts::TAU;
        let k = 0.5 - 0.5 * ph.cos();
        let bh = (4.0 + 11.0 * k) as i32;
        fill_round(hdc, x + i as i32 * 5, cy - bh / 2, 3, bh, 3, cr(255, 55, 95));
    }
}

fn pomo_frac() -> f64 {
    let a = app();
    if a.pomo_total == 0 { 0.0 } else { a.pomo_left as f64 / a.pomo_total as f64 }
}

unsafe fn paint(hdc: HDC, w: i32, h: i32, expanded: bool) {
    fill_rect_c(hdc, 0, 0, w, h, BLACK);
    app().actions.clear();
    if !expanded {
        paint_collapsed(hdc, w, h);
        return;
    }
    let a = app();
    let page = a.page;
    // header: page name / clock + date
    draw_text(hdc, PAGE_NAMES[page], 24, 0, 160, 44, "Segoe UI", 14, 700, page_color(page), DT_LEFT);
    draw_clock(hdc, w / 2, 22, true);
    // page
    match page {
        0 => paint_music(hdc, w, h),
        1 => paint_lyrics(hdc, w, h),
        2 => paint_pomo(hdc, w, h),
        3 => paint_water(hdc, w, h),
        4 => paint_shelf(hdc, w, h),
        _ => paint_todo(hdc, w, h),
    }
    // round chevrons
    let cy = h / 2 + 6;
    for (is_prev, cx) in [(true, 24), (false, w - 24)] {
        let hov = a.hover.0 == if is_prev { A_PREV_PAGE } else { A_NEXT_PAGE };
        fill_circle(hdc, cx, cy, 14, if hov { cr(44, 44, 46) } else { cr(28, 28, 30) });
        let d = if is_prev { 1 } else { -1 };
        stroke(hdc, &[POINT { x: cx + 2 * d, y: cy - 5 }, POINT { x: cx - 3 * d, y: cy }, POINT { x: cx + 2 * d, y: cy + 5 }], 2, WHITE);
        add_action(cx - 15, cy - 15, 30, 30, if is_prev { A_PREV_PAGE } else { A_NEXT_PAGE }, 0);
    }
    paint_dots(hdc, w, h);
}

// Bottom page dots (active one long, in the page colour) and the gear.
// Hovering them turns each dot into a small pill with the page icon.
unsafe fn paint_dots(hdc: HDC, w: i32, h: i32) {
    let a = app();
    let pages = visible_pages();
    let big = a.dots_hover;
    let cy = h - 20;
    let dot_w = |p: usize| if big { 32 } else if p == a.page { 18 } else { 7 };
    let total: i32 = pages.iter().map(|p| dot_w(*p)).sum::<i32>() + 8 * (pages.len() as i32 - 1) + 6 + 24;
    let mut x = (w - total) / 2;
    for p in &pages {
        let p = *p;
        let on = p == a.page;
        let dw = dot_w(p);
        if big {
            fill_round(hdc, x, cy - 12, dw, 24, 24, if on { page_color(p) } else { cr(28, 28, 30) });
            if PAGE_ICONS[p].is_empty() {
                draw_droplet(hdc, x + dw / 2, cy + 1, WHITE);
            } else {
                draw_text(hdc, PAGE_ICONS[p], x, cy - 12, dw, 24, "Segoe MDL2 Assets", 12, 400, WHITE, DT_CENTER);
            }
            add_action(x, cy - 12, dw, 24, A_DOT, p as i32);
        } else {
            fill_round(hdc, x, cy - 3, dw, 7, 7, if on { page_color(p) } else { cr(72, 72, 74) });
            add_action(x - 3, cy - 10, dw + 6, 20, A_DOT, p as i32);
        }
        x += dw + 8;
    }
    // gear: opens the settings window
    x += 6 - 8;
    let gx = x + 12;
    let hov = a.hover.0 == A_SETTINGS || !a.settings_hwnd.is_null();
    fill_circle(hdc, gx, cy, 12, if hov { cr(44, 44, 46) } else { cr(28, 28, 30) });
    draw_text(hdc, "\u{e713}", gx - 12, cy - 12, 24, 24, "Segoe MDL2 Assets", 12, 400, cr(209, 209, 214), DT_CENTER);
    add_action(gx - 13, cy - 13, 26, 26, A_SETTINGS, 0);
}

// ---------- minimized pill ----------
// What the minimized notch shows right now.
#[derive(PartialEq, Clone, Copy)]
enum Mini {
    Alert,
    Tab,
    Lyric,
    Wide, // clock with the wave and/or the timer ring
    Clock,
}

fn music_playing() -> bool {
    let m = media::snapshot();
    m.present && m.playing
}

fn mini_lyric_line() -> Option<String> {
    let a = app();
    if !a.store.show_lyrics || !music_playing() {
        return None;
    }
    // closed on the Lyrics page, or "Lyrics in the minimized notch" is on
    if a.page != 1 && !(a.store.mini_player && a.store.mini_lyrics) {
        return None;
    }
    live_lyric(&media::snapshot())
}

fn mini_mode() -> Mini {
    let a = app();
    if a.alert_until > 0 {
        return Mini::Alert;
    }
    let playing = a.store.mini_player && music_playing();
    if mini_lyric_line().is_some() {
        return Mini::Lyric;
    }
    if playing || a.pomo_running {
        return Mini::Wide;
    }
    if a.store.tiny_tab {
        return Mini::Tab;
    }
    Mini::Clock
}

fn mini_active() -> bool {
    matches!(mini_mode(), Mini::Lyric | Mini::Wide)
}

unsafe fn paint_collapsed(hdc: HDC, w: i32, h: i32) {
    let a = app();
    let cy = h / 2;
    match mini_mode() {
        Mini::Alert => {
            // "Charging" popup
            let pct = battery().map(|b| b.0).unwrap_or(0);
            fill_circle(hdc, 38, cy, 18, cr(28, 28, 30));
            polygon(hdc, &[
                POINT { x: 39, y: cy - 9 },
                POINT { x: 31, y: cy + 2 },
                POINT { x: 37, y: cy + 2 },
                POINT { x: 36, y: cy + 9 },
                POINT { x: 45, y: cy - 2 },
                POINT { x: 39, y: cy - 2 },
            ], cr(48, 209, 88));
            draw_text(hdc, "Charging", 68, cy - 19, 160, 20, "Segoe UI", 14, 600, WHITE, DT_LEFT);
            draw_text(hdc, "Charger connected", 68, cy + 1, 160, 18, "Segoe UI", 12, 400, cr(161, 161, 166), DT_LEFT);
            draw_text(hdc, &format!("{}%", pct), w - 90, 0, 70, h, "Segoe UI", 14, 600, cr(48, 209, 88), DT_RIGHT);
        }
        Mini::Tab => {
            fill_circle(hdc, w / 2, cy, 3, cr(48, 209, 88));
        }
        mode => {
            if a.pomo_running {
                ring(hdc, 24, cy, 8, 3, pomo_frac(), cr(58, 42, 16), cr(255, 159, 10));
            }
            if a.store.mini_player && music_playing() {
                draw_wave(hdc, w - 14 - 18, cy);
            }
            if mode == Mini::Lyric {
                let line = mini_lyric_line().unwrap_or_default();
                let text = if line == "\u{266a}" { line } else { format!("\u{266a}  {}", line) };
                draw_text(hdc, &text, (w - 240) / 2, 0, 240, h, "Segoe UI", 13, 500, WHITE, DT_CENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
            } else {
                draw_clock(hdc, w / 2, cy, false);
            }
        }
    }
}

// The current synced lyric line, if there is one for the playing track.
fn live_lyric(m: &media::MediaInfo) -> Option<String> {
    if let lyrics::LyricsState::Ready(lines) = lyrics::state() {
        if lyrics::info() == "unsynced lyrics" || lines.is_empty() {
            return None;
        }
        let pos = media::live_position(m);
        // before the first line: nothing to show yet
        if pos + 0.2 < lines[0].0 {
            return Some("\u{266a}".into());
        }
        let line = lines[lyrics::current_line(&lines, pos)].1.trim().to_string();
        return Some(if line.is_empty() { "\u{266a}".into() } else { line });
    }
    None
}

// GDI bitmap for the current album art, rebuilt only when the decoded
// thumbnail changes. Returns (bitmap, pixel size) or None when there's no art.
unsafe fn thumb_bitmap(hdc: HDC) -> Option<(HBITMAP, i32)> {
    let a = app();
    let t = media::THUMB.lock().ok()?;
    if t.gen != a.thumb_gen {
        if !a.thumb_bmp.is_null() {
            DeleteObject(a.thumb_bmp as HGDIOBJ);
            a.thumb_bmp = null_mut();
        }
        a.thumb_gen = t.gen;
        a.thumb_size = t.size;
        let need = (t.size.max(0) as usize) * (t.size.max(0) as usize) * 4;
        if t.size > 0 && t.bgra.len() >= need {
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = t.size;
            bi.bmiHeader.biHeight = -t.size; // top-down
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut core::ffi::c_void = null_mut();
            let bmp = CreateDIBSection(hdc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if !bmp.is_null() && !bits.is_null() {
                std::ptr::copy_nonoverlapping(t.bgra.as_ptr(), bits as *mut u8, need);
                a.thumb_bmp = bmp;
            } else if !bmp.is_null() {
                DeleteObject(bmp as HGDIOBJ);
            }
        }
    }
    if a.thumb_bmp.is_null() {
        None
    } else {
        Some((a.thumb_bmp, a.thumb_size))
    }
}

// Album art with rounded corners: the real thumbnail when the media session
// has one, otherwise the design's peach -> lilac -> sky gradient with a note.
unsafe fn draw_art(hdc: HDC, x: i32, y: i32, size: i32, rad: i32, note: bool) {
    clip_round(hdc, x, y, size, size, rad);
    if let Some((bmp, bsize)) = thumb_bitmap(hdc) {
        let src = CreateCompatibleDC(hdc);
        let old = SelectObject(src, bmp as HGDIOBJ);
        SetStretchBltMode(hdc, HALFTONE);
        SetBrushOrgEx(hdc, 0, 0, null_mut());
        StretchBlt(hdc, x, y, size, size, src, 0, 0, bsize, bsize, SRCCOPY);
        SelectObject(src, old);
        DeleteDC(src);
        unclip(hdc);
        return;
    }
    // 135deg gradient as diagonal bands
    let (c1, c2, c3) = (cr(255, 154, 139), cr(161, 140, 209), cr(90, 200, 250));
    let n = 28;
    for i in 0..n {
        let t = (i as f64 + 0.5) / n as f64;
        let col = if t < 0.6 { mix(c1, c2, t / 0.6) } else { mix(c2, c3, (t - 0.6) / 0.4) };
        let d0 = 2 * size * i / n;
        let d1 = 2 * size * (i + 1) / n + 1;
        polygon(hdc, &[
            POINT { x: x + d0, y },
            POINT { x: x + d1, y },
            POINT { x, y: y + d1 },
            POINT { x, y: y + d0 },
        ], col);
    }
    unclip(hdc);
    if note {
        draw_text(hdc, "\u{e8d6}", x, y, size, size, "Segoe MDL2 Assets", size * 2 / 5, 400, cr(240, 240, 245), DT_CENTER);
    }
}

// ---------- pages ----------
// Page area: y 50..192 (centre 121), x 48..w-48.
const PY0: i32 = 50;
const PMID: i32 = 121;

unsafe fn pill_btn(hdc: HDC, x: i32, y: i32, label: &str, size: i32, weight: i32, pad: i32, h: i32, bg: u32, fg: u32, code: i32, param: i32) -> i32 {
    let tw = text_width(label, "Segoe UI", size, weight, hdc);
    let bw = tw + pad * 2;
    let hov = app().hover == (code, param);
    let bg = if hov { mix(bg, WHITE, 0.12) } else { bg };
    fill_round(hdc, x, y, bw, h, h, bg);
    draw_text(hdc, label, x, y, bw, h, "Segoe UI", size, weight, fg, DT_CENTER);
    add_action(x, y, bw, h, code, param);
    bw
}

unsafe fn paint_music(hdc: HDC, w: i32, _h: i32) {
    let a = app();
    let m = media::snapshot();
    draw_art(hdc, 48, PMID - 48, 96, 14, true);
    let cx0 = 48 + 96 + 18;
    let cw = w - 48 - cx0;
    let title = if m.present && !m.title.is_empty() { m.title.clone() } else if m.present { "Playing".into() } else { "Not Playing".into() };
    draw_text(hdc, &title, cx0, 70, cw, 21, "Segoe UI", 16, 600, WHITE, DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
    let sub = if m.present { m.artist.clone() } else { "Play something in any app".into() };
    draw_text(hdc, &sub, cx0, 91, cw, 17, "Segoe UI", 12, 400, cr(161, 161, 166), DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
    // progress
    let pos = media::live_position(&m);
    let frac = if m.duration_secs > 0.0 { (pos / m.duration_secs).clamp(0.0, 1.0) } else { 0.0 };
    let py = 123;
    draw_text(hdc, &fmt_mmss(pos), cx0, py - 8, 36, 16, "Segoe UI", 11, 400, cr(161, 161, 166), DT_LEFT);
    draw_text(hdc, &format!("\u{2212}{}", fmt_mmss(m.duration_secs - pos)), w - 48 - 40, py - 8, 40, 16, "Segoe UI", 11, 400, cr(161, 161, 166), DT_RIGHT);
    let (bx, bw) = (cx0 + 40, cw - 84);
    a.seek_bar = (bx, bw);
    fill_round(hdc, bx, py - 2, bw, 4, 4, cr(58, 58, 60));
    if frac > 0.0 {
        fill_round(hdc, bx, py - 2, ((bw as f64 * frac) as i32).max(4), 4, 4, WHITE);
    }
    if m.present && m.duration_secs > 0.0 {
        add_action(bx, py - 8, bw, 16, A_SEEK, 0);
    }
    // controls, centred in the column
    let cy = 147;
    let mid = cx0 + cw / 2;
    let col = if m.present { WHITE } else { cr(99, 99, 108) };
    let btn = |hdc: HDC, cx: i32, glyph: &str, size: i32, box_: i32, code: i32| {
        if app().hover.0 == code && m.present {
            fill_circle(hdc, cx, cy, box_ / 2, cr(44, 44, 46));
        }
        draw_text(hdc, glyph, cx - box_ / 2, cy - box_ / 2, box_, box_, "Segoe MDL2 Assets", size, 400, col, DT_CENTER);
        if m.present {
            add_action(cx - box_ / 2, cy - box_ / 2, box_, box_, code, 0);
        }
    };
    btn(hdc, mid - 57, "\u{e892}", 15, 28, A_PREV_TRACK);
    btn(hdc, mid, if m.playing { "\u{e769}" } else { "\u{e768}" }, 20, 34, A_PLAYPAUSE);
    btn(hdc, mid + 57, "\u{e893}", 15, 28, A_NEXT_TRACK);
    // volume: speaker button on the right; the slider slides out on hover
    let (lvl, muted) = (a.vol_level, a.vol_muted);
    let silent = muted || lvl <= 0.0;
    let vx = w - 48 - 14;
    let show_slider = a.vol_drag || a.hover.0 == A_VOL_MUTE || a.hover.0 == A_VOL_BAR;
    if a.hover.0 == A_VOL_MUTE {
        fill_circle(hdc, vx, cy, 14, cr(44, 44, 46));
    }
    draw_text(hdc, if silent { "\u{e74f}" } else { "\u{e767}" }, vx - 14, cy - 14, 28, 28, "Segoe MDL2 Assets", 15, 400, WHITE, DT_CENTER);
    add_action(vx - 14, cy - 14, 28, 28, A_VOL_MUTE, 0);
    let (sx, sw) = vol_bar_geom();
    if show_slider {
        fill_rect_c(hdc, sx - 8, cy - 14, sw + 8, 28, BLACK); // over the next button
        fill_round(hdc, sx, cy - 2, sw, 4, 4, cr(58, 58, 60));
        let fw = if silent { 0 } else { (sw as f32 * lvl) as i32 };
        if fw > 0 {
            fill_round(hdc, sx, cy - 2, fw.max(4), 4, 4, WHITE);
        }
        fill_circle(hdc, sx + fw.clamp(6, sw - 6), cy, 6, WHITE);
        add_action(sx - 4, cy - 14, sw + 6, 28, A_VOL_BAR, 0);
    }
}

// Lyrics page: the art on the left, the synced lines on the right with the
// current one white and a touch larger. Click a line to jump there.
unsafe fn paint_lyrics(hdc: HDC, w: i32, _h: i32) {
    let m = media::snapshot();
    draw_art(hdc, 48, PMID - 42, 84, 14, true);
    let lx = 48 + 84 + 18;
    let lw = w - 48 - lx;
    let grey = cr(99, 99, 102);
    if !m.present {
        draw_text(hdc, "Nothing is playing", lx, PMID - 12, lw, 24, "Segoe UI", 15, 600, grey, DT_LEFT);
        return;
    }
    match lyrics::state() {
        lyrics::LyricsState::Loading | lyrics::LyricsState::Idle => {
            draw_text(hdc, "Loading lyrics\u{2026}", lx, PMID - 12, lw, 24, "Segoe UI", 15, 600, grey, DT_LEFT);
        }
        lyrics::LyricsState::NotAvailable => {
            draw_text(hdc, "Lyrics not available", lx, PMID - 22, lw, 24, "Segoe UI", 15, 600, WHITE, DT_LEFT);
            let why = lyrics::info();
            if !why.is_empty() {
                draw_text(hdc, &why, lx, PMID + 4, lw, 18, "Segoe UI", 11, 400, cr(142, 142, 147), DT_LEFT | DT_END_ELLIPSIS);
            }
        }
        lyrics::LyricsState::Ready(lines) => {
            let unsynced = lyrics::info() == "unsynced lyrics";
            let cur = if unsynced { 0 } else { lyrics::current_line(&lines, media::live_position(&m)) };
            let pitch = 26;
            for slot in -2i32..=2 {
                let idx = cur as i32 + slot;
                if idx < 0 || idx as usize >= lines.len() {
                    continue;
                }
                let (t, line) = &lines[idx as usize];
                let on = slot == 0 && !unsynced;
                let y = PMID - pitch / 2 + slot * pitch;
                let text = if line.trim().is_empty() { "\u{266a}" } else { line.as_str() };
                let hov = app().hover == (A_LYRIC_LINE, (*t * 10.0) as i32);
                let col = if on { WHITE } else if hov { cr(150, 150, 155) } else { grey };
                draw_text(hdc, text, lx, y, lw, pitch, "Segoe UI", if on { 16 } else { 15 }, if on { 700 } else { 600 }, col, DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
                if !unsynced {
                    add_action(lx, y, lw, pitch, A_LYRIC_LINE, (*t * 10.0) as i32);
                }
            }
        }
    }
}

unsafe fn paint_pomo(hdc: HDC, w: i32, _h: i32) {
    let a = app();
    let (rcx, rcy) = (48 + 52, PMID);
    ring(hdc, rcx, rcy, 46, 7, pomo_frac(), cr(44, 44, 46), cr(255, 159, 10));
    draw_text(hdc, &fmt_mmss(a.pomo_left as f64), rcx - 50, rcy - 16, 100, 32, "Segoe UI", 24, 600, WHITE, DT_CENTER);
    let cx0 = 48 + 104 + 18;
    let cw = w - 48 - cx0;
    draw_text(hdc, "Pomodoro", cx0, 66, cw, 21, "Segoe UI", 16, 600, WHITE, DT_LEFT);
    let label = a.pomo_label.clone();
    draw_text(hdc, &label, cx0, 87, cw, 17, "Segoe UI", 12, 400, cr(161, 161, 166), DT_LEFT);
    // phase pills
    let mut x = cx0;
    for (name, param, on) in [("Short Break", 5, a.pomo_mode != 0), ("Focus", 25, a.pomo_mode == 0)] {
        let (bg, fg) = if on { (WHITE, BLACK) } else { (cr(44, 44, 46), WHITE) };
        x += pill_btn(hdc, x, 112, name, 12, 500, 12, 26, bg, fg, A_POMO_MODE, param) + 6;
    }
    // start / pause (+ reset once the timer has moved)
    let bw = pill_btn(hdc, cx0, 146, if a.pomo_running { "Pause" } else { "Start" }, 13, 600, 16, 29, BLUE, WHITE, A_POMO_PLAY, 0);
    if a.pomo_left != a.pomo_total {
        pill_btn(hdc, cx0 + bw + 6, 146, "Reset", 13, 600, 16, 29, cr(44, 44, 46), WHITE, A_POMO_RESET, 0);
    }
}

unsafe fn paint_water(hdc: HDC, w: i32, _h: i32) {
    let a = app();
    let ml = a.store.water_ml;
    let goal = a.store.water_goal.max(1);
    let pct = ((ml as f64 / goal as f64) * 100.0).round().min(100.0) as i32;
    let x0 = 48;
    draw_text(hdc, "Water Reminder", x0, 66, 200, 17, "Segoe UI", 12, 400, cr(161, 161, 166), DT_LEFT);
    let litres = |v: i32| {
        let s = format!("{:.2}", v as f64 / 1000.0);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let amt = litres(ml);
    let aw = text_width(&amt, "Segoe UI", 30, 700, hdc);
    draw_text(hdc, &amt, x0, 83, aw + 4, 40, "Segoe UI", 30, 700, WHITE, DT_LEFT);
    draw_text(hdc, &format!("/ {} L", litres(goal)), x0 + aw + 7, 92, 100, 26, "Segoe UI", 15, 500, cr(161, 161, 166), DT_LEFT);
    // "Today · 60% · Keep going"
    let p1 = "Today \u{00b7} ";
    let p2 = format!("{}%", pct);
    let p3 = if ml >= goal { " \u{00b7} Goal reached" } else { " \u{00b7} Keep going" };
    let w1 = text_width(p1, "Segoe UI", 12, 400, hdc);
    let w2 = text_width(&p2, "Segoe UI", 12, 700, hdc);
    draw_text(hdc, p1, x0, 123, w1 + 2, 17, "Segoe UI", 12, 400, cr(161, 161, 166), DT_LEFT);
    draw_text(hdc, &p2, x0 + w1, 123, w2 + 2, 17, "Segoe UI", 12, 700, WHITE, DT_LEFT);
    draw_text(hdc, p3, x0 + w1 + w2, 123, 200 - w1 - w2, 17, "Segoe UI", 12, 400, cr(161, 161, 166), DT_LEFT);
    pill_btn(hdc, x0, 148, "Drink", 13, 600, 16, 29, BLUE, WHITE, A_DRINK, 0);
    // drinks by hour
    let bx0 = x0 + 200 + 18;
    let (by0, bh) = (PMID - 42, 84);
    fill_rect_c(hdc, bx0, by0, 1, bh, cr(58, 58, 60));
    let log: Vec<(i32, i32)> = {
        let l = &a.store.water_log;
        l[l.len().saturating_sub(6)..].to_vec()
    };
    if log.is_empty() {
        draw_text(hdc, "No drinks yet today", bx0 + 7, by0, w - 48 - bx0 - 7, bh, "Segoe UI", 12, 400, cr(142, 142, 147), DT_CENTER);
        return;
    }
    let maxv = log.iter().map(|e| e.1).max().unwrap_or(1).max(500) as f64;
    let area_x = bx0 + 7;
    let area_w = w - 48 - area_x - 18; // room for the "ml" label
    let n = log.len() as i32;
    let gap = 10;
    let colw = ((area_w - gap * (n - 1)) / n).min(40);
    for (i, (hr, v)) in log.iter().enumerate() {
        let x = area_x + i as i32 * (colw + gap);
        let hpx = ((*v as f64 / maxv) * 60.0).max(3.0) as i32;
        fill_round(hdc, x, by0 + bh - 16 - hpx, colw, hpx, 8, BLUE);
        draw_text(hdc, &format!("{:02}", hr), x, by0 + bh - 14, colw, 14, "Segoe UI", 10, 400, cr(142, 142, 147), DT_CENTER);
    }
    draw_text(hdc, "ml", w - 48 - 16, by0 + bh - 14, 16, 14, "Segoe UI", 10, 400, cr(142, 142, 147), DT_RIGHT);
}

unsafe fn paint_shelf(hdc: HDC, w: i32, h: i32) {
    let a = app();
    let gap = 18;
    let colw = (w - 96 - gap) / 2;
    let fx = 48;
    // files column
    let n = a.shelf_items.len().min(3) as i32;
    let rows_h = if n == 0 { 30 } else { n * 36 + (n - 1) * 6 };
    let more = a.shelf_items.len() as i32 - n;
    let total = 18 + 6 + rows_h + if more > 0 { 18 } else { 0 };
    let mut y = PMID - total / 2;
    draw_text(hdc, "Shelf", fx, y, 80, 18, "Segoe UI", 12, 400, cr(161, 161, 166), DT_LEFT);
    if !a.shelf_items.is_empty() {
        let tw = text_width("Clear", "Segoe UI", 11, 600, hdc) + 20;
        pill_btn(hdc, fx + colw - tw, y, "Clear", 11, 600, 10, 18, cr(44, 44, 46), WHITE, A_SHELF_CLEAR, 0);
    }
    y += 24;
    if n == 0 {
        draw_text(hdc, "Nothing here yet", fx, y, colw, 30, "Segoe UI", 12, 400, cr(99, 99, 108), DT_LEFT);
    }
    for (i, item) in a.shelf_items.iter().take(3).enumerate() {
        fill_round(hdc, fx, y, colw, 36, 20, cr(28, 28, 30));
        // icon tile: folder amber, file green -> blue
        let is_dir = std::fs::metadata(&item.path).map(|m| m.is_dir()).unwrap_or(false);
        if is_dir {
            fill_round(hdc, fx + 10, y + 6, 24, 24, 12, cr(255, 176, 0));
        } else {
            clip_round(hdc, fx + 10, y + 6, 24, 24, 6);
            for k in 0..12 {
                let col = mix(cr(52, 199, 89), BLUE, k as f64 / 11.0);
                fill_rect_c(hdc, fx + 10 + k * 2, y + 6, 3, 24, col);
            }
            unclip(hdc);
        }
        let hov = a.hover.1 == i as i32 && (a.hover.0 == A_SHELF_OPEN || a.hover.0 == A_SHELF_REMOVE);
        draw_text(hdc, &item.name.clone(), fx + 44, y, colw - 44 - 64, 36, "Segoe UI", 13, 400, WHITE, DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
        add_action(fx, y, colw - 30, 36, A_SHELF_OPEN, i as i32);
        if hov {
            draw_text(hdc, "\u{2715}", fx + colw - 30, y, 20, 36, "Segoe UI", 11, 400, cr(255, 107, 97), DT_CENTER);
            add_action(fx + colw - 32, y, 30, 36, A_SHELF_REMOVE, i as i32);
        } else {
            draw_text(hdc, &item.size.clone(), fx + colw - 70, y, 60, 36, "Segoe UI", 11, 400, cr(142, 142, 147), DT_RIGHT);
        }
        y += 42;
    }
    if more > 0 {
        draw_text(hdc, &format!("+{} more", more), fx, y - 4, colw, 18, "Segoe UI", 11, 400, cr(99, 99, 108), DT_LEFT);
    }
    // drop zone
    let dx = fx + colw + gap;
    let (dy, dh) = (PY0, h - 34 - 6 - PY0);
    let lb = LOGBRUSH { lbStyle: BS_SOLID, lbColor: cr(72, 72, 74), lbHatch: 0 };
    let pen = ExtCreatePen((PS_GEOMETRIC | PS_DASH) as u32, 2, &lb, 0, null());
    let op = SelectObject(hdc, pen as HGDIOBJ);
    let ob = SelectObject(hdc, GetStockObject(NULL_BRUSH as i32));
    RoundRect(hdc, dx, dy, dx + colw, dy + dh, 28, 28);
    SelectObject(hdc, op);
    SelectObject(hdc, ob);
    DeleteObject(pen as HGDIOBJ);
    let my = dy + dh / 2;
    draw_text(hdc, "Drop files here", dx, my - 24, colw, 20, "Segoe UI", 13, 600, WHITE, DT_CENTER);
    draw_text(hdc, "Release to add", dx, my - 3, colw, 16, "Segoe UI", 11, 400, cr(142, 142, 147), DT_CENTER);
    draw_text(hdc, "Drag out anytime \u{00b7} click to open", dx, my + 13, colw, 16, "Segoe UI", 11, 400, cr(142, 142, 147), DT_CENTER);
}

const TAG_COLORS: [u32; 5] = [cr(255, 69, 58), cr(10, 132, 255), cr(255, 159, 10), cr(191, 90, 242), cr(48, 209, 88)];

unsafe fn paint_todo(hdc: HDC, w: i32, _h: i32) {
    let a = app();
    let aside_w = 84;
    let lx = 48;
    let lw = w - 48 - aside_w - 14 - 14;
    let editing_box = !a.edit_hwnd.is_null() && IsWindowVisible(a.edit_hwnd) != 0;
    let max_rows = if editing_box { 2 } else { 3 };
    let n = a.store.tasks.len().min(max_rows) as i32;
    let more = a.store.tasks.len() as i32 - n;
    let rows_h = n.max(1) * 34 + if editing_box { 34 } else { 0 } + if more > 0 { 16 } else { 0 };
    let mut y = PMID - (22 + rows_h) / 2;
    draw_text(hdc, "To-do", lx, y, lw, 20, "Segoe UI", 15, 700, WHITE, DT_LEFT);
    y += 24;
    if a.store.tasks.is_empty() && !editing_box {
        draw_text(hdc, "No tasks yet", lx, y, lw, 29, "Segoe UI", 13, 400, cr(99, 99, 108), DT_LEFT);
    }
    for (i, t) in a.store.tasks.iter().take(max_rows).enumerate() {
        fill_round(hdc, lx, y, lw, 29, 18, cr(28, 28, 30));
        // round check box
        let (ccx, ccy) = (lx + 18, y + 15);
        if t.done {
            fill_circle(hdc, ccx, ccy, 8, cr(48, 209, 88));
            fill_circle(hdc, ccx, ccy, 6, cr(28, 28, 30));
            fill_circle(hdc, ccx, ccy, 4, cr(48, 209, 88));
        } else {
            fill_circle(hdc, ccx, ccy, 8, cr(99, 99, 102));
            fill_circle(hdc, ccx, ccy, 6, cr(28, 28, 30));
        }
        let col = if t.done { cr(142, 142, 147) } else { WHITE };
        let title = t.title.clone();
        let tw = text_width(&title, "Segoe UI", 13, 400, hdc).min(lw - 70);
        draw_text(hdc, &title, lx + 36, y, lw - 70, 29, "Segoe UI", 13, 400, col, DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
        if t.done {
            fill_rect_c(hdc, lx + 36, y + 15, tw, 1, col);
        }
        fill_round(hdc, lx + lw - 13, y + 8, 3, 14, 3, TAG_COLORS[i % TAG_COLORS.len()]);
        add_action(lx, y, lw - 30, 29, A_TODO_TOGGLE, i as i32);
        let hov = a.hover.1 == i as i32 && (a.hover.0 == A_TODO_TOGGLE || a.hover.0 == A_TODO_DELETE);
        if hov {
            draw_text(hdc, "\u{2715}", lx + lw - 36, y, 18, 29, "Segoe UI", 11, 400, cr(255, 107, 97), DT_CENTER);
        }
        add_action(lx + lw - 40, y, 26, 29, A_TODO_DELETE, i as i32);
        y += 34;
    }
    if more > 0 {
        draw_text(hdc, &format!("+{} more", more), lx, y - 2, lw, 16, "Segoe UI", 11, 400, cr(99, 99, 108), DT_LEFT);
    }
    // aside: done count + Add task
    let ax = w - 48 - aside_w;
    fill_rect_c(hdc, ax - 14, PMID - 50, 1, 100, cr(58, 58, 60));
    let done = a.store.tasks.iter().filter(|t| t.done).count();
    let count = format!("{}/{}", done, a.store.tasks.len());
    draw_text(hdc, &count, ax, PMID - 46, aside_w, 34, "Segoe UI", 26, 700, WHITE, DT_CENTER);
    draw_text(hdc, "Done today", ax, PMID - 10, aside_w, 15, "Segoe UI", 11, 400, cr(161, 161, 166), DT_CENTER);
    let bw = text_width("Add task", "Segoe UI", 12, 600, hdc) + 20;
    pill_btn(hdc, ax + (aside_w - bw) / 2, PMID + 14, "Add task", 12, 600, 10, 24, BLUE, WHITE, A_TODO_ADD, 0);
}

// ---------- settings ----------
fn autostart_enabled() -> bool {
    unsafe {
        let mut key: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return false;
        }
        let r = RegQueryValueExW(key, wide(RUN_VALUE).as_ptr(), null(), null_mut(), null_mut(), null_mut());
        RegCloseKey(key);
        r == 0
    }
}

// "Start with Windows": a per-user Run entry pointing at this exe.
fn set_autostart(on: bool) {
    unsafe {
        let mut key: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, KEY_SET_VALUE, &mut key) != 0 {
            return;
        }
        if on {
            if let Ok(exe) = std::env::current_exe() {
                let v = wide(&format!("\"{}\"", exe.display()));
                RegSetValueExW(key, wide(RUN_VALUE).as_ptr(), 0, REG_SZ, v.as_ptr() as *const u8, (v.len() * 2) as u32);
            }
        } else {
            RegDeleteValueW(key, wide(RUN_VALUE).as_ptr());
        }
        RegCloseKey(key);
    }
}

// ---------- actions ----------
unsafe fn hide_edit() {
    let a = app();
    if !a.edit_hwnd.is_null() && IsWindowVisible(a.edit_hwnd) != 0 {
        ShowWindow(a.edit_hwnd, SW_HIDE);
    }
}

fn set_pomo_mode(mode: u8) {
    let a = app();
    a.pomo_mode = mode;
    let mins = match mode {
        0 => a.store.pomo_focus,
        1 => a.store.pomo_short,
        _ => a.store.pomo_long,
    };
    a.pomo_total = (mins.max(1) * 60) as u32;
    a.pomo_left = a.pomo_total;
    a.pomo_running = false;
    a.pomo_label = match mode {
        0 => "Focus time".into(),
        1 => "Break time".into(),
        _ => "Long break".into(),
    };
}

// Next / previous visible page.
fn step_page(dir: i32) -> usize {
    let pages = visible_pages();
    let cur = pages.iter().position(|p| *p == app().page).unwrap_or(0) as i32;
    pages[(cur + dir).rem_euclid(pages.len() as i32) as usize]
}

unsafe fn goto_page(hwnd: HWND, page: usize) {
    let a = app();
    a.page = page % PAGES;
    a.hover = (0, -1);
    if a.page == 1 {
        let m = media::snapshot();
        if m.present && !m.title.is_empty() {
            if matches!(lyrics::state(), lyrics::LyricsState::NotAvailable) {
                lyrics::refetch(&m.title, &m.artist, m.duration_secs);
            } else {
                lyrics::ensure_fetched(&m.title, &m.artist, m.duration_secs);
            }
        }
    }
    // the inline todo editor belongs to the to-do page only
    if a.page != 5 {
        hide_edit();
    }
    apply_size(hwnd);
}

// Set the system volume from a click/drag x position on the inline slider.
fn vol_set_from_x(x: i32) {
    let a = app();
    let (bx, bw) = vol_bar_geom();
    let frac = ((x - bx) as f64 / bw.max(1) as f64).clamp(0.0, 1.0) as f32;
    let changed = (frac * 100.0).round() as i32 != (a.vol_level * 100.0).round() as i32;
    if changed || a.vol_muted {
        media::volume_set(frac);
        a.vol_level = frac;
        if a.vol_muted {
            media::volume_toggle_mute();
            a.vol_muted = false;
        }
    }
}

unsafe fn do_action(code: i32, param: i32, x: i32, _y: i32, hwnd: HWND) {
    let a = app();
    match code {
        A_PREV_PAGE => goto_page(hwnd, step_page(-1)),
        A_NEXT_PAGE => goto_page(hwnd, step_page(1)),
        A_DOT => goto_page(hwnd, param as usize),
        A_SETTINGS => open_settings(),
        A_SET_SCALE => {
            a.store.scale = param;
            store::save(&a.store);
            apply_size(hwnd);
        }
        A_MINI_PLAYER => {
            a.store.mini_player = !a.store.mini_player;
            store::save(&a.store);
            apply_size(hwnd);
        }
        A_MINI_LYRICS => {
            a.store.mini_lyrics = !a.store.mini_lyrics;
            store::save(&a.store);
        }
        A_SHELF_CLEAR => {
            a.store.shelf.clear();
            a.shelf_items.clear();
            store::save(&a.store);
        }
        A_TODO_CLEAR_DONE => {
            a.store.tasks.retain(|t| !t.done);
            store::save(&a.store);
        }
        A_WATER_RESET => {
            a.store.water_ml = 0;
            a.store.water_log.clear();
            a.reset_flash_until = GetTickCount64() + 1500;
            store::save(&a.store);
        }
        A_MONITOR => {
            a.store.monitor = param;
            store::save(&a.store);
            place_window(hwnd, a.cur_px.0, a.cur_px.1);
        }
        A_MINIMAL => {
            a.store.tiny_tab = !a.store.tiny_tab;
            store::save(&a.store);
            apply_size(hwnd);
        }
        A_OPEN_MODE => {
            a.store.open_click = param == 1;
            store::save(&a.store);
        }
        A_PAGE_ICONS => {
            a.store.page_icons = !a.store.page_icons;
            a.dots_hover = false;
            store::save(&a.store);
        }
        A_ANIM => {
            a.store.anim_fast = param == 1;
            store::save(&a.store);
        }
        A_SHOW_LYRICS => {
            a.store.show_lyrics = !a.store.show_lyrics;
            if !a.store.show_lyrics && a.page == 1 {
                a.page = 0;
            }
            store::save(&a.store);
        }
        A_POMO_FOCUS | A_POMO_SHORT | A_POMO_LONG => {
            let (v, step, lo, hi) = match code {
                A_POMO_FOCUS => (&mut a.store.pomo_focus, 5, 5, 90),
                A_POMO_SHORT => (&mut a.store.pomo_short, 1, 1, 30),
                _ => (&mut a.store.pomo_long, 5, 5, 60),
            };
            *v = (*v + step * param).clamp(lo, hi);
            store::save(&a.store);
            if !a.pomo_running {
                set_pomo_mode(a.pomo_mode);
            }
        }
        A_WATER_GOAL => {
            a.store.water_goal = param;
            store::save(&a.store);
        }
        A_DRINK_ML => {
            a.store.drink_ml = (a.store.drink_ml + 50 * param).clamp(50, 3000);
            store::save(&a.store);
        }
        A_ACTIVE_FROM => {
            a.store.active_from = (a.store.active_from + param).clamp(0, 23);
            store::save(&a.store);
        }
        A_ACTIVE_UNTIL => {
            a.store.active_until = (a.store.active_until + param).clamp(0, 23);
            store::save(&a.store);
        }
        A_EMPTY_SHELF => {
            a.store.empty_shelf = !a.store.empty_shelf;
            store::save(&a.store);
        }
        A_OPEN_LRCLIB => {
            ShellExecuteW(null_mut(), wide("open").as_ptr(), wide("https://lrclib.net").as_ptr(), null(), null(), SW_SHOW as i32);
        }
        A_OPEN_REPO => {
            ShellExecuteW(null_mut(), wide("open").as_ptr(), wide(REPO_URL).as_ptr(), null(), null(), SW_SHOW as i32);
        }
        A_AUTOSTART => {
            set_autostart(!a.autostart);
            a.autostart = autostart_enabled();
        }
        A_SOUNDS => {
            a.store.sounds = !a.store.sounds;
            store::save(&a.store);
            if a.store.sounds {
                MessageBeep(0x40); // preview the chime
            }
        }
        A_HIDE_FS => {
            a.store.hide_fs = !a.store.hide_fs;
            store::save(&a.store);
        }
        A_PLAYPAUSE => media::play_pause(media::snapshot().playing),
        A_NEXT_TRACK => media::next_track(),
        A_PREV_TRACK => media::prev_track(),
        A_SEEK => {
            let m = media::snapshot();
            if m.duration_secs > 0.0 {
                let (bx, bw) = a.seek_bar;
                let frac = ((x - bx) as f64 / bw as f64).clamp(0.0, 1.0);
                media::seek(frac * m.duration_secs);
            }
        }
        A_LYRICS => goto_page(hwnd, 1),
        A_VOL_MUTE => {
            media::volume_toggle_mute();
            if let Some((lvl, muted)) = media::volume_get() {
                a.vol_level = lvl;
                a.vol_muted = muted;
            }
        }
        A_VOL_BAR => vol_set_from_x(x),
        A_LYRIC_LINE => media::seek(param as f64 / 10.0),
        A_POMO_PLAY => {
            a.pomo_running = !a.pomo_running;
        }
        A_POMO_RESET => {
            a.pomo_running = false;
            a.pomo_left = a.pomo_total;
        }
        A_POMO_MODE => set_pomo_mode(if param == 5 { 1 } else { 0 }),
        A_DRINK => {
            if a.store.water_ml < a.store.water_goal {
                let before = a.store.water_ml;
                a.store.water_ml = (a.store.water_ml + a.store.drink_ml).min(a.store.water_goal);
                let mut t: SYSTEMTIME = std::mem::zeroed();
                GetLocalTime(&mut t);
                let hr = t.wHour as i32;
                let added = a.store.water_ml - before;
                match a.store.water_log.last_mut() {
                    Some(e) if e.0 == hr => e.1 += added,
                    _ => a.store.water_log.push((hr, added)),
                }
                store::save(&a.store);
            }
        }
        A_WATER_BELL => {
            a.store.reminders = !a.store.reminders;
            store::save(&a.store);
        }
        A_SHELF_OPEN => {
            if let Some(item) = a.shelf_items.get(param as usize) {
                let path = item.path.clone();
                ShellExecuteW(hwnd, wide("open").as_ptr(), wide(&path).as_ptr(), null(), null(), SW_SHOW as i32);
            }
        }
        A_SHELF_REMOVE => {
            let idx = param as usize;
            if idx < a.shelf_items.len() {
                a.shelf_items.remove(idx);
                a.store.shelf.remove(idx);
                store::save(&a.store);
                apply_size(hwnd);
            }
        }
        A_TODO_ADD => {
            show_edit(hwnd);
        }
        A_TODO_EDIT => {
            a.editing = !a.editing;
        }
        A_TODO_TOGGLE => {
            // delete hit-zone takes precedence: it was added after toggle
            if let Some(t) = a.store.tasks.get_mut(param as usize) {
                t.done = !t.done;
            }
            store::save(&a.store);
        }
        A_TODO_DELETE => {
            let idx = param as usize;
            if idx < a.store.tasks.len() {
                a.store.tasks.remove(idx);
                store::save(&a.store);
            }
        }
        _ => {}
    }
    InvalidateRect(hwnd, null(), 0);
    let sh = app().settings_hwnd;
    if !sh.is_null() {
        InvalidateRect(sh, null(), 0);
    }
}

// ---------- todo inline edit ----------
unsafe fn show_edit(hwnd: HWND) {
    let a = app();
    if a.page != 5 {
        a.page = 5;
        apply_size(hwnd);
    }
    let (dw, dh) = expanded_size();
    let (pw, ph) = px_of((dw, dh), expanded_scale());
    // under the visible tasks, in the list column (design -> physical)
    let n = a.store.tasks.len().min(2) as i32;
    let rows_h = n.max(1) * 34 + 34;
    let top = 121 - (22 + rows_h) / 2 + 24 + n * 34;
    let lw = dw - 48 - 84 - 28;
    SetWindowPos(a.edit_hwnd, null_mut(), 48 * pw / dw, top * ph / dh, lw * pw / dw, 28 * ph / dh, SWP_NOZORDER | SWP_SHOWWINDOW);
    SetWindowTextW(a.edit_hwnd, wide("").as_ptr());
    ShowWindow(a.edit_hwnd, SW_SHOW);
    SetFocus(a.edit_hwnd);
}

unsafe fn commit_edit() {
    let a = app();
    let mut buf = [0u16; 256];
    let n = GetWindowTextW(a.edit_hwnd, buf.as_mut_ptr(), 256);
    ShowWindow(a.edit_hwnd, SW_HIDE);
    if n > 0 {
        let title = String::from_utf16_lossy(&buf[..n as usize]).trim().to_string();
        if !title.is_empty() {
            a.store.tasks.push(store::Task { title, done: false });
            store::save(&a.store);
        }
    }
    SetFocus(a.hwnd);
    InvalidateRect(a.hwnd, null(), 0);
}

unsafe extern "system" fn edit_subclass(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == WM_KEYDOWN {
        if wparam as u32 == 0x0D {
            commit_edit();
            return 0;
        }
        if wparam as u32 == 0x1B {
            ShowWindow(hwnd, SW_HIDE);
            SetFocus(app().hwnd);
            return 0;
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

// ---------- pointer helpers ----------
// Client pixel position -> design units.
unsafe fn client_to_design(hwnd: HWND, px: i32, py: i32) -> (i32, i32) {
    let mut crc: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut crc);
    let (dw, dh) = cur_design();
    (px * dw / crc.right.max(1), py * dh / crc.bottom.max(1))
}

unsafe fn lparam_to_design(hwnd: HWND, lparam: LPARAM) -> (i32, i32) {
    client_to_design(
        hwnd,
        (lparam & 0xFFFF) as i16 as i32,
        ((lparam >> 16) & 0xFFFF) as i16 as i32,
    )
}

// Shrink back to the idle tab (used on mouse leave and after a drag that
// ended outside the window).
unsafe fn collapse(hwnd: HWND) {
    let a = app();
    let edit_visible = !a.edit_hwnd.is_null() && IsWindowVisible(a.edit_hwnd) != 0;
    if a.expanded && !edit_visible && !a.vol_drag {
        a.expanded = false;
        a.dots_hover = false;
        a.hover = (0, -1);
        apply_size(hwnd);
    }
}

// ---------- settings window ----------
// Its own window, laid out like the "Nusa Settings" design (dark theme):
// a sidebar of sections with coloured icon tiles, About pinned at the
// bottom, and grouped cards of rows on the right. Opened from the gear on
// the notch. Design units are 96-dpi pixels, scaled to Windows scaling.
const S_WIN: u32 = cr(34, 34, 37);
const S_SIDE: u32 = cr(24, 24, 27);
const S_CARD: u32 = cr(44, 44, 47);
const S_TEXT: u32 = cr(245, 245, 247);
const S_SUB: u32 = cr(152, 152, 159);
const S_LINE: u32 = cr(60, 60, 63);
const S_SEL: u32 = cr(52, 52, 55);
const S_HOVER: u32 = cr(42, 42, 45);
const S_SEG: u32 = cr(61, 61, 64);
const S_SEGSEL: u32 = cr(99, 99, 102);
const S_OFF: u32 = cr(74, 74, 77);
const S_ON: u32 = cr(52, 199, 89);
const S_RED: u32 = cr(255, 69, 58);

enum RowKind {
    Switch(bool, i32, bool),             // on, action, enabled
    Seg(Vec<(String, i32)>, i32, i32),   // (label, value), current value, action
    Step(String, i32),                   // shown value, action (param -1 / +1)
    Link(String, i32),                   // right-hand text, action
    Plain(String, i32),                  // red text button, action
    Text(String),                        // right-hand text
}

struct Row {
    title: String,
    sub: String,
    kind: RowKind,
}

fn row(title: &str, sub: &str, kind: RowKind) -> Row {
    Row { title: title.to_string(), sub: sub.to_string(), kind }
}

fn seg(opts: &[(&str, i32)], cur: i32, code: i32) -> RowKind {
    RowKind::Seg(opts.iter().map(|(l, v)| (l.to_string(), *v)).collect(), cur, code)
}

fn settings_scale() -> i32 {
    unsafe {
        let s = (GetDpiForSystem() as i32 * 100 / 96).max(100);
        // keep the window on screen on small displays
        let fit = GetSystemMetrics(SM_CYSCREEN) * 90 / SET_H;
        s.min(fit.max(60))
    }
}

unsafe fn open_settings() {
    let a = app();
    if !a.settings_hwnd.is_null() {
        ShowWindow(a.settings_hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(a.settings_hwnd);
        return;
    }
    a.autostart = autostart_enabled();
    hide_edit();
    let sc = settings_scale();
    let (w, h) = px_of((SET_W, SET_H), sc);
    let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let hwnd = CreateWindowExW(
        WS_EX_APPWINDOW,
        wide("IsleSettings").as_ptr(),
        wide("Isle Settings").as_ptr(),
        WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
        (sw - w) / 2, (sh - h) / 2, w, h,
        null_mut(), null_mut(), GetModuleHandleW(null()), null(),
    );
    if hwnd.is_null() {
        return;
    }
    a.settings_hwnd = hwnd;
    let rad = 28 * sc / 100; // 14 px corners
    SetWindowRgn(hwnd, CreateRoundRectRgn(0, 0, w + 1, h + 1, rad, rad), 1);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    InvalidateRect(a.hwnd, null(), 0);
}

// Section label above a card ("Startup", "Notch" ...).
unsafe fn s_h2(hdc: HDC, x0: i32, cw: i32, y: &mut i32, label: &str) {
    *y += 16;
    draw_text(hdc, label, x0 + 8, *y, cw, 16, "Segoe UI", 12, 500, S_SUB, DT_LEFT);
    *y += 22;
}

fn row_h(r: &Row) -> i32 {
    if r.sub.is_empty() { 44 } else { 52 }
}

unsafe fn s_switch(hdc: HDC, x: i32, y: i32, on: bool, enabled: bool) {
    let track = match (on, enabled) {
        (true, true) => S_ON,
        (true, false) => cr(36, 92, 52),
        _ => S_OFF,
    };
    fill_round(hdc, x, y, 40, 24, 24, track);
    fill_circle(hdc, if on { x + 28 } else { x + 12 }, y + 12, 10, if enabled { WHITE } else { cr(150, 150, 155) });
}

// Segmented control ending at `right`, vertically centred on `cy`.
unsafe fn s_seg(hdc: HDC, right: i32, cy: i32, opts: &[(String, i32)], cur: i32, code: i32) {
    let widths: Vec<i32> = opts.iter().map(|(l, _)| text_width(l, "Segoe UI", 12, 400, hdc) + 24).collect();
    let total: i32 = widths.iter().sum::<i32>() + 4;
    let x0 = right - total;
    fill_round(hdc, x0, cy - 14, total, 28, 16, S_SEG);
    let mut x = x0 + 2;
    for (i, (label, val)) in opts.iter().enumerate() {
        let on = *val == cur;
        if on {
            fill_round(hdc, x, cy - 12, widths[i], 24, 12, S_SEGSEL);
        }
        draw_text(hdc, label, x, cy - 12, widths[i], 24, "Segoe UI", 12, if on { 600 } else { 400 }, S_TEXT, DT_CENTER);
        add_action(x, cy - 12, widths[i], 24, code, *val);
        x += widths[i];
    }
}

unsafe fn s_row(hdc: HDC, x0: i32, cw: i32, y: i32, h: i32, r: &Row) {
    let enabled = !matches!(r.kind, RowKind::Switch(_, _, false));
    let tc = if enabled { S_TEXT } else { S_SUB };
    if r.sub.is_empty() {
        draw_text(hdc, &r.title, x0 + 14, y, cw - 28, h, "Segoe UI", 14, 500, tc, DT_LEFT | DT_END_ELLIPSIS);
    } else {
        draw_text(hdc, &r.title, x0 + 14, y + 9, cw - 80, 19, "Segoe UI", 14, 500, tc, DT_LEFT | DT_END_ELLIPSIS);
        draw_text(hdc, &r.sub, x0 + 14, y + 28, cw - 80, 16, "Segoe UI", 12, 400, S_SUB, DT_LEFT | DT_END_ELLIPSIS);
    }
    let right = x0 + cw - 14;
    let cy = y + h / 2;
    match &r.kind {
        RowKind::Switch(on, code, en) => {
            s_switch(hdc, right - 40, cy - 12, *on, *en);
            if *en {
                add_action(x0, y, cw, h, *code, 0);
            }
        }
        RowKind::Seg(opts, cur, code) => s_seg(hdc, right, cy, opts, *cur, *code),
        RowKind::Step(val, code) => {
            // [-]  value  [+]
            let plus_x = right - 26;
            let val_x = plus_x - 14 - 62;
            let minus_x = val_x - 14 - 26;
            fill_circle(hdc, minus_x + 13, cy, 13, S_SEG);
            draw_text(hdc, "\u{2212}", minus_x, cy - 13, 26, 26, "Segoe UI", 16, 400, S_TEXT, DT_CENTER);
            draw_text(hdc, val, val_x, cy - 12, 62, 24, "Segoe UI", 14, 400, S_SUB, DT_CENTER);
            fill_circle(hdc, plus_x + 13, cy, 13, S_SEG);
            draw_text(hdc, "+", plus_x, cy - 13, 26, 26, "Segoe UI", 16, 400, S_TEXT, DT_CENTER);
            add_action(minus_x - 3, cy - 16, 32, 32, *code, -1);
            add_action(plus_x - 3, cy - 16, 32, 32, *code, 1);
        }
        RowKind::Link(text, code) => {
            let t = format!("{}  \u{203a}", text);
            let tw = text_width(&t, "Segoe UI", 14, 400, hdc);
            draw_text(hdc, &t, right - tw - 4, y, tw + 4, h, "Segoe UI", 14, 400, S_SUB, DT_RIGHT);
            add_action(x0, y, cw, h, *code, 0);
        }
        RowKind::Plain(text, code) => {
            let tw = text_width(text, "Segoe UI", 14, 400, hdc);
            draw_text(hdc, text, right - tw - 4, y, tw + 4, h, "Segoe UI", 14, 400, S_RED, DT_RIGHT);
            add_action(right - tw - 12, y + 6, tw + 14, h - 12, *code, 0);
        }
        RowKind::Text(text) => {
            let tw = text_width(text, "Segoe UI", 14, 400, hdc);
            draw_text(hdc, text, right - tw - 4, y, tw + 4, h, "Segoe UI", 14, 400, S_SUB, DT_RIGHT);
        }
    }
}

// A card of rows with hairlines between them.
unsafe fn s_group(hdc: HDC, x0: i32, cw: i32, y: &mut i32, rows: &[Row]) {
    let total: i32 = rows.iter().map(row_h).sum();
    fill_round(hdc, x0, *y, cw, total, 20, S_CARD);
    let mut ry = *y;
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            fill_rect_c(hdc, x0 + 14, ry, cw - 14, 1, S_LINE);
        }
        let h = row_h(r);
        s_row(hdc, x0, cw, ry, h, r);
        ry += h;
    }
    *y += total;
}

// The display picker: every screen drawn to scale, the notch's one in green.
unsafe fn s_monitors(hdc: HDC, x0: i32, cw: i32, y: i32, h: i32) {
    let list = monitors();
    if list.is_empty() {
        return;
    }
    let cur = {
        let want = app().store.monitor;
        if want >= 0 && (want as usize) < list.len() { want as usize } else { list.iter().position(|(_, m)| *m).unwrap_or(0) }
    };
    let (mut l, mut t, mut r, mut b) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for (rc, _) in &list {
        l = l.min(rc.left);
        t = t.min(rc.top);
        r = r.max(rc.right);
        b = b.max(rc.bottom);
    }
    let (vw, vh) = ((r - l).max(1) as f64, (b - t).max(1) as f64);
    let (aw, ah) = ((cw - 40) as f64, h as f64);
    // a single screen is drawn at the design's 274 x 150
    let k = if list.len() == 1 { (274.0 / vw).min(150.0 / vh) } else { (aw / vw).min(ah / vh) * 0.92 };
    let (ow, oh) = (vw * k, vh * k);
    let ox = x0 as f64 + (cw as f64 - ow) / 2.0;
    let oy = y as f64 + (ah - oh) / 2.0;
    for (i, (rc, main)) in list.iter().enumerate() {
        let mx = (ox + (rc.left - l) as f64 * k) as i32 + 3;
        let my = (oy + (rc.top - t) as f64 * k) as i32 + 3;
        let mw = ((rc.right - rc.left) as f64 * k) as i32 - 6;
        let mh = ((rc.bottom - rc.top) as f64 * k) as i32 - 6;
        let on = i == cur;
        fill_round(hdc, mx, my, mw, mh, 24, if on { cr(48, 209, 88) } else { cr(80, 80, 85) });
        fill_round(hdc, mx + 2, my + 2, mw - 4, mh - 4, 20, if on { cr(31, 58, 41) } else { cr(52, 52, 56) });
        if on {
            // the notch on top of the chosen screen
            fill_round(hdc, mx + mw / 2 - 19, my + 2, 38, 6, 6, S_TEXT);
            fill_rect_c(hdc, mx + mw / 2 - 19, my + 2, 38, 3, S_TEXT);
        }
        draw_text(hdc, &format!("{}", i + 1), mx, my + mh / 2 - 20, mw, 24, "Segoe UI", 17, 700, S_TEXT, DT_CENTER);
        let info = format!("{}\u{00d7}{}{}", rc.right - rc.left, rc.bottom - rc.top, if *main { " \u{00b7} Main" } else { "" });
        draw_text(hdc, &info, mx, my + mh / 2 + 4, mw, 18, "Segoe UI", 12, 400, S_SUB, DT_CENTER | DT_END_ELLIPSIS);
        add_action(mx, my, mw, mh, A_MONITOR, i as i32);
    }
}

// App icon for About: a little notch face, like the design's logo.
unsafe fn s_logo(hdc: HDC, cx: i32, y: i32) {
    let x = cx - 40;
    fill_round(hdc, x + 2, y + 4, 80, 80, 36, cr(14, 14, 16)); // soft shadow
    fill_round(hdc, x, y, 80, 80, 36, cr(232, 235, 244));
    // body: dark notch shape
    let pts = [
        POINT { x: x + 16, y: y + 24 },
        POINT { x: x + 28, y: y + 32 },
        POINT { x: x + 52, y: y + 32 },
        POINT { x: x + 64, y: y + 24 },
        POINT { x: x + 64, y: y + 56 },
        POINT { x: x + 16, y: y + 56 },
    ];
    let brush = CreateSolidBrush(cr(18, 18, 24));
    let pen = CreatePen(PS_NULL as i32, 0, 0);
    let ob = SelectObject(hdc, brush as HGDIOBJ);
    let op = SelectObject(hdc, pen as HGDIOBJ);
    Polygon(hdc, pts.as_ptr(), pts.len() as i32);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    DeleteObject(brush as HGDIOBJ);
    DeleteObject(pen as HGDIOBJ);
    fill_round(hdc, x + 16, y + 46, 48, 20, 22, cr(18, 18, 24));
    // eyes
    fill_round(hdc, x + 28, y + 42, 6, 9, 6, cr(207, 230, 255));
    fill_round(hdc, x + 46, y + 42, 6, 9, 6, cr(207, 230, 255));
    // smile: blue -> purple
    for i in 0..16 {
        let t = i as f64 / 15.0;
        let col = cr((10.0 + 181.0 * t) as u32, (132.0 - 42.0 * t) as u32, (255.0 - 13.0 * t) as u32);
        let px = x + 24 + (32.0 * t) as i32;
        let py = y + 58 + (3.0 * (1.0 - (2.0 * t - 1.0).powi(2))) as i32;
        fill_circle(hdc, px, py, 2, col);
    }
}

fn hour_str(h: i32) -> String {
    format!("{:02}:00", h)
}

unsafe fn paint_settings_window(hdc: HDC, w: i32, h: i32) {
    let a = app();
    a.set_actions.clear();
    a.painting_settings = true;
    fill_rect_c(hdc, 0, 0, w, h, S_WIN);
    fill_rect_c(hdc, 0, 0, SIDEBAR_W, h, S_SIDE);
    fill_rect_c(hdc, SIDEBAR_W, 0, 1, h, S_LINE);
    // window lights: close, minimise, (zoom does nothing)
    for (i, col) in [cr(255, 95, 87), cr(254, 188, 46), cr(40, 200, 64)].iter().enumerate() {
        let cx = 24 + i as i32 * 20;
        fill_circle(hdc, cx, 22, 6, *col);
    }
    add_action(14, 12, 20, 20, S_CLOSE, 0);
    add_action(34, 12, 20, 20, S_MINIMIZE, 0);
    // sidebar
    let sec = a.settings_section.min(SECTIONS.len() - 1);
    let last = SECTIONS.len() - 1;
    for (i, (name, glyph, col)) in SECTIONS.iter().enumerate() {
        let y = if i == last { h - 50 } else { 44 + i as i32 * 38 };
        if i == sec {
            fill_round(hdc, 12, y, SIDEBAR_W - 24, 36, 16, S_SEL);
        } else if a.settings_hover == i as i32 {
            fill_round(hdc, 12, y, SIDEBAR_W - 24, 36, 16, S_HOVER);
        }
        fill_round(hdc, 20, y + 5, 26, 26, 14, *col);
        if glyph.is_empty() {
            draw_droplet(hdc, 33, y + 18, WHITE);
        } else {
            draw_text(hdc, glyph, 20, y + 5, 26, 26, "Segoe MDL2 Assets", 13, 400, WHITE, DT_CENTER);
        }
        draw_text(hdc, name, 56, y, SIDEBAR_W - 72, 36, "Segoe UI", 14, 400, S_TEXT, DT_LEFT);
        add_action(12, y, SIDEBAR_W - 24, 36, S_SECTION, i as i32);
    }
    // content
    let x0 = SIDEBAR_W + 26;
    let cw = w - x0 - 26;
    let mut y = 22;
    if sec != last {
        draw_text(hdc, SECTIONS[sec].0, x0, y, cw, 30, "Segoe UI", 22, 700, S_TEXT, DT_LEFT);
        y += 30;
    }
    let st = &a.store;
    match sec {
        0 => {
            s_h2(hdc, x0, cw, &mut y, "Startup");
            s_group(hdc, x0, cw, &mut y, &[row("Start with Windows", "", RowKind::Switch(a.autostart, A_AUTOSTART, true))]);
            s_h2(hdc, x0, cw, &mut y, "Notch");
            // Size: heading, full-width segmented control, then more rows
            let card_h = 52 + 40 + 52;
            fill_round(hdc, x0, y, cw, card_h, 20, S_CARD);
            draw_text(hdc, "Size", x0 + 14, y + 9, cw - 28, 19, "Segoe UI", 14, 500, S_TEXT, DT_LEFT);
            draw_text(hdc, "Applies right away", x0 + 14, y + 28, cw - 28, 16, "Segoe UI", 12, 400, S_SUB, DT_LEFT);
            let (sx0, sw_all) = (x0 + 14, cw - 28);
            let sy = y + 52;
            fill_round(hdc, sx0, sy, sw_all, 28, 16, S_SEG);
            let choices: [(i32, &str); 5] = [(0, "Auto"), (100, "100%"), (115, "115%"), (130, "130%"), (150, "150%")];
            for (i, (val, label)) in choices.iter().enumerate() {
                let bx = sx0 + 2 + (sw_all - 4) * i as i32 / 5;
                let bw = sx0 + 2 + (sw_all - 4) * (i as i32 + 1) / 5 - bx;
                let on = st.scale == *val;
                if on {
                    fill_round(hdc, bx, sy + 2, bw, 24, 12, S_SEGSEL);
                }
                draw_text(hdc, label, bx, sy + 2, bw, 24, "Segoe UI", 12, if on { 600 } else { 400 }, S_TEXT, DT_CENTER);
                add_action(bx, sy, bw, 28, A_SET_SCALE, *val);
            }
            let ry = y + 92;
            fill_rect_c(hdc, x0 + 14, ry, cw - 14, 1, S_LINE);
            s_row(hdc, x0, cw, ry, 52, &row("Minimal notch", "A tiny tab instead of the clock when nothing is playing", RowKind::Switch(st.tiny_tab, A_MINIMAL, true)));
            y += card_h;
            s_h2(hdc, x0, cw, &mut y, "Sounds");
            s_group(hdc, x0, cw, &mut y, &[row("Reminder sounds", "Chimes for Pomodoro and Water", RowKind::Switch(st.sounds, A_SOUNDS, true))]);
            s_h2(hdc, x0, cw, &mut y, "Full screen");
            s_group(hdc, x0, cw, &mut y, &[row("Hide in full screen", "Games, videos and slides that fill the screen", RowKind::Switch(st.hide_fs, A_HIDE_FS, true))]);
        }
        1 => {
            s_h2(hdc, x0, cw, &mut y, "Notch screen");
            let multi = monitors().len() > 1;
            let card_h = 52 + 182;
            fill_round(hdc, x0, y, cw, card_h, 20, S_CARD);
            draw_text(hdc, "Display", x0 + 14, y + 9, cw - 28, 19, "Segoe UI", 14, 500, S_TEXT, DT_LEFT);
            let sub = if multi { "Click a screen to put the notch there" } else { "Connect another display to choose where the notch appears" };
            draw_text(hdc, sub, x0 + 14, y + 28, cw - 28, 16, "Segoe UI", 12, 400, S_SUB, DT_LEFT);
            s_monitors(hdc, x0, cw, y + 62, 150);
        }
        2 => {
            s_h2(hdc, x0, cw, &mut y, "Notch");
            s_group(hdc, x0, cw, &mut y, &[
                row("Open the notch", "", seg(&[("Hover", 0), ("Click", 1)], st.open_click as i32, A_OPEN_MODE)),
                row("Page icons", "Show page icons when you hover the page dots", RowKind::Switch(st.page_icons, A_PAGE_ICONS, true)),
            ]);
            s_h2(hdc, x0, cw, &mut y, "Motion");
            s_group(hdc, x0, cw, &mut y, &[row("Animation speed", "", seg(&[("Normal", 0), ("Fast", 1)], st.anim_fast as i32, A_ANIM))]);
        }
        3 => {
            s_h2(hdc, x0, cw, &mut y, "Lyrics");
            s_group(hdc, x0, cw, &mut y, &[row("Show lyrics", "From LRCLIB, a community-contributed lyrics database", RowKind::Switch(st.show_lyrics, A_SHOW_LYRICS, true))]);
            s_h2(hdc, x0, cw, &mut y, "Minimized notch");
            s_group(hdc, x0, cw, &mut y, &[
                row("Music in the minimized notch", "A sound wave next to the clock while music plays", RowKind::Switch(st.mini_player, A_MINI_PLAYER, true)),
                row("Lyrics in the minimized notch", "The current line instead of the clock (always shown when closed on Lyrics)", RowKind::Switch(st.mini_lyrics, A_MINI_LYRICS, st.mini_player && st.show_lyrics)),
            ]);
        }
        4 => {
            let done = st.tasks.iter().filter(|t| t.done).count();
            s_h2(hdc, x0, cw, &mut y, "Tasks");
            s_group(hdc, x0, cw, &mut y, &[
                row("Done today", "", RowKind::Text(format!("{} of {}", done, st.tasks.len()))),
                row("Completed tasks", "", RowKind::Plain("Clear".into(), A_TODO_CLEAR_DONE)),
            ]);
        }
        5 => {
            s_h2(hdc, x0, cw, &mut y, "Durations");
            s_group(hdc, x0, cw, &mut y, &[
                row("Focus", "", RowKind::Step(format!("{} min", st.pomo_focus), A_POMO_FOCUS)),
                row("Short break", "", RowKind::Step(format!("{} min", st.pomo_short), A_POMO_SHORT)),
                row("Long break", "", RowKind::Step(format!("{} min", st.pomo_long), A_POMO_LONG)),
            ]);
            draw_text(hdc, "Every fourth break is a long one.", x0 + 8, y + 8, cw, 18, "Segoe UI", 12, 400, S_SUB, DT_LEFT);
        }
        6 => {
            s_h2(hdc, x0, cw, &mut y, "Reminders");
            s_group(hdc, x0, cw, &mut y, &[row("Water reminders", "Spread across your active hours", RowKind::Switch(st.reminders, A_WATER_BELL, true))]);
            s_h2(hdc, x0, cw, &mut y, "Plan");
            s_group(hdc, x0, cw, &mut y, &[
                row("Daily goal", "", seg(&[("1.5 L", 1500), ("2 L", 2000), ("2.5 L", 2500), ("3 L", 3000)], st.water_goal, A_WATER_GOAL)),
                row("Per drink", "", RowKind::Step(format!("{} ml", st.drink_ml), A_DRINK_ML)),
                row("Active from", "", RowKind::Step(hour_str(st.active_from), A_ACTIVE_FROM)),
                row("Active until", "", RowKind::Step(hour_str(st.active_until), A_ACTIVE_UNTIL)),
            ]);
            s_h2(hdc, x0, cw, &mut y, "Today");
            let flash = GetTickCount64() < a.reset_flash_until;
            s_group(hdc, x0, cw, &mut y, &[row("Today\u{2019}s progress", "", RowKind::Plain(if flash { "Cleared".into() } else { "Reset".into() }, A_WATER_RESET))]);
        }
        7 => {
            s_h2(hdc, x0, cw, &mut y, "Shelf");
            s_group(hdc, x0, cw, &mut y, &[
                row("Start with an empty Shelf", "Files from your last session are cleared when Isle starts", RowKind::Switch(st.empty_shelf, A_EMPTY_SHELF, true)),
                row("Files on the Shelf", "", RowKind::Plain(if a.shelf_items.is_empty() { "None".into() } else { format!("Clear {}", a.shelf_items.len()) }, A_SHELF_CLEAR)),
            ]);
        }
        _ => {
            let cx = x0 + cw / 2;
            y += 6;
            s_logo(hdc, cx, y);
            y += 88;
            draw_text(hdc, "Isle", x0, y, cw, 32, "Segoe UI", 23, 700, S_TEXT, DT_CENTER);
            y += 32;
            let ver = format!("Version {}", env!("CARGO_PKG_VERSION"));
            draw_text(hdc, &ver, x0, y, cw, 19, "Segoe UI", 13, 400, S_SUB, DT_CENTER);
            y += 19;
            draw_text(hdc, "A living notch for Windows", x0, y, cw, 19, "Segoe UI", 13, 400, S_SUB, DT_CENTER);
            y += 22;
            s_h2(hdc, x0, cw, &mut y, "Info");
            s_group(hdc, x0, cw, &mut y, &[
                row("Website", "", RowKind::Link("github.com/Sree-Ram-MS".into(), A_OPEN_REPO)),
                row("Lyrics", "", RowKind::Link("LRCLIB".into(), A_OPEN_LRCLIB)),
                row("Made by", "", RowKind::Text("Sree Ram M S".into())),
            ]);
        }
    }
    a.painting_settings = false;
}

unsafe fn settings_hit(x: i32, y: i32) -> Option<(i32, i32)> {
    let mut hit = None;
    for (r, code, param) in &app().set_actions {
        if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
            hit = Some((*code, *param));
        }
    }
    hit
}

unsafe fn settings_to_design(hwnd: HWND, px: i32, py: i32) -> (i32, i32) {
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rc);
    (px * SET_W / rc.right.max(1), py * SET_H / rc.bottom.max(1))
}

unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            render_scaled(hdc, hdc, 0, 0, rc.right, rc.bottom, SET_W, SET_H, &|m| paint_settings_window(m, SET_W, SET_H));
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND => 1,
        // drag the window by its empty top band
        WM_NCHITTEST => {
            let mut p = POINT { x: (lparam & 0xFFFF) as i16 as i32, y: ((lparam >> 16) & 0xFFFF) as i16 as i32 };
            ScreenToClient(hwnd, &mut p);
            let (x, y) = settings_to_design(hwnd, p.x, p.y);
            if y >= 0 && y < 56 && settings_hit(x, y).is_none() {
                return HTCAPTION as LRESULT;
            }
            HTCLIENT as LRESULT
        }
        WM_SETCURSOR => {
            SetCursor(LoadCursorW(null_mut(), IDC_ARROW));
            1
        }
        WM_LBUTTONUP => {
            let (x, y) = settings_to_design(hwnd, (lparam & 0xFFFF) as i16 as i32, ((lparam >> 16) & 0xFFFF) as i16 as i32);
            if let Some((code, param)) = settings_hit(x, y) {
                match code {
                    S_CLOSE => {
                        DestroyWindow(hwnd);
                    }
                    S_MINIMIZE => {
                        ShowWindow(hwnd, SW_MINIMIZE);
                    }
                    S_SECTION => {
                        app().settings_section = param as usize;
                        if param == 0 {
                            app().autostart = autostart_enabled();
                        }
                        InvalidateRect(hwnd, null(), 0);
                    }
                    _ => {
                        do_action(code, param, x, y, app().hwnd);
                        if code == A_WATER_RESET {
                            SetTimer(hwnd, 1, 1600, None);
                        }
                    }
                }
            }
            0
        }
        WM_TIMER => {
            KillTimer(hwnd, 1);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        // hover highlight on the sidebar
        WM_MOUSEMOVE => {
            let (x, y) = settings_to_design(hwnd, (lparam & 0xFFFF) as i16 as i32, ((lparam >> 16) & 0xFFFF) as i16 as i32);
            let hov = match settings_hit(x, y) {
                Some((S_SECTION, p)) => p,
                _ => -1,
            };
            let a = app();
            if hov != a.settings_hover {
                a.settings_hover = hov;
                InvalidateRect(hwnd, null(), 0);
            }
            if hov >= 0 {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut tme);
            }
            0
        }
        WM_MOUSELEAVE_MSG => {
            if app().settings_hover != -1 {
                app().settings_hover = -1;
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_KEYDOWN if wparam as u32 == 0x1B => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let a = app();
            a.settings_hwnd = null_mut();
            InvalidateRect(a.hwnd, null(), 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// Render a painter that draws in design units (dw x dh) into `dst` at
// (dx, dy, pw, ph): drawn into a 2x buffer through a world transform, then
// halftone-downscaled for smooth ClearType text and shapes.
unsafe fn render_scaled(ref_dc: HDC, dst: HDC, dx: i32, dy: i32, pw: i32, ph: i32, dw: i32, dh: i32, painter: &dyn Fn(HDC)) {
    const SS: i32 = 2;
    if pw <= 0 || ph <= 0 {
        return;
    }
    let mem = CreateCompatibleDC(ref_dc);
    let bmp = CreateCompatibleBitmap(ref_dc, pw * SS, ph * SS);
    let old = SelectObject(mem, bmp as HGDIOBJ);
    // v0.2.1 used MM_ANISOTROPIC window/viewport exts here and on Ram's
    // machine the mapping silently did not apply. A world transform
    // (GM_ADVANCED) scales every draw call reliably.
    SetGraphicsMode(mem, GM_ADVANCED);
    let xform = XFORM {
        eM11: (pw * SS) as f32 / dw as f32,
        eM12: 0.0,
        eM21: 0.0,
        eM22: (ph * SS) as f32 / dh as f32,
        eDx: 0.0,
        eDy: 0.0,
    };
    SetWorldTransform(mem, &xform);
    let prev_k = app().paint_k;
    app().paint_k = (xform.eM11, xform.eM22);
    painter(mem);
    app().paint_k = prev_k;
    // Reset to identity BEFORE StretchBlt: a transform left active on the
    // source DC can be applied to the source rectangle too.
    ModifyWorldTransform(mem, null(), MWT_IDENTITY);
    SetStretchBltMode(dst, HALFTONE);
    SetBrushOrgEx(dst, 0, 0, null_mut());
    StretchBlt(dst, dx, dy, pw, ph, mem, 0, 0, pw * SS, ph * SS, SRCCOPY);
    SelectObject(mem, old);
    DeleteObject(bmp as HGDIOBJ);
    DeleteDC(mem);
}

// During a resize animation: draw one look of the notch (minimized or
// opened) at its own final size, centred on the top edge of the window and
// clipped to it, blended over `back` with the given opacity.
unsafe fn blend_frame(ref_dc: HDC, back: HDC, pw: i32, ph: i32, expanded: bool, alpha: u8) {
    if alpha == 0 {
        return;
    }
    let (dw, dh) = if expanded { expanded_size() } else { collapsed_size() };
    let (fw, fh) = px_of((dw, dh), expanded_scale());
    let fd = CreateCompatibleDC(ref_dc);
    let fb = CreateCompatibleBitmap(ref_dc, fw, fh);
    let old = SelectObject(fd, fb as HGDIOBJ);
    render_scaled(ref_dc, fd, 0, 0, fw, fh, dw, dh, &|m| paint(m, dw, dh, expanded));
    let (sx, dx, cw) = if fw > pw { ((fw - pw) / 2, 0, pw) } else { (0, (pw - fw) / 2, fw) };
    let ch = fh.min(ph);
    let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: alpha, AlphaFormat: 0 };
    AlphaBlend(back, dx, 0, cw, ch, fd, sx, 0, cw, ch, bf);
    SelectObject(fd, old);
    DeleteObject(fb as HGDIOBJ);
    DeleteDC(fd);
}

// ---------- window proc ----------
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let edit = CreateWindowExW(
                0,
                wide("Edit").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_BORDER,
                0, 0, 10, 10,
                hwnd,
                EDIT_ID as isize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            app().edit_hwnd = edit;
            let font = get_font("Segoe UI", 13, 400);
            SendMessageW(edit, WM_SETFONT, font as WPARAM, 1);
            SetWindowSubclass(edit, Some(edit_subclass), 1, 0);
            DragAcceptFiles(hwnd, 1);
            SetTimer(hwnd, TIMER_ID, 1000, None);
            0
        }
        WM_CTLCOLOREDIT => {
            let hdc = wparam as HDC;
            SetTextColor(hdc, WHITE);
            SetBkColor(hdc, CARD2);
            CreateSolidBrush(CARD2) as LRESULT
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            let (pw, ph) = (rc.right - rc.left, rc.bottom - rc.top);
            let a = app();
            if !a.animating {
                let (dw, dh) = cur_design();
                let exp = a.expanded;
                render_scaled(hdc, hdc, 0, 0, pw, ph, dw, dh, &|m| paint(m, dw, dh, exp));
            } else {
                // Mid-animation: the black shape is resizing; the minimized
                // look fades out early and the opened page fades in late
                // (or the reverse when closing), so nothing ever stretches.
                let back = CreateCompatibleDC(hdc);
                let bb = CreateCompatibleBitmap(hdc, pw.max(1), ph.max(1));
                let old = SelectObject(back, bb as HGDIOBJ);
                fill_rect_c(back, 0, 0, pw, ph, BLACK);
                let open = a.open_level.clamp(0.0, 1.0);
                if open < 0.35 {
                    blend_frame(hdc, back, pw, ph, false, ((1.0 - open / 0.35) * 255.0) as u8);
                }
                if open > 0.3 {
                    blend_frame(hdc, back, pw, ph, true, (((open - 0.3) / 0.7).clamp(0.0, 1.0) * 255.0) as u8);
                }
                BitBlt(hdc, 0, 0, pw, ph, back, 0, 0, SRCCOPY);
                SelectObject(back, old);
                DeleteObject(bb as HGDIOBJ);
                DeleteDC(back);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            let a = app();
            if !a.tracking {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut tme);
                a.tracking = true;
            }
            if !a.expanded {
                if !a.store.open_click {
                    a.expanded = true;
                    apply_size(hwnd);
                }
                return 0;
            }
            if a.animating {
                return 0;
            }
            let (x, y) = lparam_to_design(hwnd, lparam);
            // dragging the volume slider
            if a.vol_drag && (wparam & 0x0001) != 0 {
                vol_set_from_x(x);
                InvalidateRect(hwnd, null(), 0);
                return 0;
            }
            // bottom dots <-> page icons on hover
            let (dw, dh) = cur_design();
            let z = dots_zone(dw, dh, a.dots_hover);
            let over = a.store.page_icons && x >= z.left && x < z.right && y >= z.top && y < z.bottom;
            if over != a.dots_hover {
                a.dots_hover = over;
                InvalidateRect(hwnd, null(), 0);
            }
            // hover looks (buttons, rows, the volume slider)
            let mut hov = (0, -1);
            for (r, code, param) in &a.actions {
                if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                    hov = (*code, *param);
                }
            }
            if hov != a.hover {
                a.hover = hov;
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_MOUSELEAVE_MSG => {
            let a = app();
            a.tracking = false;
            if a.dots_hover || a.hover.0 != 0 {
                a.dots_hover = false;
                a.hover = (0, -1);
                InvalidateRect(hwnd, null(), 0);
            }
            // while dragging with the mouse captured, collapse happens on release
            collapse(hwnd);
            0
        }
        WM_LBUTTONDOWN => {
            let a = app();
            if a.animating {
                return 0;
            }
            let (x, y) = lparam_to_design(hwnd, lparam);
            let mut hit: Option<i32> = None;
            for (r, code, _) in &a.actions {
                if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                    hit = Some(*code);
                }
            }
            if hit == Some(A_VOL_BAR) {
                a.vol_drag = true;
                SetCapture(hwnd);
                vol_set_from_x(x);
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_SETCURSOR => {
            SetCursor(LoadCursorW(null_mut(), IDC_ARROW));
            1
        }
        WM_LBUTTONUP => {
            if app().vol_drag {
                app().vol_drag = false;
                ReleaseCapture();
                // released outside the notch: close it now
                let mut p = POINT { x: 0, y: 0 };
                GetCursorPos(&mut p);
                let mut wr: RECT = std::mem::zeroed();
                GetWindowRect(hwnd, &mut wr);
                if !(p.x >= wr.left && p.x < wr.right && p.y >= wr.top && p.y < wr.bottom) {
                    collapse(hwnd);
                }
                InvalidateRect(hwnd, null(), 0);
                return 0;
            }
            if app().animating {
                return 0;
            }
            if !app().expanded {
                // "Open the notch: Click"
                app().expanded = true;
                app().popup_until = 0;
                apply_size(hwnd);
                return 0;
            }
            let (x, y) = lparam_to_design(hwnd, lparam);
            // last matching action wins (delete zones were added after toggles)
            let mut hit: Option<(i32, i32)> = None;
            for (r, code, param) in &app().actions {
                if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                    hit = Some((*code, *param));
                }
            }
            if let Some((code, param)) = hit {
                do_action(code, param, x, y, hwnd);
            }
            0
        }
        WM_DROPFILES => {
            let hdrop = wparam as HDROP;
            let count = DragQueryFileW(hdrop, 0xFFFFFFFF, null_mut(), 0);
            let a = app();
            for i in 0..count {
                let len = DragQueryFileW(hdrop, i, null_mut(), 0);
                let mut buf = vec![0u16; len as usize + 1];
                DragQueryFileW(hdrop, i, buf.as_mut_ptr(), len + 1);
                let path = String::from_utf16_lossy(&buf[..len as usize]);
                if !a.store.shelf.contains(&path) {
                    a.store.shelf.push(path);
                }
            }
            a.shelf_items = build_shelf_items(&a.store);
            store::save(&a.store);
            DragFinish(hdrop);
            a.page = 4;
            a.expanded = true;
            apply_size(hwnd);
            0
        }
        // Mouse wheel (vertical) and touchpad swipe (horizontal) flip pages.
        // Wheel down / swipe left = next page. Precision touchpads send many
        // small deltas, so they are summed up to one wheel notch (120), with
        // a short cooldown so one swipe's momentum only flips one page.
        // Over the volume slider the wheel changes the volume instead.
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let a = app();
            if !a.expanded || a.animating {
                return 0;
            }
            let delta = ((wparam >> 16) & 0xFFFF) as i16 as i32;
            let mut p = POINT {
                x: (lparam & 0xFFFF) as i16 as i32,
                y: ((lparam >> 16) & 0xFFFF) as i16 as i32,
            };
            ScreenToClient(hwnd, &mut p);
            let (x, y) = client_to_design(hwnd, p.x, p.y);
            if msg == WM_MOUSEWHEEL && a.page == 0 && media::snapshot().present {
                let (bx, _) = vol_bar_geom();
                if y >= 133 && y < 161 && x >= bx - 8 && x < expanded_size().0 - 48 {
                    if let Some((lvl, _)) = media::volume_get() {
                        let next = (lvl + delta as f32 / 120.0 * 0.05).clamp(0.0, 1.0);
                        media::volume_set(next);
                        a.vol_level = next;
                        if a.vol_muted {
                            media::volume_toggle_mute();
                            a.vol_muted = false;
                        }
                        InvalidateRect(hwnd, null(), 0);
                    }
                    return 0;
                }
            }
            let step = if msg == WM_MOUSEWHEEL { -delta } else { delta };
            if step == 0 {
                return 0;
            }
            let now = GetTickCount64();
            if now.saturating_sub(a.last_wheel_tick) > 300 || (step > 0) != (a.wheel_acc > 0) {
                a.wheel_acc = 0; // new gesture or direction change
            }
            a.last_wheel_tick = now;
            a.wheel_acc += step;
            if a.wheel_acc.abs() >= 120 {
                let dir = a.wheel_acc.signum();
                a.wheel_acc = 0;
                if now.saturating_sub(a.last_page_tick) >= 350 {
                    a.last_page_tick = now;
                    goto_page(hwnd, step_page(dir));
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        WM_TIMER if wparam == TIMER_ANIM => {
            anim_step(hwnd);
            0
        }
        WM_TIMER if wparam == TIMER_WAVE => {
            if !app().animating {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_TIMER => {
            let a = app();
            // pomodoro countdown
            if a.pomo_running && a.pomo_left > 0 {
                a.pomo_left -= 1;
                if a.pomo_left == 0 {
                    a.pomo_running = false;
                    if a.store.sounds {
                        MessageBeep(0x40);
                    }
                    // auto-switch focus <-> break, paused, like the original
                    // cycle; every 4th break is a long one
                    if a.pomo_mode == 0 {
                        a.pomo_count += 1;
                        set_pomo_mode(if a.pomo_count % 4 == 0 { 2 } else { 1 });
                    } else {
                        set_pomo_mode(0);
                    }
                }
            }
            // hide while a full-screen app is running (games, video, slides)
            let visible = IsWindowVisible(hwnd) != 0;
            if a.store.hide_fs {
                let mut st: i32 = 0;
                let fs = SHQueryUserNotificationState(&mut st) >= 0 && matches!(st, 2 | 3 | 4);
                if fs && visible {
                    ShowWindow(hwnd, SW_HIDE);
                } else if !fs && !visible {
                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
            } else if !visible {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            // water reminders: spread the day's drinks over the active
            // hours; each one pops the notch open on the Water page
            let now = GetTickCount64();
            if a.last_water_tick == 0 {
                a.last_water_tick = now;
            }
            if a.store.reminders && a.store.water_ml < a.store.water_goal {
                let mut t: SYSTEMTIME = std::mem::zeroed();
                GetLocalTime(&mut t);
                let (from, until) = (a.store.active_from, a.store.active_until);
                let hr = t.wHour as i32;
                if until > from && hr >= from && hr < until {
                    let drinks = (a.store.water_goal / a.store.drink_ml.max(1)).max(1) as u64;
                    let every_ms = ((until - from) as u64 * 3_600_000 / drinks).max(15 * 60_000);
                    if now.saturating_sub(a.last_water_tick) >= every_ms && !a.expanded {
                        a.last_water_tick = now;
                        a.page = 3;
                        a.expanded = true;
                        a.popup_until = now + 6000;
                        if a.store.sounds {
                            MessageBeep(0x40);
                        }
                        apply_size(hwnd);
                    }
                }
            }
            // close a reminder popup the pointer never touched
            if a.popup_until > 0 && now >= a.popup_until {
                a.popup_until = 0;
                let mut p = POINT { x: 0, y: 0 };
                GetCursorPos(&mut p);
                let mut wr: RECT = std::mem::zeroed();
                GetWindowRect(hwnd, &mut wr);
                if !(p.x >= wr.left && p.x < wr.right && p.y >= wr.top && p.y < wr.bottom) {
                    collapse(hwnd);
                }
            }
            // "Charging" popup when the charger is plugged in
            if let Some((_, plugged)) = battery() {
                let ac = plugged as i8;
                if a.last_ac == 0 && ac == 1 && !a.expanded {
                    a.alert_until = now + 3800;
                }
                a.last_ac = ac;
            }
            if a.alert_until > 0 && (now >= a.alert_until || a.expanded) {
                a.alert_until = 0;
            }
            // water day rollover
            let today = today_string();
            if a.store.water_date != today {
                a.store.water_date = today;
                a.store.water_ml = 0;
                a.store.water_log.clear();
                store::save(&a.store);
            }
            if a.page == 0 && !a.vol_drag {
                if let Some((lvl, muted)) = media::volume_get() {
                    a.vol_level = lvl;
                    a.vol_muted = muted;
                }
            }
            // keep lyrics fetched for the current track
            let m = media::snapshot();
            if a.store.show_lyrics && m.present && !m.title.is_empty() {
                lyrics::ensure_fetched(&m.title, &m.artist, m.duration_secs);
            }
            // the minimized look follows playback: the mini player slides
            // in when music starts and back to the tab when it stops
            if !a.expanded && !a.animating && cur_size() != a.cur_px {
                apply_size(hwnd);
            }
            let want_wave = (!a.expanded && mini_active()) || (a.expanded && a.page == 1);
            if want_wave != a.wave_on {
                a.wave_on = want_wave;
                if want_wave {
                    SetTimer(hwnd, TIMER_WAVE, 90, None);
                } else {
                    KillTimer(hwnd, TIMER_WAVE);
                }
            }
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_RBUTTONUP => {
            DestroyWindow(hwnd);
            0
        }
        WM_KEYDOWN if wparam as u32 == 0x1B => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        media::com_init_ui();
        let mut st = store::load();
        let today = today_string();
        if st.water_date != today {
            st.water_date = today;
            st.water_ml = 0;
            st.water_log.clear();
        }
        if st.empty_shelf && !st.shelf.is_empty() {
            st.shelf.clear();
            store::save(&st);
        }
        let shelf_items = build_shelf_items(&st);
        *(&raw mut APP) = Some(App {
            hwnd: null_mut(),
            expanded: false,
            page: 0,
            hover: (0, -1),
            seek_bar: (0, 1),
            paint_k: (1.0, 1.0),
            alert_until: 0,
            last_ac: -1,
            autostart: false,
            settings_hwnd: null_mut(),
            settings_section: 0,
            set_actions: Vec::new(),
            painting_settings: false,
            cur_px: (0, 0),
            animating: false,
            anim_from: (0, 0),
            anim_to: (0, 0),
            anim_start: 0,
            anim_dur: 0,
            open_level: 0.0,
            open_from: 0.0,
            open_to: 0.0,
            wave_on: false,
            thumb_gen: 0,
            thumb_bmp: null_mut(),
            thumb_size: 0,
            tracking: false,
            fonts: Vec::new(),
            actions: Vec::new(),
            pomo_total: (st.pomo_focus * 60) as u32,
            pomo_left: (st.pomo_focus * 60) as u32,
            pomo_running: false,
            pomo_label: "Focus time".into(),
            pomo_mode: 0,
            pomo_count: 0,
            last_water_tick: 0,
            popup_until: 0,
            settings_hover: -1,
            reset_flash_until: 0,
            vol_level: 0.0,
            vol_muted: false,
            vol_drag: false,
            dots_hover: false,
            wheel_acc: 0,
            last_wheel_tick: 0,
            last_page_tick: 0,
            store: st,
            shelf_items,
            editing: false,
            edit_hwnd: null_mut(),
        });

        let hinstance = GetModuleHandleW(null());
        let class_name = wide("IsleNativeNotch");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wc);
        let set_class = wide("IsleSettings");
        let wc2 = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(settings_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: set_class.as_ptr(),
        };
        RegisterClassW(&wc2);

        let (w, h) = cur_size();
        let mon = notch_monitor();
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            wide("Isle").as_ptr(),
            WS_POPUP,
            mon.left + (mon.right - mon.left - w) / 2, mon.top + top_gap(), w, h,
            null_mut(), null_mut(), hinstance, null(),
        );
        if hwnd.is_null() {
            return;
        }
        app().hwnd = hwnd;
        app().cur_px = (w, h);
        apply_region(hwnd, w, h, collapsed_radius());
        ShowWindow(hwnd, SW_SHOW);
        media::spawn_media_thread();

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

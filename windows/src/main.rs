// Isle for Windows — native Rust v0.9.0
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
// v0.6.0: drawing moved from GDI to Direct2D + DirectWrite (gfx.rs) and the
// notch became a per-pixel-alpha layered window: anti-aliased corners and
// text like the HTML design, plus a soft green glow around the notch.

#![windows_subsystem = "windows"]

mod dnd;
mod lyrics;
mod media;
mod gfx;
mod store;
mod sysinfo;
mod translit;

use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SYSTEMTIME, WPARAM};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_LARGE_ICON, NIIF_NOSOUND, NIIF_USER, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIN_BALLOONUSERCLICK, NOTIFYICONDATAW};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, RegDeleteTreeW,
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
const A_LYRICS_EN: i32 = 85;
const A_GLOW: i32 = 86;
const A_WATER_SETTINGS: i32 = 87;
const S_MINIMIZE: i32 = 102;
const A_GLOW_COLOR: i32 = 88;
const A_GLOW_SIZE: i32 = 89;
const A_GLOW_STRENGTH: i32 = 90;
const A_BOTTLE: i32 = 91;
const A_WATER_VIEW: i32 = 92;
const A_WSET_BACK: i32 = 93;
const A_TODO_RENAME: i32 = 94;
const A_CLOSE_NOTCH: i32 = 95;
const A_TODO_COMMIT: i32 = 96;
const A_AUTO_HIDE: i32 = 97;
const A_CAL_PREV: i32 = 110;
const A_CAL_NEXT: i32 = 111;
const A_CAL_DAY: i32 = 112;
const A_CAL_ADD: i32 = 113;
const A_CAL_DEL: i32 = 114;
const A_CAL_TODAY: i32 = 115;
const A_SHOW_SYSMON: i32 = 116;
const A_SHOW_CAL: i32 = 117;
const A_INDICATORS: i32 = 118;
const A_NOTIF_POPUP: i32 = 119;
const WM_LOCKKEY: u32 = WM_APP + 3;
const WM_NETCHANGE: u32 = WM_APP + 4;
const WM_TOAST: u32 = WM_APP + 5;
const HIDE_DELAY_MS: u64 = 3000;
const SLIDE_MS: u64 = 280;
const WM_TRAY: u32 = WM_APP + 1;
const WM_SHOW_NOTCH: u32 = WM_APP + 2;
const TRAY_SHOW: usize = 1;
const TRAY_SETTINGS: usize = 2;
const TRAY_QUIT: usize = 3;

// Settings window (Nusa Settings design), in design units (96-dpi pixels).
const SET_W: i32 = 820;
const SET_H: i32 = 640;
const SIDEBAR_W: i32 = 228;
// (name, icon glyph, tile colour); Water's droplet is drawn by hand.
// About is pinned to the bottom of the sidebar.
const SECTIONS: [(&str, &str, u32); 10] = [
    ("General", "\u{e713}", cr(142, 142, 147)),
    ("Display", "\u{e7f4}", cr(94, 92, 230)),
    ("Appearance", "\u{e790}", cr(58, 58, 60)),
    ("Music and lyrics", "\u{e8d6}", cr(255, 55, 95)),
    ("To-do", "\u{e73e}", cr(48, 209, 88)),
    ("Pomodoro", "\u{e916}", cr(255, 159, 10)),
    ("Water", "", cr(10, 132, 255)),
    ("Shelf", "\u{e7b8}", cr(255, 176, 0)),
    ("Pages and alerts", "\u{ea8f}", cr(255, 69, 58)),
    ("About", "\u{e946}", cr(99, 99, 102)),
];

const PAGES: usize = 8; // 0 Music, 1 Lyrics, 2 Focus timer, 3 Water, 4 Shelf, 5 To-do, 6 System Monitor, 7 Calendar


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
    // where the notch sits in its layered window: (left, top) pixels and
    // the design -> pixel scale, for mapping the pointer
    notch_off: (i32, i32),
    notch_k: f32,
    win_pos: (i32, i32),
    presenting: bool,
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
    // which task the edit box renames (None = adding a new one)
    edit_index: Option<usize>,
    // Water page: in-notch settings view, right-panel view (gauge/hours/week)
    water_settings: bool,
    water_view: u8,
    // a file drag is over the notch, and the files it carries
    drop_hover: bool,
    drag_preview: Vec<ShelfItem>,
    // pressed on a Shelf file: (index, screen x, screen y) until it drags
    shelf_press: Option<(usize, i32, i32)>,
    // page slide
    sliding: bool,
    slide_from: usize,
    slide_dir: i32,
    slide_start: u64,
    // auto-hide: the minimized notch shows only after the top-centre hover
    revealed: bool,
    reveal_tick: u64,
    tray_icon: isize,
    // page a clicked notification opens
    notify_page: usize,
    last_own_notify: u64,
    // auto-hide: when the revealed mini notch goes away again
    hide_at: u64,
    // popups in the minimized notch: 0 charging, 1 indicator / notification
    alert_kind: u8,
    ind_glyph: String,
    ind_title: String,
    ind_sub: String,
    ind_color: u32,
    // lock keys, Wi-Fi SSID ("" = not connected), connected Bluetooth devices
    caps: bool,
    num: bool,
    scroll: bool,
    wifi: Option<String>,
    bt: Option<Vec<String>>,
    net_known: bool,
    sys: sysinfo::Reading,
    // Calendar page: shown month and selected day
    cal_y: i32,
    cal_m: i32,
    cal_sel: (i32, i32, i32),
    // what the edit box is for: 0 a task, 1 a Calendar reminder
    edit_target: u8,
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

unsafe fn draw_text(_hdc: HDC, s: &str, x: i32, y: i32, w: i32, h: i32, font_name: &str, size: i32, weight: i32, color: u32, flags: u32) {
    let align = if flags & DT_CENTER != 0 { 1 } else if flags & DT_RIGHT != 0 { 2 } else { 0 };
    gfx::text(s, x as f32, y as f32, w as f32, h as f32, font_name, size, weight, color, 1.0, align, flags & DT_END_ELLIPSIS != 0);
}

unsafe fn text_width(s: &str, font_name: &str, size: i32, weight: i32, _hdc: HDC) -> i32 {
    gfx::text_width(s, font_name, size, weight).ceil() as i32
}

unsafe fn fill_rect_c(_hdc: HDC, x: i32, y: i32, w: i32, h: i32, color: u32) {
    gfx::fill_rect(x as f32, y as f32, w as f32, h as f32, color, 1.0);
}

// `rad` is the corner's diameter (GDI RoundRect convention used across the
// layout code), so a pill passes its height.
unsafe fn fill_round(_hdc: HDC, x: i32, y: i32, w: i32, h: i32, rad: i32, color: u32) {
    gfx::fill_round(x as f32, y as f32, w as f32, h as f32, rad as f32 / 2.0, color, 1.0);
}

unsafe fn fill_circle(_hdc: HDC, cx: i32, cy: i32, r: i32, color: u32) {
    gfx::fill_circle(cx as f32, cy as f32, r as f32, color, 1.0);
}

unsafe fn draw_arc(_hdc: HDC, cx: i32, cy: i32, r: i32, from_deg: f64, sweep_deg: f64, width: i32, color: u32) {
    gfx::arc(cx as f32, cy as f32, r as f32, from_deg as f32, sweep_deg as f32, width as f32, color, 1.0);
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
    // small by default; wider only for the timer ring or the lyric line
    match mini_mode() {
        Mini::Alert => if app().alert_kind == 1 { (300, 48) } else { (330, 62) },
        Mini::Hidden => (96, 4),
        Mini::Tab => (120, 16),
        Mini::Lyric => (384, 34),
        Mini::Wide => if app().store.mini_player && music_playing() { (214, 34) } else { (186, 34) },
        Mini::Clock => if app().store.mini_player && music_playing() { (168, 34) } else { (140, 34) },
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

// Glow margin around the notch (design units): the glow size setting.
fn glow_margin() -> i32 {
    if app().store.glow { app().store.glow_size } else { 0 }
}

fn glow_color() -> u32 {
    let a = app();
    match a.store.glow_color {
        1 => cr(10, 132, 255),
        2 => cr(191, 90, 242),
        3 => cr(255, 55, 95),
        4 => cr(255, 159, 10),
        5 => cr(235, 235, 245),
        6 => page_color(a.page),
        _ => cr(48, 209, 88),
    }
}

unsafe fn place_window(_hwnd: HWND, _w: i32, _h: i32) {
    present();
}

// Repaint: the notch through UpdateLayeredWindow, other windows normally.
unsafe fn inval(h: HWND) {
    if h == app().hwnd {
        present();
    } else {
        InvalidateRect(h, null(), 0);
    }
}

// Move the notch towards the size of its current state. Size changes
// (open, close, mini player appearing) animate instead of jumping.
unsafe fn apply_size(hwnd: HWND) {
    let a = app();
    let target = cur_size();
    let open_to = if a.expanded { 1.0 } else { 0.0 };
    if target == a.cur_px && !a.animating {
        a.open_level = open_to;
        present();
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
    if a.sliding && GetTickCount64().saturating_sub(a.slide_start) >= SLIDE_MS {
        a.sliding = false;
    }
    if !a.animating {
        if !a.sliding {
            KillTimer(hwnd, TIMER_ANIM);
        }
        present();
        return;
    }
    let t = ((GetTickCount64().saturating_sub(a.anim_start)) as f32 / a.anim_dur as f32).clamp(0.0, 1.0);
    let opening = a.open_to > a.open_from || (a.anim_to.0 * a.anim_to.1 > a.anim_from.0 * a.anim_from.1);
    let e_size = if opening { ease_out_back(t) } else { ease_out_cubic(t) };
    let e_open = ease_out_cubic(t);
    let w = lerp_i(a.anim_from.0, a.anim_to.0, e_size).max(8);
    let h = lerp_i(a.anim_from.1, a.anim_to.1, e_size).max(4);
    a.open_level = a.open_from + (a.open_to - a.open_from) * e_open;
    a.cur_px = (w, h);
    if t >= 1.0 {
        a.animating = false;
        a.cur_px = a.anim_to;
        a.open_level = a.open_to;
        KillTimer(hwnd, TIMER_ANIM);
    }
    present();
}

// Draw the notch (glow, black rounded shape, the current look or the two
// looks cross-fading mid-animation) into a premultiplied-alpha bitmap and
// hand it to the layered window. Everything is anti-aliased by Direct2D.
unsafe fn present() {
    let a = app();
    if a.hwnd.is_null() || a.presenting {
        return;
    }
    a.presenting = true;
    let sc = expanded_scale() as f32 / 100.0;
    let (w, h) = a.cur_px;
    let g = if !a.expanded && !a.animating && mini_mode() == Mini::Hidden {
        (14.0 * sc).ceil() as i32 // hover area around the shadow
    } else {
        (glow_margin() as f32 * sc).ceil() as i32
    };
    let gap = top_gap();
    let (ww, wh) = (w + 2 * g, gap + h + g);
    let m = notch_monitor();
    let (wx, wy) = (m.left + (m.right - m.left - ww) / 2, m.top);
    let screen = GetDC(null_mut());
    let mem = CreateCompatibleDC(screen);
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = ww;
    bi.bmiHeader.biHeight = -wh;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut core::ffi::c_void = null_mut();
    let dib = CreateDIBSection(screen, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    let old = SelectObject(mem, dib as HGDIOBJ);
    if !dib.is_null() && gfx::begin(mem as _, ww, wh) {
        let (nx, ny) = (g as f32, gap as f32);
        let rad = if a.animating {
            lerp_i(collapsed_radius(), expanded_radius(), a.open_level.clamp(0.0, 1.0)) as f32
        } else if a.expanded {
            expanded_radius() as f32
        } else {
            collapsed_radius() as f32
        };
        let (wf, hf) = (w as f32, h as f32);
        let hidden = !a.expanded && !a.animating && mini_mode() == Mini::Hidden;
        if hidden {
            // only a small soft shadow at the top centre; every pixel of the
            // window keeps a little alpha so it still catches the pointer
            gfx::fill_rect(0.0, 0.0, ww as f32, wh as f32, BLACK, 0.004);
            let cx = ww as f32 / 2.0;
            let sw = 120.0 * sc;
            let sh = (wh as f32).min(22.0 * sc);
            for i in 0..10 {
                let k = i as f32 / 10.0;
                let (rw, rh) = (sw * (1.0 - 0.45 * k), sh * (1.0 - 0.7 * k));
                gfx::fill_round(cx - rw / 2.0, -rh, rw, rh * 2.0, rh, BLACK, 0.045);
            }
            let col = if a.store.glow { glow_color() } else { cr(160, 160, 166) };
            gfx::fill_round(cx - 18.0 * sc, 3.0 * sc, 36.0 * sc, 3.0 * sc, 1.5 * sc, col, 0.75);
            gfx::end();
            let pos = POINT { x: wx, y: wy };
            let size = windows_sys::Win32::Foundation::SIZE { cx: ww, cy: wh };
            let src = POINT { x: 0, y: 0 };
            let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            UpdateLayeredWindow(a.hwnd, screen, &pos, &size, mem, &src, 0, &bf, ULW_ALPHA);
            a.notch_off = (0, 0);
            a.notch_k = 1.0;
            a.win_pos = (wx, wy);
            SelectObject(mem, old);
            DeleteObject(dib as HGDIOBJ);
            DeleteDC(mem);
            ReleaseDC(null_mut(), screen);
            a.presenting = false;
            return;
        }
        // soft glow: stacked translucent outlines fading outwards, in the
        // chosen colour, size and strength
        if a.store.glow && g > 0 {
            let k = a.store.glow_strength as f32 / 100.0;
            let col = glow_color();
            for i in (1..=g).rev() {
                let t = i as f32 / g as f32;
                let alpha = ((0.07 * (1.0 - t) * (1.0 - t) + 0.004) * k).min(0.5);
                let fi = i as f32;
                gfx::fill_round(nx - fi, ny - fi, wf + 2.0 * fi, hf + 2.0 * fi, rad + fi, col, alpha);
            }
        }
        gfx::fill_round(nx, ny, wf, hf, rad, BLACK, 1.0);
        gfx::push_round_clip(nx, ny, wf, hf, rad);
        if !a.animating {
            let (dw, dh) = cur_design();
            let exp = a.expanded;
            gfx::transform(wf / dw as f32, hf / dh as f32, nx, ny);
            paint(mem as _, dw, dh, exp);
        } else {
            // the minimized look fades out early and the opened page fades
            // in late (or the reverse when closing), so nothing stretches
            let open = a.open_level.clamp(0.0, 1.0);
            let looks = [
                (false, if open < 0.35 { 1.0 - open / 0.35 } else { 0.0 }),
                (true, if open > 0.3 { ((open - 0.3) / 0.7).clamp(0.0, 1.0) } else { 0.0 }),
            ];
            for (exp, alpha) in looks {
                if alpha <= 0.0 {
                    continue;
                }
                let (dw, dh) = if exp { expanded_size() } else { collapsed_size() };
                let (fw, _) = px_of((dw, dh), expanded_scale());
                gfx::transform(1.0, 1.0, 0.0, 0.0);
                gfx::push_opacity(alpha);
                gfx::transform(sc, sc, nx + (w - fw) as f32 / 2.0, ny);
                paint(mem as _, dw, dh, exp);
                gfx::transform(1.0, 1.0, 0.0, 0.0);
                gfx::pop();
            }
        }
        gfx::end();
        let pos = POINT { x: wx, y: wy };
        let size = windows_sys::Win32::Foundation::SIZE { cx: ww, cy: wh };
        let src = POINT { x: 0, y: 0 };
        let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        UpdateLayeredWindow(a.hwnd, screen, &pos, &size, mem, &src, 0, &bf, ULW_ALPHA);
        a.notch_off = (g, gap);
        a.notch_k = wf / cur_design().0 as f32;
        a.win_pos = (wx, wy);
    }
    SelectObject(mem, old);
    if !dib.is_null() {
        DeleteObject(dib as HGDIOBJ);
    }
    DeleteDC(mem);
    ReleaseDC(null_mut(), screen);
    a.presenting = false;
}

// Is this client pixel on the notch itself (not its glow)?
fn in_notch(px: i32, py: i32) -> bool {
    let a = app();
    if !a.expanded && !a.animating && mini_mode() == Mini::Hidden {
        return true;
    }
    let (ox, oy) = a.notch_off;
    px >= ox && px < ox + a.cur_px.0 && py >= oy && py < oy + a.cur_px.1
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
        6 => cr(60, 198, 244),
        7 => cr(255, 69, 58),
        _ => cr(48, 209, 88),
    }
}

const PAGE_NAMES: [&str; PAGES] = ["Music", "Lyrics", "Pomodoro", "Water", "Shelf", "To-do", "System Monitor", "Calendar"];
// Page icons shown on the bottom dots while hovering them ("" = droplet).
const PAGE_ICONS: [&str; PAGES] = ["\u{e8d6}", "\u{e8e4}", "\u{e916}", "", "\u{e7b8}", "\u{e73e}", "\u{e9d9}", "\u{e787}"];

// Pages reachable with the chevrons, dots, wheel and swipe (Lyrics only
// when lyrics are turned on in Settings).
fn visible_pages() -> Vec<usize> {
    let st = &app().store;
    (0..PAGES)
        .filter(|p| match p {
            1 => st.show_lyrics,
            6 => st.show_sysmon,
            7 => st.show_calendar,
            _ => true,
        })
        .collect()
}

fn mix(a: u32, b: u32, t: f64) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xFF) as f64;
    let l = |s: u32| (ch(a, s) + (ch(b, s) - ch(a, s)) * t.clamp(0.0, 1.0)).round() as u32;
    l(0) | (l(8) << 8) | (l(16) << 16)
}

// Clip the following drawing to a rounded rectangle (design units). GDI
// clip regions are in device pixels, so the current paint scale is used.
unsafe fn clip_round(_hdc: HDC, x: i32, y: i32, w: i32, h: i32, rad: i32) {
    gfx::push_round_clip(x as f32, y as f32, w as f32, h as f32, rad as f32);
}

unsafe fn unclip(_hdc: HDC) {
    gfx::pop();
}

fn pts_f(pts: &[POINT]) -> Vec<(f32, f32)> {
    pts.iter().map(|p| (p.x as f32, p.y as f32)).collect()
}

unsafe fn polygon(_hdc: HDC, pts: &[POINT], col: u32) {
    gfx::polygon(&pts_f(pts), col, 1.0);
}

// Stroked polyline with round caps (chevrons, check marks).
unsafe fn stroke(_hdc: HDC, pts: &[POINT], width: i32, col: u32) {
    gfx::polyline(&pts_f(pts), width as f32, col, 1.0);
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
            ring(hdc, cx, cy, 7, 2, pct as f64 / 100.0, cr(31, 58, 40), if low { cr(255, 69, 58) } else { cr(48, 209, 88) });
            if plugged {
                polygon(hdc, &[
                    POINT { x: cx + 1, y: cy - 4 },
                    POINT { x: cx - 3, y: cy + 1 },
                    POINT { x: cx - 1, y: cy + 1 },
                    POINT { x: cx - 1, y: cy + 4 },
                    POINT { x: cx + 3, y: cy - 1 },
                    POINT { x: cx + 1, y: cy - 1 },
                ], cr(48, 209, 88));
            }
        }
        None => {
            fill_circle(hdc, cx, cy, 5, cr(12, 52, 24));
            fill_circle(hdc, cx, cy, 3, cr(48, 209, 88));
        }
    }
}

// Battery ring + "12:19 PM" (+ date), centred on cx.
unsafe fn draw_clock(hdc: HDC, cx: i32, cy: i32, with_date: bool) {
    let (t, ap, date) = time_parts();
    let tw = text_width(&t, "Segoe UI", 15, 600, hdc);
    let aw = text_width(&ap, "Segoe UI", 11, 600, hdc);
    let dw = if with_date { 10 + text_width(&date, "Segoe UI", 12, 400, hdc) } else { 0 };
    let total = 18 + 7 + tw + 2 + aw + dw;
    let x = cx - total / 2;
    draw_indicator(hdc, x + 9, cy);
    let mut tx = x + 25;
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
        let bh = 4.0 + 11.0 * k as f32;
        gfx::fill_round((x + i as i32 * 5) as f32, cy as f32 - bh / 2.0, 3.0, bh, 1.5, cr(255, 55, 95), 1.0);
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
    if page == 3 && a.water_settings {
        let hov = a.hover.0 == A_WSET_BACK;
        fill_circle(hdc, 36, 22, 12, if hov { cr(62, 62, 66) } else { cr(40, 40, 44) });
        stroke(hdc, &[POINT { x: 38, y: 17 }, POINT { x: 33, y: 22 }, POINT { x: 38, y: 27 }], 2, WHITE);
        add_action(20, 6, 110, 32, A_WSET_BACK, 0);
        draw_text(hdc, "Settings", 56, 0, 120, 44, "Segoe UI", 15, 700, WHITE, DT_LEFT);
    } else if page == 5 {
        let ww = text_width("Todo", "Segoe UI", 15, 700, hdc);
        draw_text(hdc, "Todo", 24, 0, ww + 2, 44, "Segoe UI", 15, 700, WHITE, DT_LEFT);
        draw_text(hdc, "List", 24 + ww, 0, 80, 44, "Segoe UI", 15, 700, page_color(5), DT_LEFT);
    } else if page == 3 {
        let ww = text_width("Water ", "Segoe UI", 15, 700, hdc);
        draw_text(hdc, "Water ", 24, 0, ww + 2, 44, "Segoe UI", 15, 700, WHITE, DT_LEFT);
        draw_text(hdc, "Reminder", 24 + ww, 0, 120, 44, "Segoe UI", 15, 700, BLUE, DT_LEFT);
    } else {
        draw_text(hdc, PAGE_NAMES[page], 24, 0, 160, 44, "Segoe UI", 15, 700, page_color(page), DT_LEFT);
    }
    draw_clock(hdc, w / 2, 22, true);
    // page (sliding in from the side after a page change)
    if a.sliding {
        let t = (GetTickCount64().saturating_sub(a.slide_start) as f32 / SLIDE_MS as f32).clamp(0.0, 1.0);
        let e = ease_out_cubic(t);
        let d = a.slide_dir as f32 * w as f32;
        let old = gfx::shift_x(-d * e);
        gfx::push_opacity(1.0 - e * 0.6);
        paint_page(hdc, w, h, a.slide_from);
        gfx::pop();
        gfx::restore(old);
        let old = gfx::shift_x(d * (1.0 - e));
        paint_page(hdc, w, h, page);
        gfx::restore(old);
        // only the new page's buttons count, and only once it is in place
        a.actions.clear();
    } else {
        paint_page(hdc, w, h, page);
    }
    // status icons in the header, right to left before the close button
    {
        let mut ix = w - 52;
        let blue = cr(10, 132, 255);
        let icon = |ix: &mut i32, glyph: &str, col: u32| {
            *ix -= 22;
            draw_text(hdc, glyph, *ix, 12, 20, 20, "Segoe MDL2 Assets", 12, 400, col, DT_CENTER);
        };
        let badge = |ix: &mut i32, t: &str| {
            *ix -= 22;
            gfx::fill_round(*ix as f32 + 2.0, 13.0, 18.0, 18.0, 5.0, cr(48, 48, 52), 1.0);
            draw_text(hdc, t, *ix + 2, 13, 18, 18, "Segoe UI", 10, 700, WHITE, DT_CENTER);
        };
        match a.wifi.as_deref() {
            Some("") => icon(&mut ix, "\u{eb5e}", cr(110, 110, 115)),
            Some(_) => icon(&mut ix, "\u{e701}", WHITE),
            None => {}
        }
        if let Some(bt) = &a.bt {
            icon(&mut ix, "\u{e702}", if bt.is_empty() { cr(110, 110, 115) } else { blue });
        }
        if a.num {
            badge(&mut ix, "1");
        }
        if a.caps {
            badge(&mut ix, "A");
        }
    }
    // close button (top right): back to the minimized / hidden notch

    let hov = a.hover.0 == A_CLOSE_NOTCH;
    fill_circle(hdc, w - 30, 22, 11, if hov { cr(62, 62, 66) } else { cr(32, 32, 35) });
    stroke(hdc, &[POINT { x: w - 34, y: 18 }, POINT { x: w - 26, y: 26 }], 2, cr(209, 209, 214));
    stroke(hdc, &[POINT { x: w - 34, y: 26 }, POINT { x: w - 26, y: 18 }], 2, cr(209, 209, 214));
    add_action(w - 44, 8, 28, 28, A_CLOSE_NOTCH, 0);
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

unsafe fn paint_page(hdc: HDC, w: i32, h: i32, page: usize) {
    match page {
        0 => paint_music(hdc, w, h),
        1 => paint_lyrics(hdc, w, h),
        2 => paint_pomo(hdc, w, h),
        3 => paint_water(hdc, w, h),
        4 => paint_shelf(hdc, w, h),
        5 => paint_todo(hdc, w, h),
        6 => paint_sysmon(hdc, w, h),
        _ => paint_calendar(hdc, w, h),
    }
}

// System Monitor (as on the GNOME page): CPU, RAM, GPU on the left;
// network up/down and the system drive on the right.
unsafe fn sys_tile(hdc: HDC, x: i32, y: i32, w: i32, h: i32, tag: &str, color: u32, label: &str, value: &str, bar: Option<f64>) {
    gfx::fill_round(x as f32, y as f32, w as f32, h as f32, 12.0, cr(28, 28, 31), 1.0);
    gfx::fill_round((x + 9) as f32, (y + 9) as f32, 28.0, 28.0, 8.0, mix(color, BLACK, 0.72), 1.0);
    draw_text(hdc, tag, x + 9, y + 9, 28, 28, "Segoe UI", 9, 700, color, DT_CENTER);
    draw_text(hdc, label, x + 46, y + 6, w - 56, 14, "Segoe UI", 9, 600, cr(142, 142, 147), DT_LEFT);
    draw_text(hdc, value, x + 46, y + 18, w - 56, 18, "Segoe UI", 13, 600, WHITE, DT_LEFT | DT_END_ELLIPSIS);
    if let Some(f) = bar {
        let bw = (w - 58) as f32;
        gfx::fill_round((x + 46) as f32, (y + 37) as f32, bw, 4.0, 2.0, cr(52, 52, 56), 1.0);
        gfx::fill_round((x + 46) as f32, (y + 37) as f32, (bw * (f / 100.0).clamp(0.0, 1.0) as f32).max(4.0), 4.0, 2.0, color, 1.0);
    }
}

unsafe fn paint_sysmon(hdc: HDC, w: i32, _h: i32) {
    let r = app().sys.clone();
    let (cyan, amber, red) = (cr(60, 198, 244), cr(240, 180, 41), cr(239, 83, 80));
    let gb = |b: u64| b as f64 / 1073741824.0;
    let (x0, y0, gap, th) = (40, 48, 6, 45);
    let cw = (w - 80 - 10) / 2;
    let x1 = x0 + cw + 10;
    let ram_pct = if r.ram_total > 0 { 100.0 * r.ram_used as f64 / r.ram_total as f64 } else { 0.0 };
    let disk_pct = if r.disk_total > 0 { 100.0 * r.disk_used as f64 / r.disk_total as f64 } else { 0.0 };
    sys_tile(hdc, x0, y0, cw, th, "CPU", cyan, "CPU", &format!("{:.0}%", r.cpu), Some(r.cpu));
    sys_tile(hdc, x0, y0 + th + gap, cw, th, "RAM", amber, "RAM", &format!("{:.1} / {:.1} GB", gb(r.ram_used), gb(r.ram_total)), Some(ram_pct));
    let gpu = r.gpu.map(|g| format!("{:.0}%", g)).unwrap_or_else(|| "N/A".into());
    sys_tile(hdc, x0, y0 + 2 * (th + gap), cw, th, "GPU", red, "GPU", &gpu, Some(r.gpu.unwrap_or(0.0)));
    sys_tile(hdc, x1, y0, cw, th, "\u{2191}", cyan, "NET UP", &sysinfo::fmt_rate(r.up), None);
    sys_tile(hdc, x1, y0 + th + gap, cw, th, "\u{2193}", cyan, "NET DOWN", &sysinfo::fmt_rate(r.down), None);
    sys_tile(hdc, x1, y0 + 2 * (th + gap), cw, th, "SSD", cyan, "DISK", &format!("{:.0}% \u{00b7} {:.0} / {:.0} GB", disk_pct, gb(r.disk_used), gb(r.disk_total)), Some(disk_pct));
}

// Calendar: the selected date on the left, the month grid in the middle,
// that day's reminders (with + Add) on the right.
const CAL_LIST: (i32, i32, i32) = (344, 78, 192); // x, y, width
const CAL_ROW_H: i32 = 26;

fn cal_events(key: &str) -> Vec<usize> {
    let a = app();
    let mut v: Vec<usize> = (0..a.store.events.len()).filter(|i| a.store.events[*i].date == key).collect();
    v.sort_by_key(|i| a.store.events[*i].min);
    v
}

fn cal_edit_slot() -> (i32, i32, i32, i32) {
    let (sy, sm, sd) = app().cal_sel;
    let n = cal_events(&date_key(sy, sm, sd)).len().min(3) as i32;
    let (x, y, w) = CAL_LIST;
    (x, y + n * CAL_ROW_H, w, CAL_ROW_H - 2)
}

unsafe fn draw_field(hdc: HDC, slot: (i32, i32, i32, i32)) {
    let a = app();
    let (fx, fy, fw, fh) = slot;
    gfx::fill_round(fx as f32, fy as f32, fw as f32, fh as f32, fh as f32 / 2.0, FIELD_BG, 1.0);
    gfx::stroke_round(fx as f32 + 0.75, fy as f32 + 0.75, fw as f32 - 1.5, fh as f32 - 1.5, fh as f32 / 2.0, 1.5, false, BLUE, 1.0);
    let (bx, by) = (fx + fw - fh / 2 - 1, fy + fh / 2);
    let hov = a.hover.0 == A_TODO_COMMIT;
    fill_circle(hdc, bx, by, fh / 2 - 3, if hov { cr(64, 156, 255) } else { BLUE });
    stroke(hdc, &[POINT { x: bx - 4, y: by }, POINT { x: bx - 1, y: by + 3 }, POINT { x: bx + 4, y: by - 3 }], 2, WHITE);
    add_action(bx - fh / 2, fy, fh, fh, A_TODO_COMMIT, 0);
}

unsafe fn paint_calendar(hdc: HDC, _w: i32, _h: i32) {
    let a = app();
    let grey = cr(142, 142, 147);
    let red = page_color(7);
    let (ty, tm, td) = today_ymd();
    let (sy, sm, sd) = a.cal_sel;
    // left: selected date
    let lx = 40;
    draw_text(hdc, &MONTHS[(sm - 1) as usize].to_uppercase(), lx, 56, 100, 16, "Segoe UI", 11, 700, red, DT_LEFT);
    draw_text(hdc, WEEKDAYS[weekday(sy, sm, sd) as usize], lx, 72, 100, 20, "Segoe UI", 14, 600, WHITE, DT_LEFT);
    draw_text(hdc, &sd.to_string(), lx - 2, 90, 100, 60, "Segoe UI", 46, 700, WHITE, DT_LEFT);
    if (sy, sm, sd) != (ty, tm, td) {
        let hov = a.hover.0 == A_CAL_TODAY;
        pill_btn(hdc, lx, 156, "Today", 11, 600, 10, 22, if hov { cr(60, 60, 64) } else { cr(40, 40, 44) }, WHITE, A_CAL_TODAY, 0);
    }
    // middle: month grid
    let gx = 146;
    let cw = 26;
    let gw = cw * 7;
    let hov_p = a.hover.0 == A_CAL_PREV;
    let hov_n = a.hover.0 == A_CAL_NEXT;
    draw_text(hdc, "\u{e76b}", gx, 46, 20, 20, "Segoe MDL2 Assets", 9, 400, if hov_p { WHITE } else { grey }, DT_CENTER);
    add_action(gx - 4, 44, 28, 24, A_CAL_PREV, 0);
    draw_text(hdc, "\u{e76c}", gx + gw - 20, 46, 20, 20, "Segoe MDL2 Assets", 9, 400, if hov_n { WHITE } else { grey }, DT_CENTER);
    add_action(gx + gw - 24, 44, 28, 24, A_CAL_NEXT, 0);
    draw_text(hdc, &format!("{} {}", MONTHS[(a.cal_m - 1) as usize], a.cal_y), gx + 20, 46, gw - 40, 20, "Segoe UI", 12, 600, WHITE, DT_CENTER);
    for (i, dch) in ["S", "M", "T", "W", "T", "F", "S"].iter().enumerate() {
        draw_text(hdc, dch, gx + i as i32 * cw, 66, cw, 16, "Segoe UI", 9, 600, grey, DT_CENTER);
    }
    let first = weekday(a.cal_y, a.cal_m, 1);
    let nd = days_in_month(a.cal_y, a.cal_m);
    for d in 1..=nd {
        let idx = first + d - 1;
        let (col, row) = (idx % 7, idx / 7);
        let cx = gx + col * cw + cw / 2;
        let cy = 90 + row * 18;
        let is_today = (a.cal_y, a.cal_m, d) == (ty, tm, td);
        let is_sel = (a.cal_y, a.cal_m, d) == (sy, sm, sd);
        let hov = a.hover.0 == A_CAL_DAY && a.hover.1 == d;
        if is_today {
            fill_circle(hdc, cx, cy, 8, red);
        } else if is_sel {
            fill_circle(hdc, cx, cy, 8, cr(64, 64, 70));
        } else if hov {
            fill_circle(hdc, cx, cy, 8, cr(44, 44, 48));
        }
        if is_sel && is_today {
            gfx::stroke_round((cx - 9) as f32, (cy - 9) as f32, 18.0, 18.0, 9.0, 1.2, false, WHITE, 1.0);
        }
        let fg = if is_today || is_sel { WHITE } else if col == 0 { cr(200, 120, 120) } else { cr(220, 220, 225) };
        draw_text(hdc, &d.to_string(), cx - 12, cy - 8, 24, 16, "Segoe UI", 10, if is_today { 700 } else { 500 }, fg, DT_CENTER);
        if !is_today && a.store.events.iter().any(|e| e.date == date_key(a.cal_y, a.cal_m, d)) {
            fill_circle(hdc, cx, cy + 8, 1, red);
        }
        add_action(cx - cw / 2, cy - 9, cw, 18, A_CAL_DAY, d);
    }
    // right: reminders for the selected day
    let (rx, ry, rw) = CAL_LIST;
    draw_text(hdc, "Reminders", rx, 46, 120, 22, "Segoe UI", 12, 600, grey, DT_LEFT);
    let hov_a = a.hover.0 == A_CAL_ADD;
    let aw = 58;
    fill_round(hdc, rx + rw - aw, 46, aw, 22, 22, if hov_a { cr(255, 99, 88) } else { red });
    draw_text(hdc, "+ Add", rx + rw - aw, 46, aw, 22, "Segoe UI", 11, 600, WHITE, DT_CENTER);
    add_action(rx + rw - aw, 46, aw, 22, A_CAL_ADD, 0);
    let key = date_key(sy, sm, sd);
    let list = cal_events(&key);
    let edit_box = !a.edit_hwnd.is_null() && IsWindowVisible(a.edit_hwnd) != 0 && a.edit_target == 1;
    if list.is_empty() && !edit_box {
        draw_text(hdc, "No reminders", rx, ry + 14, rw, 18, "Segoe UI", 12, 500, cr(110, 110, 115), DT_CENTER);
        draw_text(hdc, "Add one with a time, like 14:30 Call", rx, ry + 34, rw, 16, "Segoe UI", 10, 400, cr(90, 90, 95), DT_CENTER);
    }
    let max_rows = if edit_box { 3 } else { 4 };
    for (row, ei) in list.iter().take(max_rows).enumerate() {
        let e = &a.store.events[*ei];
        let y = ry + row as i32 * CAL_ROW_H;
        let hov = a.hover.1 == *ei as i32 && a.hover.0 == A_CAL_DEL;
        gfx::fill_round(rx as f32, y as f32, rw as f32, (CAL_ROW_H - 4) as f32, 8.0, cr(28, 28, 31), 1.0);
        draw_text(hdc, &fmt_min(e.min), rx + 8, y, 50, CAL_ROW_H - 4, "Segoe UI", 10, 700, if e.fired { grey } else { red }, DT_LEFT);
        draw_text(hdc, &e.title.clone(), rx + 58, y, rw - 58 - 26, CAL_ROW_H - 4, "Segoe UI", 12, 500, if e.fired { grey } else { WHITE }, DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
        draw_text(hdc, "\u{2715}", rx + rw - 24, y, 20, CAL_ROW_H - 4, "Segoe UI", 9, 600, if hov { WHITE } else { cr(90, 90, 95) }, DT_CENTER);
        add_action(rx + rw - 26, y, 26, CAL_ROW_H - 4, A_CAL_DEL, *ei as i32);
    }
    if list.len() > max_rows {
        draw_text(hdc, &format!("+{} more", list.len() - max_rows), rx, ry + max_rows as i32 * CAL_ROW_H, rw, 16, "Segoe UI", 10, 500, grey, DT_CENTER);
    }
    if edit_box {
        draw_field(hdc, cal_edit_slot());
    }
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
    Hidden,
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
    // auto-hide: stay hidden unless something is going on (a lyric line,
    // music in the mini player, a running timer)
    let busy = mini_lyric_line().is_some() || a.pomo_running || (a.store.mini_player && music_playing());
    if a.store.auto_hide && !a.revealed && a.popup_until == 0 && a.shelf_press.is_none() && !busy {
        return Mini::Hidden;
    }
    let playing = a.store.mini_player && music_playing();
    if mini_lyric_line().is_some() {
        return Mini::Lyric;
    }
    if a.pomo_running {
        return Mini::Wide;
    }
    if playing {
        return Mini::Clock;
    }
    if a.store.tiny_tab {
        return Mini::Tab;
    }
    Mini::Clock
}

fn mini_active() -> bool {
    matches!(mini_mode(), Mini::Lyric | Mini::Wide) || (app().store.mini_player && music_playing())
}

unsafe fn paint_collapsed(hdc: HDC, w: i32, h: i32) {
    let a = app();
    let cy = h / 2;
    match mini_mode() {
        Mini::Alert if a.alert_kind == 1 => {
            // indicator / notification popup: round icon, title, subtitle
            fill_circle(hdc, 28, cy, 14, mix(a.ind_color, BLACK, 0.75));
            draw_text(hdc, &a.ind_glyph.clone(), 14, cy - 14, 28, 28, "Segoe MDL2 Assets", 13, 400, a.ind_color, DT_CENTER);
            if a.ind_sub.is_empty() {
                draw_text(hdc, &a.ind_title.clone(), 52, 0, w - 70, h, "Segoe UI", 13, 600, WHITE, DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
            } else {
                draw_text(hdc, &a.ind_title.clone(), 52, cy - 17, w - 70, 18, "Segoe UI", 13, 600, WHITE, DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
                draw_text(hdc, &a.ind_sub.clone(), 52, cy + 1, w - 70, 16, "Segoe UI", 11, 400, cr(161, 161, 166), DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
            }
        }
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
        Mini::Hidden => {}
        mode => {
            let ring_on = a.pomo_running && mode != Mini::Lyric;
            // the wave always shows with the lyric line; next to the clock
            // it follows "Music in the minimized notch"
            let wave_on = music_playing() && (mode == Mini::Lyric || a.store.mini_player);
            if ring_on {
                ring(hdc, 22, cy, 8, 3, pomo_frac(), cr(58, 42, 16), cr(255, 159, 10));
            }
            if mode == Mini::Lyric {
                // wave on the left, the line centred in the rest
                draw_wave(hdc, 18, cy);
                let line = mini_lyric_line().unwrap_or_default();
                draw_text(hdc, &line, 46, 0, w - 46 - 22, h, "Segoe UI", 13, 500, WHITE, DT_CENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
            } else if wave_on {
                draw_wave(hdc, w - 16 - 18, cy);
            }
            if mode != Mini::Lyric {
                let l = if ring_on { 36 } else { 0 };
                let r = if wave_on { 34 } else { 0 };
                draw_clock(hdc, (l + w - r) / 2, cy, false);
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

// Album art with rounded corners: the real thumbnail when the media session
// has one, otherwise the design's peach -> lilac -> sky gradient with a note.
unsafe fn draw_art(hdc: HDC, x: i32, y: i32, size: i32, rad: i32, note: bool) {
    clip_round(hdc, x, y, size, size, rad);
    let drawn = match media::THUMB.lock() {
        Ok(t) if t.size > 0 && !t.bgra.is_empty() => {
            gfx::image(t.gen, t.size, &t.bgra, x as f32, y as f32, size as f32, size as f32);
            true
        }
        _ => false,
    };
    if drawn {
        unclip(hdc);
        return;
    }
    // 135deg gradient as diagonal bands
    let (c1, c2, c3) = (cr(255, 154, 139), cr(161, 140, 209), cr(90, 200, 250));
    let n = 40;
    for i in 0..n {
        let t = (i as f64 + 0.5) / n as f64;
        let col = if t < 0.6 { mix(c1, c2, t / 0.6) } else { mix(c2, c3, (t - 0.6) / 0.4) };
        let d0 = 2.0 * size as f32 * i as f32 / n as f32;
        let d1 = 2.0 * size as f32 * (i + 1) as f32 / n as f32 + 0.8;
        let (xf, yf) = (x as f32, y as f32);
        gfx::polygon(&[(xf + d0, yf), (xf + d1, yf), (xf, yf + d1), (xf, yf + d0)], col, 1.0);
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
    let show_slider = a.vol_drag || a.hover.0 == A_VOL_MUTE || a.hover.0 == A_VOL_BAR;
    if !show_slider {
        btn(hdc, mid + 57, "\u{e893}", 15, 28, A_NEXT_TRACK);
    }
    // lyrics button: opens the Lyrics page
    if a.store.show_lyrics {
        let lcx = cx0 + 14;
        if a.hover.0 == A_LYRICS {
            fill_circle(hdc, lcx, cy, 14, cr(44, 44, 46));
        }
        draw_text(hdc, "\u{e8f2}", lcx - 14, cy - 14, 28, 28, "Segoe MDL2 Assets", 15, 400, if m.present { WHITE } else { cr(99, 99, 108) }, DT_CENTER);
        add_action(lcx - 14, cy - 14, 28, 28, A_LYRICS, 0);
    }
    // volume: speaker button on the right; the slider slides out on hover
    let (lvl, muted) = (a.vol_level, a.vol_muted);
    let silent = muted || lvl <= 0.0;
    let vx = w - 48 - 14;
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

// Pomodoro in the style Ram picked: a glowing red stopwatch disc on the
// left; big time + reset, the phase, then Short Break / play / Focus Time.
unsafe fn paint_pomo(hdc: HDC, w: i32, _h: i32) {
    let a = app();
    let (dcx, dcy) = (48 + 58, PMID);
    fill_circle(hdc, dcx, dcy, 58, cr(46, 10, 9));
    fill_circle(hdc, dcx, dcy, 54, cr(110, 22, 18));
    fill_circle(hdc, dcx, dcy, 50, cr(255, 82, 72));
    fill_circle(hdc, dcx, dcy, 43, cr(214, 30, 26));
    draw_text(hdc, "\u{e916}", dcx - 30, dcy - 30, 60, 60, "Segoe MDL2 Assets", 38, 400, WHITE, DT_CENTER);
    // right column
    let rx0 = 48 + 116 + 22;
    let rcx = (rx0 + w - 48) / 2;
    let t = fmt_mmss(a.pomo_left as f64);
    let tw = text_width(&t, "Segoe UI", 40, 700, hdc);
    let tx = rcx - (tw + 40) / 2;
    draw_text(hdc, &t, tx, 54, tw + 6, 50, "Segoe UI", 40, 700, WHITE, DT_LEFT);
    let (bcx, bcy) = (tx + tw + 26, 80);
    fill_circle(hdc, bcx, bcy, 14, if a.hover.0 == A_POMO_RESET { cr(58, 58, 62) } else { cr(38, 38, 42) });
    draw_text(hdc, "\u{e777}", bcx - 14, bcy - 14, 28, 28, "Segoe MDL2 Assets", 13, 400, WHITE, DT_CENTER);
    add_action(bcx - 15, bcy - 15, 30, 30, A_POMO_RESET, 0);
    let label = a.pomo_label.clone();
    draw_text(hdc, &label, rx0, 104, w - 48 - rx0, 20, "Segoe UI", 14, 400, cr(161, 161, 166), DT_CENTER);
    // buttons
    let y = 134;
    let (bw, pw, gap) = (120, 64, 8);
    let x0 = rcx - (bw * 2 + pw + gap * 2) / 2;
    let pill = |hdc: HDC, x: i32, wdt: i32, icon: &str, text: &str, on: bool, param: i32| {
        let hov = app().hover == (A_POMO_MODE, param);
        let bg = if on { cr(58, 58, 64) } else if hov { cr(48, 48, 52) } else { cr(38, 38, 42) };
        fill_round(hdc, x, y, wdt, 34, 34, bg);
        draw_text(hdc, icon, x + 10, y, 22, 34, "Segoe UI Symbol", 13, 400, WHITE, DT_CENTER);
        draw_text(hdc, text, x + 32, y, wdt - 40, 34, "Segoe UI", 13, 500, cr(232, 232, 236), DT_LEFT);
        add_action(x, y, wdt, 34, A_POMO_MODE, param);
    };
    pill(hdc, x0, bw, "\u{2615}", "Short Break", a.pomo_mode != 0, 5);
    let px = x0 + bw + gap;
    let red = if a.hover.0 == A_POMO_PLAY { cr(255, 99, 88) } else { cr(255, 69, 58) };
    fill_round(hdc, px, y, pw, 34, 34, red);
    draw_text(hdc, if a.pomo_running { "\u{e769}" } else { "\u{e768}" }, px, y, pw, 34, "Segoe MDL2 Assets", 15, 400, WHITE, DT_CENTER);
    add_action(px, y, pw, 34, A_POMO_PLAY, 0);
    pill(hdc, px + pw + gap, bw, "\u{25ce}", "Focus Time", a.pomo_mode == 0, 25);
}

// Water Reminder (Ram's reference): litres drunk / goal with the reminder
// bell switch, a progress line, Drink + Settings; a gauge on the right.
// Water plan from the design: a sip reminder every 15 minutes across the
// active hours; each sip is the goal shared out over those reminders.
fn water_plan() -> (i32, i32) {
    let st = &app().store;
    let hours = (st.active_until - st.active_from).max(0);
    let reminders = hours * 4;
    let sip = if reminders > 0 { ((st.water_goal as f64 / reminders as f64).round() as i32).max(10) } else { st.drink_ml.max(10) };
    (reminders, sip)
}

// Days since 1970-01-01 <-> (y, m, d) (proleptic Gregorian).
fn days_from_civil(y: i32, m: i32, d: i32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i32, i32, i32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as i32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as i32;
    let y = (yoe + era * 400) as i32 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

// The last seven days, oldest first: (weekday letter, ml, is today).
fn water_week() -> Vec<(&'static str, i32, bool)> {
    let st = &app().store;
    let t = today_string();
    let parts: Vec<i32> = t.split('-').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 3 {
        return Vec::new();
    }
    let today = days_from_civil(parts[0], parts[1], parts[2]);
    const WD: [&str; 7] = ["Th", "Fr", "Sa", "Su", "Mo", "Tu", "We"]; // 1970-01-01 was a Thursday
    (0..7)
        .map(|i| {
            let day = today - 6 + i;
            let (y, m, d) = civil_from_days(day);
            let key = format!("{:04}-{:02}-{:02}", y, m, d);
            let ml = if i == 6 { st.water_ml } else { st.water_hist.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or(0) };
            (WD[day.rem_euclid(7) as usize], ml, i == 6)
        })
        .collect()
}

unsafe fn paint_water(hdc: HDC, w: i32, h: i32) {
    if app().water_settings {
        paint_water_settings(hdc, w, h);
        return;
    }
    let a = app();
    let ml = a.store.water_ml;
    let goal = a.store.water_goal.max(1);
    let frac = (ml as f64 / goal as f64).clamp(0.0, 1.0);
    let pct = (frac * 100.0).round() as i32;
    let grey = cr(161, 161, 166);
    let x0 = 48;
    let lw = 236; // left column width
    // 0.0 / 1.5 L
    let amt = format!("{:.1}", ml as f64 / 1000.0);
    let aw = text_width(&amt, "Segoe UI", 34, 700, hdc);
    draw_text(hdc, &amt, x0, 52, aw + 4, 46, "Segoe UI", 34, 700, WHITE, DT_LEFT);
    let g = format!("{:.1}", goal as f64 / 1000.0);
    draw_text(hdc, &format!("/ {} L", g), x0 + aw + 6, 64, 90, 30, "Segoe UI", 15, 500, grey, DT_LEFT);
    // reminders switch with a bell knob
    let on = a.store.reminders;
    let (sx, sy, sw, sh) = (x0 + lw - 44, 62, 44, 26);
    fill_round(hdc, sx, sy, sw, sh, sh, if on { BLUE } else { cr(58, 58, 62) });
    let kx = if on { sx + sw - 13 } else { sx + 13 };
    fill_circle(hdc, kx, sy + 13, 10, WHITE);
    draw_text(hdc, "\u{ea8f}", kx - 10, sy + 3, 20, 20, "Segoe MDL2 Assets", 10, 400, cr(30, 30, 32), DT_CENTER);
    add_action(sx, sy, sw, sh, A_WATER_BELL, 0);
    // Today · 0% · Reminders off
    let status = format!("Today \u{00b7} {}% \u{00b7} Reminders {}", pct, if on { "on" } else { "off" });
    draw_text(hdc, &status, x0, 98, lw, 18, "Segoe UI", 12, 400, grey, DT_LEFT);
    // progress line
    fill_round(hdc, x0, 121, lw, 5, 5, cr(44, 44, 48));
    if frac > 0.0 {
        gfx::fill_round(x0 as f32, 121.0, (lw as f32 * frac as f32).max(5.0), 5.0, 2.5, BLUE, 1.0);
    }
    // Drink + Settings
    let by = 140;
    let (_, sip) = water_plan();
    let full = ml >= goal;
    let hov_d = a.hover.0 == A_DRINK && !full;
    fill_round(hdc, x0, by, 82, 34, 34, if hov_d { cr(18, 58, 104) } else { cr(12, 42, 78) });
    let dc = if full { cr(60, 90, 130) } else { BLUE };
    draw_droplet(hdc, x0 + 22, by + 17, dc);
    draw_text(hdc, "Drink", x0 + 34, by, 44, 34, "Segoe UI", 13, 600, dc, DT_LEFT);
    add_action(x0, by, 82, 34, A_DRINK, 0);
    let _ = sip;
    let sx2 = x0 + 92;
    let hov_s = a.hover.0 == A_WATER_SETTINGS;
    fill_round(hdc, sx2, by, 98, 34, 34, if hov_s { cr(52, 52, 56) } else { cr(38, 38, 42) });
    draw_text(hdc, "\u{e9e9}", sx2 + 10, by, 22, 34, "Segoe MDL2 Assets", 13, 400, WHITE, DT_CENTER);
    draw_text(hdc, "Settings", sx2 + 34, by, 60, 34, "Segoe UI", 13, 600, WHITE, DT_LEFT);
    add_action(sx2, by, 98, 34, A_WATER_SETTINGS, 0);
    // divider
    let px0 = x0 + lw + 18;
    gfx::fill_rect(px0 as f32, 58.0, 1.0, 118.0, cr(48, 48, 52), 1.0);
    // right panel: gauge / today by hour / this week, picked with the dots
    let (rx, rr) = (px0 + 14, w - 48);
    add_action(rx, 58, rr - rx, 122, A_WATER_VIEW, (a.water_view as i32 + 1) % 3);
    for i in 0..3 {
        let on = a.water_view as i32 == i;
        let dx = rr - 34 + i * 12;
        let hov = a.hover == (A_WATER_VIEW, i) ;
        gfx::fill_circle(dx as f32 + 3.0, 52.0, if on { 3.2 } else { 2.6 }, if on || hov { WHITE } else { cr(84, 84, 90) }, 1.0);
        add_action(dx - 3, 44, 12, 16, A_WATER_VIEW, i);
    }
    match a.water_view {
        1 => water_chart_hours(hdc, rx, rr),
        2 => water_chart_week(hdc, rx, rr),
        _ => {
            // gauge: 270 degrees open at the bottom
            let gx = (rx + rr) / 2;
            let gy = 118;
            let r = 48;
            draw_arc(hdc, gx, gy, r, 135.0, 270.0, 9, cr(40, 40, 44));
            if frac > 0.0 {
                draw_arc(hdc, gx, gy, r, 135.0, 270.0 * frac, 9, BLUE);
            }
            let end = (135.0 + 270.0 * frac).to_radians();
            let (ex, ey) = (gx as f64 + r as f64 * end.cos(), gy as f64 + r as f64 * end.sin());
            gfx::fill_circle(ex as f32, ey as f32, 5.5, if frac > 0.0 { WHITE } else { cr(120, 120, 126) }, 1.0);
            draw_text(hdc, &pct.to_string(), gx - 40, gy - 24, 80, 40, "Segoe UI", 30, 700, WHITE, DT_CENTER);
            draw_droplet(hdc, gx - 7, gy + 22, cr(120, 120, 126));
            draw_text(hdc, "%", gx - 1, gy + 14, 20, 18, "Segoe UI", 12, 600, cr(161, 161, 166), DT_LEFT);
        }
    }
}

// Bar chart frame shared by the two charts: value labels on the right.
unsafe fn water_bars(hdc: HDC, rx: i32, rr: i32, vals: &[(String, i32, bool)], top: i32, label: &dyn Fn(i32) -> String, goal_line: Option<i32>) {
    let grey = cr(142, 142, 147);
    let (py0, py1) = (66, 158); // plot top / bottom
    let lab_w = 40;
    let plot_r = rr - lab_w;
    for (i, v) in [top, top / 2, 0].iter().enumerate() {
        let y = py0 + (py1 - py0) * i as i32 / 2;
        draw_text(hdc, &label(*v), plot_r + 6, y - 8, lab_w - 6, 16, "Segoe UI", 10, 500, grey, DT_LEFT);
    }
    let n = vals.len().max(1) as i32;
    let slot = (plot_r - rx) as f32 / n as f32;
    let bw = (slot * 0.62).clamp(4.0, 14.0);
    if let Some(gl) = goal_line {
        if gl > 0 && gl <= top {
            let y = py1 as f32 - (py1 - py0) as f32 * gl as f32 / top as f32;
            let mut x = rx as f32;
            while x < plot_r as f32 {
                gfx::fill_rect(x, y, 4.0, 1.0, cr(90, 90, 96), 1.0);
                x += 7.0;
            }
        }
    }
    for (i, (lab, v, hi)) in vals.iter().enumerate() {
        let cx = rx as f32 + slot * (i as f32 + 0.5);
        let bx = cx - bw / 2.0;
        gfx::fill_round(bx, py0 as f32, bw, (py1 - py0) as f32, bw / 2.0, cr(30, 30, 34), 1.0);
        if *v > 0 {
            let hgt = ((py1 - py0) as f32 * (*v as f32 / top as f32).min(1.0)).max(bw);
            gfx::fill_round(bx, py1 as f32 - hgt, bw, hgt, bw / 2.0, if *hi { BLUE } else { cr(64, 156, 255) }, 1.0);
        }
        if !lab.is_empty() {
            draw_text(hdc, lab, cx as i32 - 14, py1 + 4, 28, 16, "Segoe UI", 10, if *hi { 700 } else { 500 }, if *hi { WHITE } else { grey }, DT_CENTER);
        }
    }
}

unsafe fn water_chart_hours(hdc: HDC, rx: i32, rr: i32) {
    let st = &app().store;
    let mut from = st.active_from;
    let mut until = st.active_until.max(from + 1);
    for (hr, _) in &st.water_log {
        from = from.min(*hr);
        until = until.max(*hr + 1);
    }
    let mut t: SYSTEMTIME = std::mem::zeroed();
    GetLocalTime(&mut t);
    let now = t.wHour as i32;
    let vals: Vec<(String, i32, bool)> = (from..until)
        .map(|hr| {
            let ml = st.water_log.iter().filter(|(x, _)| *x == hr).map(|(_, m)| *m).sum();
            let lab = if (hr - from) % 3 == 1 { format!("{:02}", hr) } else { String::new() };
            (lab, ml, hr == now)
        })
        .collect();
    let peak = vals.iter().map(|v| v.1).max().unwrap_or(0);
    let top = ((peak + 199) / 200 * 200).max(400);
    water_bars(hdc, rx, rr, &vals, top, &|v| if v == 0 { "0 ml".into() } else { v.to_string() }, None);
}

unsafe fn water_chart_week(hdc: HDC, rx: i32, rr: i32) {
    let goal = app().store.water_goal;
    let vals: Vec<(String, i32, bool)> = water_week().into_iter().map(|(d, ml, today)| (d.to_string(), ml, today)).collect();
    let peak = vals.iter().map(|v| v.1).max().unwrap_or(0).max(goal);
    let top = (peak + 499) / 500 * 500;
    water_bars(hdc, rx, rr, &vals, top, &|v| if v == 0 { "0 L".into() } else { format!("{:.1}", v as f64 / 1000.0) }, Some(goal));
}

// small round  -  /  +  button
unsafe fn round_btn(hdc: HDC, cx: i32, cy: i32, glyph: &str, code: i32, param: i32) {
    let hov = app().hover == (code, param);
    fill_circle(hdc, cx, cy, 13, if hov { cr(62, 62, 66) } else { cr(40, 40, 44) });
    draw_text(hdc, glyph, cx - 13, cy - 14, 26, 26, "Segoe UI", 17, 600, WHITE, DT_CENTER);
    add_action(cx - 15, cy - 15, 30, 30, code, param);
}

// Water settings inside the notch (design: bottle, goal chips, hours,
// reminders / refills card, reset).
unsafe fn paint_water_settings(hdc: HDC, w: i32, _h: i32) {
    let a = app();
    let grey = cr(142, 142, 147);
    let lx = 48;
    let cx0 = 128; // controls start
    let (reminders, sip) = water_plan();
    // Bottle
    let y1 = 66;
    draw_text(hdc, "Bottle", lx, y1 - 12, 70, 24, "Segoe UI", 14, 600, WHITE, DT_LEFT);
    round_btn(hdc, cx0 + 13, y1, "\u{2212}", A_BOTTLE, -1);
    draw_text(hdc, &format!("{} ml", a.store.bottle_ml), cx0 + 28, y1 - 12, 76, 24, "Segoe UI", 15, 700, WHITE, DT_CENTER);
    round_btn(hdc, cx0 + 119, y1, "+", A_BOTTLE, 1);
    let per_bottle = if sip > 0 { (a.store.bottle_ml as f64 / sip as f64).round() as i32 } else { 0 };
    draw_text(hdc, &format!("{} ml each \u{00b7} {}/bottle", sip, per_bottle), cx0 + 138, y1 - 10, 128, 20, "Segoe UI", 10, 400, grey, DT_LEFT);
    // Goal chips
    let y2 = 104;
    draw_text(hdc, "Goal", lx, y2 - 12, 70, 24, "Segoe UI", 14, 600, WHITE, DT_LEFT);
    let mut x = cx0;
    for g in store::GOALS {
        let on = a.store.water_goal == g;
        let hov = a.hover == (A_WATER_GOAL, g);
        let bg = if on { cr(10, 132, 255) } else if hov { cr(48, 48, 52) } else { cr(34, 34, 38) };
        fill_round(hdc, x, y2 - 13, 56, 26, 26, bg);
        draw_text(hdc, &format!("{:.1} L", g as f64 / 1000.0), x, y2 - 13, 56, 26, "Segoe UI", 12, 600, if on { WHITE } else { grey }, DT_CENTER);
        add_action(x, y2 - 13, 56, 26, A_WATER_GOAL, g);
        x += 62;
    }
    // Hours
    let y3 = 142;
    draw_text(hdc, "Hours", lx, y3 - 12, 70, 24, "Segoe UI", 14, 600, WHITE, DT_LEFT);
    round_btn(hdc, cx0 + 13, y3, "\u{2212}", A_ACTIVE_FROM, -1);
    draw_text(hdc, &format!("{:02}:00", a.store.active_from), cx0 + 28, y3 - 12, 52, 24, "Segoe UI", 15, 700, WHITE, DT_CENTER);
    round_btn(hdc, cx0 + 95, y3, "+", A_ACTIVE_FROM, 1);
    draw_text(hdc, "to", cx0 + 110, y3 - 12, 24, 24, "Segoe UI", 12, 400, grey, DT_CENTER);
    round_btn(hdc, cx0 + 149, y3, "\u{2212}", A_ACTIVE_UNTIL, -1);
    draw_text(hdc, &format!("{:02}:00", a.store.active_until), cx0 + 164, y3 - 12, 52, 24, "Segoe UI", 15, 700, WHITE, DT_CENTER);
    round_btn(hdc, cx0 + 231, y3, "+", A_ACTIVE_UNTIL, 1);
    // reminders / refills card + Reset
    let (kx, kw) = (w - 48 - 116, 116);
    gfx::fill_round(kx as f32, 50.0, kw as f32, 90.0, 16.0, cr(22, 34, 48), 1.0);
    draw_text(hdc, &reminders.to_string(), kx, 54, kw, 32, "Segoe UI", 24, 700, CYAN, DT_CENTER);
    draw_text(hdc, "reminders", kx, 84, kw, 16, "Segoe UI", 11, 500, CYAN, DT_CENTER);
    let refills = (a.store.water_goal as f64 / a.store.bottle_ml.max(1) as f64).round() as i32;
    let r = refills.to_string();
    let rw = text_width(&r, "Segoe UI", 15, 700, hdc);
    let lab = if refills == 1 { " refill" } else { " refills" };
    let lw2 = text_width(lab, "Segoe UI", 11, 500, hdc);
    let rx = kx + (kw - rw - lw2) / 2;
    draw_text(hdc, &r, rx, 104, rw + 2, 26, "Segoe UI", 15, 700, CYAN, DT_LEFT);
    draw_text(hdc, lab, rx + rw, 106, lw2 + 4, 24, "Segoe UI", 11, 500, CYAN, DT_LEFT);
    let flash = GetTickCount64() < a.reset_flash_until;
    let hov = a.hover.0 == A_WATER_RESET;
    fill_round(hdc, kx, 150, kw, 26, 26, if hov { cr(48, 48, 52) } else { cr(30, 30, 34) });
    draw_text(hdc, "\u{e74d}", kx + 14, 150, 18, 26, "Segoe MDL2 Assets", 11, 400, WHITE, DT_CENTER);
    draw_text(hdc, if flash { "Cleared" } else { "Reset" }, kx + 36, 150, kw - 44, 26, "Segoe UI", 12, 600, WHITE, DT_LEFT);
    add_action(kx, 150, kw, 26, A_WATER_RESET, 0);
}

// Shelf: one big dashed drop zone when empty; otherwise file tiles in a
// row with a smaller drop tile at the end. While files are dragged over the
// notch they already show as tiles (outlined, with a +), before the drop.
// Press and drag a tile to drop the file into another app.
unsafe fn shelf_tile(hdc: HDC, tx: i32, ty: i32, tw: i32, th: i32, item: &ShelfItem, idx: i32, preview: bool) {
    let a = app();
    let grey = cr(142, 142, 147);
    let hov = !preview && a.hover.1 == idx && (a.hover.0 == A_SHELF_OPEN || a.hover.0 == A_SHELF_REMOVE);
    fill_round(hdc, tx, ty, tw, th, 32, if preview { cr(36, 30, 12) } else if hov { cr(40, 40, 44) } else { cr(28, 28, 31) });
    if preview {
        gfx::stroke_round(tx as f32 + 1.0, ty as f32 + 1.0, tw as f32 - 2.0, th as f32 - 2.0, 16.0, 1.5, true, page_color(4), 1.0);
    }
    let is_dir = std::fs::metadata(&item.path).map(|m| m.is_dir()).unwrap_or(false);
    let (ic, glyph) = if is_dir { (cr(255, 176, 0), "\u{e8b7}") } else { (BLUE, "\u{e8a5}") };
    fill_round(hdc, tx + tw / 2 - 22, ty + 16, 44, 44, 24, ic);
    draw_text(hdc, glyph, tx + tw / 2 - 22, ty + 16, 44, 44, "Segoe MDL2 Assets", 20, 400, WHITE, DT_CENTER);
    draw_text(hdc, &item.name.clone(), tx + 6, ty + 68, tw - 12, 18, "Segoe UI", 12, 500, WHITE, DT_CENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
    draw_text(hdc, &item.size.clone(), tx + 6, ty + 86, tw - 12, 16, "Segoe UI", 10, 400, grey, DT_CENTER);
    if preview {
        fill_circle(hdc, tx + tw - 13, ty + 13, 9, page_color(4));
        draw_text(hdc, "+", tx + tw - 22, ty + 3, 18, 20, "Segoe UI", 13, 700, BLACK, DT_CENTER);
        return;
    }
    add_action(tx, ty, tw, th, A_SHELF_OPEN, idx);
    if hov {
        fill_circle(hdc, tx + tw - 13, ty + 13, 9, cr(64, 64, 68));
        draw_text(hdc, "\u{2715}", tx + tw - 22, ty + 4, 18, 18, "Segoe UI", 9, 600, WHITE, DT_CENTER);
        add_action(tx + tw - 24, ty + 2, 22, 22, A_SHELF_REMOVE, idx);
    }
}

unsafe fn paint_shelf(hdc: HDC, w: i32, _h: i32) {
    let a = app();
    let grey = cr(142, 142, 147);
    let over = a.drop_hover;
    let dash = if over { page_color(4) } else { cr(84, 84, 90) };
    let (zx, zy, zw, zh) = (48, 44, w - 96, 150);
    let total = a.shelf_items.len() + if over { a.drag_preview.len() } else { 0 };
    if total == 0 {
        let bg = if over { cr(36, 28, 6) } else { cr(14, 14, 16) };
        gfx::fill_round(zx as f32, zy as f32, zw as f32, zh as f32, 18.0, bg, 1.0);
        gfx::stroke_round(zx as f32 + 0.75, zy as f32 + 0.75, zw as f32 - 1.5, zh as f32 - 1.5, 18.0, if over { 2.0 } else { 1.5 }, true, dash, 1.0);
        let my = zy + zh / 2;
        draw_text(hdc, "\u{e896}", w / 2 - 20, my - 46, 40, 40, "Segoe MDL2 Assets", 28, 400, if over { page_color(4) } else { cr(225, 225, 230) }, DT_CENTER);
        draw_text(hdc, if over { "Release to add to the Shelf" } else { "Drop files here" }, zx, my - 2, zw, 22, "Segoe UI", 15, 600, WHITE, DT_CENTER);
        let sub = if over { "Already on the Shelf" } else { "Drag files onto the notch \u{00b7} drag them out to any app" };
        draw_text(hdc, sub, zx, my + 20, zw, 18, "Segoe UI", 11, 400, grey, DT_CENTER);
        return;
    }
    let shown = total.min(4);
    let (tw, gap) = (86, 9);
    let ty = zy + 8;
    let th = zh - 16;
    // previews first while dragging, so the new files are always visible
    let mut slot = 0;
    if over {
        for item in a.drag_preview.iter().take(shown) {
            let tx = zx + slot as i32 * (tw + gap);
            shelf_tile(hdc, tx, ty, tw, th, item, -1, true);
            slot += 1;
        }
    }
    for (i, item) in a.shelf_items.iter().enumerate() {
        if slot >= shown {
            break;
        }
        let tx = zx + slot as i32 * (tw + gap);
        shelf_tile(hdc, tx, ty, tw, th, item, i as i32, false);
        slot += 1;
    }
    // drop tile
    let dx = zx + shown as i32 * (tw + gap);
    let dw = zx + zw - dx;
    if over {
        gfx::fill_round(dx as f32, ty as f32, dw as f32, th as f32, 16.0, cr(36, 28, 6), 1.0);
    }
    gfx::stroke_round(dx as f32 + 0.75, ty as f32 + 0.75, dw as f32 - 1.5, th as f32 - 1.5, 16.0, 1.5, true, dash, 1.0);
    draw_text(hdc, "\u{e896}", dx, ty + 14, dw, 34, "Segoe MDL2 Assets", 20, 400, if over { page_color(4) } else { cr(225, 225, 230) }, DT_CENTER);
    draw_text(hdc, if over { "Release" } else { "Drop files" }, dx, ty + 50, dw, 18, "Segoe UI", 12, 600, WHITE, DT_CENTER);
    let more = if total > shown { format!("+{} more", total - shown) } else if over { "to add".to_string() } else { "Drag out to share".to_string() };
    draw_text(hdc, &more, dx, ty + 68, dw, 16, "Segoe UI", 10, 400, grey, DT_CENTER);
    if !over {
        let cw = text_width("Clear", "Segoe UI", 11, 600, hdc) + 20;
        pill_btn(hdc, dx + (dw - cw) / 2, ty + th - 30, "Clear", 11, 600, 10, 22, cr(44, 44, 46), WHITE, A_SHELF_CLEAR, 0);
    }
}

const TAG_COLORS: [u32; 5] = [cr(255, 69, 58), cr(10, 132, 255), cr(255, 159, 10), cr(191, 90, 242), cr(48, 209, 88)];

// To-do (design): big done/total count, "Task Done Today", Add Task and
// Edit Task on the left; the task list in a rounded card on the right.
// Edit mode: click a task to rename it, the cross deletes it.
const TODO_CARD: (i32, i32, i32, i32) = (284, 50, 228, 138); // x, y, w, h
const TODO_ROW_H: i32 = 30;

const FIELD_BG: u32 = cr(44, 44, 48);

fn todo_rows_visible() -> usize {
    ((TODO_CARD.3 - 16) / TODO_ROW_H) as usize
}

unsafe fn paint_todo(hdc: HDC, _w: i32, _h: i32) {
    let a = app();
    let grey = cr(142, 142, 147);
    let lx = 48;
    let done = a.store.tasks.iter().filter(|t| t.done).count();
    let count = format!("{}/{}", done, a.store.tasks.len());
    let cw = text_width(&count, "Segoe UI", 40, 700, hdc);
    draw_text(hdc, &count, lx, 48, cw + 4, 54, "Segoe UI", 40, 700, WHITE, DT_LEFT);
    draw_text(hdc, "Task Done Today", lx, 100, 200, 20, "Segoe UI", 13, 400, grey, DT_LEFT);
    // Add Task
    let by = 134;
    let hov = a.hover.0 == A_TODO_ADD;
    let aw = 96;
    fill_round(hdc, lx, by, aw, 32, 32, if hov { cr(52, 52, 56) } else { cr(34, 34, 38) });
    fill_circle(hdc, lx + 18, by + 16, 8, WHITE);
    gfx::fill_rect((lx + 14) as f32, (by + 15) as f32, 8.0, 2.0, BLACK, 1.0);
    gfx::fill_rect((lx + 17) as f32, (by + 12) as f32, 2.0, 8.0, BLACK, 1.0);
    draw_text(hdc, "Add Task", lx + 31, by, aw - 33, 32, "Segoe UI", 12, 600, WHITE, DT_LEFT);
    add_action(lx, by, aw, 32, A_TODO_ADD, 0);
    // Edit Task (dim with no tasks; blue while editing)
    let ex = lx + aw + 8;
    let ew = 92;
    let has = !a.store.tasks.is_empty();
    let on = a.editing && has;
    let hov_e = a.hover.0 == A_TODO_EDIT && has;
    let bg = if on { cr(10, 132, 255) } else if hov_e { cr(52, 52, 56) } else if has { cr(34, 34, 38) } else { cr(22, 22, 25) };
    let fg = if has { WHITE } else { cr(84, 84, 90) };
    fill_round(hdc, ex, by, ew, 32, 32, bg);
    draw_text(hdc, "\u{e70f}", ex + 10, by, 18, 32, "Segoe MDL2 Assets", 12, 400, fg, DT_CENTER);
    draw_text(hdc, if on { "Done" } else { "Edit Task" }, ex + 31, by, ew - 35, 32, "Segoe UI", 12, 600, fg, DT_LEFT);
    if has {
        add_action(ex, by, ew, 32, A_TODO_EDIT, 0);
    }
    // list card
    let (kx, ky, kw, kh) = TODO_CARD;
    gfx::fill_round(kx as f32, ky as f32, kw as f32, kh as f32, 18.0, cr(30, 30, 33), 1.0);
    let edit_box = !a.edit_hwnd.is_null() && IsWindowVisible(a.edit_hwnd) != 0 && a.edit_target == 0;
    if a.store.tasks.is_empty() && !edit_box {
        let mx = kx + kw / 2;
        gfx::fill_round((mx - 21) as f32, (ky + 30) as f32, 42.0, 42.0, 11.0, cr(52, 52, 56), 1.0);
        draw_text(hdc, "\u{e9d5}", mx - 21, ky + 30, 42, 42, "Segoe MDL2 Assets", 18, 400, cr(190, 190, 196), DT_CENTER);
        draw_text(hdc, "No tasks.", kx, ky + 80, kw, 22, "Segoe UI", 13, 500, grey, DT_CENTER);
        return;
    }
    let cap = todo_rows_visible();
    let n = a.store.tasks.len();
    // newest-first would hide old ones; keep order, show what fits
    let slots = if edit_box && a.edit_index.is_none() { cap - 1 } else { cap };
    let shown = if n > slots { slots - 1 } else { n };
    let mut y = ky + 8;
    for (i, t) in a.store.tasks.iter().take(shown).enumerate() {
        let hov_row = a.hover.1 == i as i32 && matches!(a.hover.0, A_TODO_TOGGLE | A_TODO_DELETE | A_TODO_RENAME);
        if hov_row {
            gfx::fill_round((kx + 6) as f32, y as f32, (kw - 12) as f32, (TODO_ROW_H - 2) as f32, 12.0, cr(44, 44, 48), 1.0);
        }
        let (ccx, ccy) = (kx + 22, y + TODO_ROW_H / 2 - 1);
        if t.done {
            fill_circle(hdc, ccx, ccy, 8, cr(48, 209, 88));
            stroke(hdc, &[POINT { x: ccx - 4, y: ccy }, POINT { x: ccx - 1, y: ccy + 3 }, POINT { x: ccx + 4, y: ccy - 3 }], 2, BLACK);
        } else {
            gfx::arc(ccx as f32, ccy as f32, 7.5, 0.0, 360.0, 1.5, cr(120, 120, 126), 1.0);
        }
        let col = if t.done { grey } else { WHITE };
        let tx = kx + 38;
        let tmax = kw - 38 - if a.editing { 34 } else { 14 };
        let title = t.title.clone();
        let tw = text_width(&title, "Segoe UI", 13, 400, hdc).min(tmax);
        draw_text(hdc, &title, tx, y, tmax, TODO_ROW_H - 2, "Segoe UI", 13, 400, col, DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
        if t.done {
            fill_rect_c(hdc, tx, y + TODO_ROW_H / 2 - 1, tw, 1, col);
        }
        if a.editing {
            add_action(kx + 6, y, kw - 44, TODO_ROW_H - 2, A_TODO_RENAME, i as i32);
            let hd = a.hover == (A_TODO_DELETE, i as i32);
            draw_text(hdc, "\u{2715}", kx + kw - 32, y, 20, TODO_ROW_H - 2, "Segoe UI", 11, 600, if hd { cr(255, 107, 97) } else { cr(200, 80, 72) }, DT_CENTER);
            add_action(kx + kw - 38, y, 32, TODO_ROW_H - 2, A_TODO_DELETE, i as i32);
        } else {
            add_action(kx + 6, y, kw - 12, TODO_ROW_H - 2, A_TODO_TOGGLE, i as i32);
        }
        y += TODO_ROW_H;
    }
    if shown < n {
        draw_text(hdc, &format!("+{} more", n - shown), kx + 38, y, kw - 50, TODO_ROW_H - 4, "Segoe UI", 11, 500, grey, DT_LEFT);
    }
    // the input field behind the edit box: rounded, blue outline, a save
    // button at its right end
    if edit_box {
        let (_fx, fy, _fw, fh) = todo_edit_slot();
        draw_field(hdc, todo_edit_slot());
        if a.store.tasks.is_empty() && a.edit_index.is_none() {
            draw_text(hdc, "Enter to save \u{00b7} Esc to cancel", kx, fy + fh + 8, kw, 16, "Segoe UI", 10, 400, grey, DT_CENTER);
        }
    }
}

// Where the edit box goes (design units): the row it renames, or the next
// free row for a new task.
fn todo_edit_slot() -> (i32, i32, i32, i32) {
    let a = app();
    let (kx, ky, kw, _) = TODO_CARD;
    let row = match a.edit_index {
        Some(i) => i as i32,
        None => (a.store.tasks.len().min(todo_rows_visible() - 1)) as i32,
    };
    let row = row.min(todo_rows_visible() as i32 - 1);
    (kx + 10, ky + 8 + row * TODO_ROW_H, kw - 20, TODO_ROW_H - 2)
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
    let page = page % PAGES;
    if a.expanded && !a.animating && page != a.page {
        // slide the old page out and the new one in
        let pages = visible_pages();
        let pos = |p: usize| pages.iter().position(|x| *x == p).unwrap_or(0) as i32;
        a.slide_from = a.page;
        a.slide_dir = if pos(page) >= pos(a.page) { 1 } else { -1 };
        // wrapping around (last -> first with the next chevron) keeps going forward
        if (pos(page) - pos(a.page)).abs() == pages.len() as i32 - 1 && pages.len() > 2 {
            a.slide_dir = -a.slide_dir;
        }
        a.slide_start = GetTickCount64();
        a.sliding = true;
        SetTimer(hwnd, TIMER_ANIM, 10, None);
    }
    a.page = page;
    if page == 6 {
        a.sys = sysinfo::poll();
    }
    a.hover = (0, -1);
    a.water_settings = false;
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
        A_GLOW => {
            a.store.glow = !a.store.glow;
            store::save(&a.store);
            present();
        }
        A_WATER_SETTINGS => a.water_settings = true,
        A_WSET_BACK => a.water_settings = false,
        A_CLOSE_NOTCH => {
            a.popup_until = 0;
            a.hover = (0, -1);
            hide_edit();
            a.expanded = false;
            a.revealed = false;
            a.hide_at = 0;
            a.water_settings = false;
            a.editing = false;
            apply_size(hwnd);
        }
        A_TODO_COMMIT => commit_edit(),
        A_CAL_PREV | A_CAL_NEXT => {
            let k = if code == A_CAL_PREV { -1 } else { 1 };
            let mi = a.cal_y * 12 + (a.cal_m - 1) + k;
            a.cal_y = mi.div_euclid(12);
            a.cal_m = mi.rem_euclid(12) + 1;
            hide_edit();
        }
        A_CAL_DAY => {
            a.cal_sel = (a.cal_y, a.cal_m, param);
            hide_edit();
        }
        A_CAL_TODAY => {
            let (y, m, d) = today_ymd();
            a.cal_y = y;
            a.cal_m = m;
            a.cal_sel = (y, m, d);
            hide_edit();
        }
        A_CAL_ADD => show_cal_edit(hwnd),
        A_CAL_DEL => {
            if (param as usize) < a.store.events.len() {
                a.store.events.remove(param as usize);
                store::save(&a.store);
            }
        }
        A_SHOW_SYSMON | A_SHOW_CAL => {
            if code == A_SHOW_SYSMON {
                a.store.show_sysmon = !a.store.show_sysmon;
            } else {
                a.store.show_calendar = !a.store.show_calendar;
            }
            if !visible_pages().contains(&a.page) {
                a.page = 0;
            }
            store::save(&a.store);
        }
        A_INDICATORS => {
            a.store.indicators = !a.store.indicators;
            store::save(&a.store);
        }
        A_NOTIF_POPUP => {
            a.store.notif_popup = !a.store.notif_popup;
            store::save(&a.store);
        }

        A_AUTO_HIDE => {
            a.store.auto_hide = !a.store.auto_hide;
            store::save(&a.store);
            apply_size(hwnd);
        }
        A_WATER_VIEW => a.water_view = param.clamp(0, 2) as u8,
        A_BOTTLE => {
            a.store.bottle_ml = (a.store.bottle_ml + 50 * param).clamp(100, 5000);
            store::save(&a.store);
        }
        A_GLOW_COLOR => {
            let n = store::GLOW_COLORS.len() as i32;
            a.store.glow_color = (a.store.glow_color + param).rem_euclid(n);
            store::save(&a.store);
            present();
        }
        A_GLOW_SIZE => {
            a.store.glow_size = param;
            store::save(&a.store);
            present();
        }
        A_GLOW_STRENGTH => {
            a.store.glow_strength = param;
            store::save(&a.store);
            present();
        }
        A_LYRICS_EN => {
            a.store.lyrics_english = !a.store.lyrics_english;
            lyrics::ENGLISH.store(a.store.lyrics_english, std::sync::atomic::Ordering::Relaxed);
            store::save(&a.store);
            let m = media::snapshot();
            if m.present && !m.title.is_empty() {
                lyrics::refetch(&m.title, &m.artist, m.duration_secs);
            }
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
                let (_, sip) = water_plan();
                a.store.water_ml = (a.store.water_ml + sip).min(a.store.water_goal);
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
            a.editing = false;
            show_edit(hwnd, None);
        }
        A_TODO_EDIT => {
            a.editing = !a.editing;
        }
        A_TODO_RENAME => show_edit(hwnd, Some(param as usize)),
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
                if a.store.tasks.is_empty() {
                    a.editing = false;
                }
                store::save(&a.store);
            }
        }
        _ => {}
    }
    inval(hwnd);
    let sh = app().settings_hwnd;
    if !sh.is_null() {
        inval(sh);
    }
}

// ---------- tray icon + Windows notifications ----------
fn taskbar_created_msg() -> u32 {
    static mut MSG: u32 = 0;
    unsafe {
        if MSG == 0 {
            MSG = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        }
        MSG
    }
}

unsafe fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid
}

fn copy_w(dst: &mut [u16], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

unsafe fn tray_add(hwnd: HWND) {
    let a = app();
    if a.tray_icon == 0 {
        a.tray_icon = gfx::make_icon() as isize;
    }
    let mut nid = tray_data(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = a.tray_icon as HICON;
    copy_w(&mut nid.szTip, "Isle");
    Shell_NotifyIconW(NIM_ADD, &nid);
}

unsafe fn tray_remove(hwnd: HWND) {
    let nid = tray_data(hwnd);
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

// A Windows notification (shown as a toast on Windows 10/11) from the tray
// icon; clicking it opens the notch on `page`.
unsafe fn notify(title: &str, text: &str, page: usize) {
    let a = app();
    a.notify_page = page;
    a.last_own_notify = GetTickCount64();
    let mut nid = tray_data(a.hwnd);
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON | if a.store.sounds { 0 } else { NIIF_NOSOUND };
    nid.hBalloonIcon = a.tray_icon as HICON;
    copy_w(&mut nid.szInfoTitle, title);
    copy_w(&mut nid.szInfo, text);
    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

unsafe fn show_notch(hwnd: HWND, page: Option<usize>) {
    let a = app();
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    if let Some(p) = page {
        a.page = p;
    }
    a.revealed = true;
    a.expanded = true;
    a.popup_until = GetTickCount64() + 6000;
    apply_size(hwnd);
}

unsafe fn tray_menu(hwnd: HWND) {
    let m = CreatePopupMenu();
    AppendMenuW(m, MF_STRING, TRAY_SHOW, wide("Open the notch").as_ptr());
    AppendMenuW(m, MF_STRING, TRAY_SETTINGS, wide("Settings").as_ptr());
    AppendMenuW(m, MF_SEPARATOR, 0, null());
    AppendMenuW(m, MF_STRING, TRAY_QUIT, wide("Quit Isle").as_ptr());
    let mut p = POINT { x: 0, y: 0 };
    GetCursorPos(&mut p);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(m, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, p.x, p.y, 0, hwnd, null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(m);
}

// ---------- status indicators and other apps' notifications ----------
// A short popup in the minimized notch (skipped while the notch is open:
// the header shows the same state).
unsafe fn show_indicator(glyph: &str, title: &str, sub: &str, color: u32, ms: u64) {
    let a = app();
    if a.expanded {
        inval(a.hwnd);
        return;
    }
    a.alert_kind = 1;
    a.ind_glyph = glyph.to_string();
    a.ind_title = title.to_string();
    a.ind_sub = sub.to_string();
    a.ind_color = color;
    a.alert_until = GetTickCount64() + ms;
    apply_size(a.hwnd);
}

static mut KEY_DOWN: [bool; 3] = [false; 3];

// Low-level keyboard hook: only watches Caps, Num and Scroll Lock presses.
unsafe extern "system" fn kb_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = &*(lparam as *const KBDLLHOOKSTRUCT);
        let slot = match k.vkCode {
            0x14 => Some(0),
            0x90 => Some(1),
            0x91 => Some(2),
            _ => None,
        };
        if let Some(i) = slot {
            let down = wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN;
            let kd = &mut *(&raw mut KEY_DOWN);
            if down && !kd[i] {
                PostMessageW(app().hwnd, WM_LOCKKEY, k.vkCode as WPARAM, 0);
            }
            kd[i] = down;
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

unsafe fn on_lock_key(vk: usize) {
    let a = app();
    let (name, on) = match vk {
        0x14 => {
            a.caps = !a.caps;
            ("Caps Lock", a.caps)
        }
        0x90 => {
            a.num = !a.num;
            ("Num Lock", a.num)
        }
        _ => {
            a.scroll = !a.scroll;
            ("Scroll Lock", a.scroll)
        }
    };
    if a.store.indicators {
        let glyph = match vk { 0x14 => "\u{e84b}", 0x90 => "\u{e765}", _ => "\u{e8cb}" };
        show_indicator(glyph, name, if on { "On" } else { "Off" }, if on { cr(48, 209, 88) } else { cr(142, 142, 147) }, 1800);
    }
    inval(a.hwnd);
}

static NET: std::sync::Mutex<Option<(Option<String>, Option<Vec<String>>)>> = std::sync::Mutex::new(None);

// Wi-Fi and Bluetooth are polled off the UI thread every 3 s; a change is
// posted to the notch.
fn start_net_watch(hwnd: HWND) {
    let h = hwnd as isize;
    std::thread::spawn(move || {
        let mut last: Option<(Option<String>, Option<Vec<String>>)> = None;
        loop {
            let now = (sysinfo::wifi(), sysinfo::bluetooth());
            if last.as_ref() != Some(&now) {
                *NET.lock().unwrap() = Some(now.clone());
                unsafe {
                    PostMessageW(h as HWND, WM_NETCHANGE, 0, 0);
                }
                last = Some(now);
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    });
}

unsafe fn on_net_change() {
    let a = app();
    let Some((wifi, bt)) = NET.lock().unwrap().take() else { return };
    let first = !a.net_known;
    a.net_known = true;
    let show = a.store.indicators && !first;
    if show {
        let old = a.wifi.clone().unwrap_or_default();
        let new = wifi.clone().unwrap_or_default();
        if new != old {
            if new.is_empty() {
                show_indicator("\u{eb5e}", "Wi-Fi disconnected", &old, cr(255, 69, 58), 2600);
            } else {
                show_indicator("\u{e701}", "Wi-Fi connected", &new, cr(10, 132, 255), 2600);
            }
        }
        let olds = a.bt.clone().unwrap_or_default();
        let news = bt.clone().unwrap_or_default();
        if let Some(d) = news.iter().find(|d| !olds.contains(d)) {
            show_indicator("\u{e702}", d, "Bluetooth connected", cr(10, 132, 255), 2600);
        } else if let Some(d) = olds.iter().find(|d| !news.contains(d)) {
            show_indicator("\u{e702}", d, "Bluetooth disconnected", cr(142, 142, 147), 2600);
        }
    }
    a.wifi = wifi;
    a.bt = bt;
    inval(a.hwnd);
}

// Another app's Windows notification: the toast is a CoreWindow titled
// "New notification" owned by ShellExperienceHost.exe.
unsafe extern "system" fn win_event(_h: HWINEVENTHOOK, _ev: u32, hwnd: HWND, id_object: i32, id_child: i32, _t: u32, _ms: u32) {
    if id_object != 0 || id_child != 0 || hwnd.is_null() {
        return;
    }
    let mut cls = [0u16; 64];
    let n = GetClassNameW(hwnd, cls.as_mut_ptr(), 64);
    if String::from_utf16_lossy(&cls[..n.max(0) as usize]) != "Windows.UI.Core.CoreWindow" {
        return;
    }
    let mut title = [0u16; 128];
    let n = GetWindowTextW(hwnd, title.as_mut_ptr(), 128);
    let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
    if !title.to_lowercase().contains("new notification") {
        return;
    }
    PostMessageW(app().hwnd, WM_TOAST, 0, 0);
}

unsafe fn on_toast() {
    let a = app();
    if !a.store.notif_popup || GetTickCount64().saturating_sub(a.last_own_notify) < 5000 {
        return;
    }
    show_indicator("\u{ea8f}", "New notification", "", WHITE, 4000);
}

// ---------- calendar math ----------
fn days_in_month(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 },
    }
}

// 0 = Sunday
fn weekday(y: i32, m: i32, d: i32) -> i32 {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    (y + y / 4 - y / 100 + y / 400 + t[(m - 1) as usize] + d).rem_euclid(7)
}

fn date_key(y: i32, m: i32, d: i32) -> String {
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn today_ymd() -> (i32, i32, i32) {
    unsafe {
        let mut t: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut t);
        (t.wYear as i32, t.wMonth as i32, t.wDay as i32)
    }
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

fn fmt_min(m: i32) -> String {
    if m < 0 { "All day".into() } else { format!("{:02}:{:02}", m / 60, m % 60) }
}

// "14:30 Team call", "9am Gym", "9:15 pm Call mum", "Dentist" (all day)
fn parse_event(s: &str) -> (i32, String) {
    let s = s.trim();
    let mut words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() {
        return (-1, String::new());
    }
    let first = words[0].to_lowercase();
    let (mut tok, mut ampm) = (first.clone(), "");
    for suf in ["am", "pm"] {
        if first.ends_with(suf) && first.len() > 2 {
            tok = first[..first.len() - 2].to_string();
            ampm = suf;
        }
    }
    let parsed = if let Some((h, m)) = tok.split_once([':', '.']) {
        match (h.parse::<i32>(), m.parse::<i32>()) {
            (Ok(h), Ok(m)) if h < 24 && m < 60 => Some((h, m)),
            _ => None,
        }
    } else if !ampm.is_empty() || (words.len() > 1 && matches!(words[1].to_lowercase().as_str(), "am" | "pm")) {
        tok.parse::<i32>().ok().filter(|h| *h >= 1 && *h <= 12).map(|h| (h, 0))
    } else {
        None
    };
    let Some((mut h, m)) = parsed else { return (-1, s.to_string()) };
    words.remove(0);
    if ampm.is_empty() && !words.is_empty() && matches!(words[0].to_lowercase().as_str(), "am" | "pm") {
        ampm = if words[0].eq_ignore_ascii_case("pm") { "pm" } else { "am" };
        words.remove(0);
    }
    if ampm == "pm" && h < 12 {
        h += 12;
    } else if ampm == "am" && h == 12 {
        h = 0;
    }
    let title = words.join(" ");
    (h * 60 + m, if title.is_empty() { "Reminder".into() } else { title })
}

// ---------- dragging files onto the notch ----------
// A file dragged over the notch opens it on the Shelf right away.
unsafe fn drag_enter_cb(files: Vec<String>) {
    let a = app();
    a.drop_hover = true;
    a.revealed = true;
    a.drag_preview = files
        .iter()
        .filter(|p| !a.store.shelf.contains(p))
        .map(|p| ShelfItem { path: p.clone(), size: file_size_str(p), name: p.rsplit(['\\', '/']).next().unwrap_or(p.as_str()).to_string() })
        .collect();
    a.popup_until = 0;
    if a.page != 4 {
        goto_page(a.hwnd, 4);
    }
    if !a.expanded {
        a.expanded = true;
        apply_size(a.hwnd);
    }
    inval(a.hwnd);
}

unsafe fn drag_leave_cb() {
    let a = app();
    a.drop_hover = false;
    a.drag_preview.clear();
    let mut p = POINT { x: 0, y: 0 };
    GetCursorPos(&mut p);
    let mut wr: RECT = std::mem::zeroed();
    GetWindowRect(a.hwnd, &mut wr);
    if !(p.x >= wr.left && p.x < wr.right && p.y >= wr.top && p.y < wr.bottom) {
        collapse(a.hwnd);
    }
    inval(a.hwnd);
}

unsafe fn drop_cb(paths: Vec<String>) {
    let a = app();
    a.drop_hover = false;
    a.drag_preview.clear();
    for path in paths {
        if !path.is_empty() && !a.store.shelf.contains(&path) {
            a.store.shelf.push(path);
        }
    }
    a.shelf_items = build_shelf_items(&a.store);
    store::save(&a.store);
    if a.page != 4 {
        goto_page(a.hwnd, 4);
    }
    a.expanded = true;
    // no mouse messages arrive during a drag: close after a few seconds
    // unless the pointer is on the notch then
    a.tracking = false;
    a.popup_until = GetTickCount64() + 4000;
    apply_size(a.hwnd);
}

// ---------- todo inline edit ----------
unsafe fn show_edit(hwnd: HWND, index: Option<usize>) {
    let a = app();
    if a.page != 5 {
        a.page = 5;
        apply_size(hwnd);
    }
    a.edit_index = index;
    a.edit_target = 0;
    let text = index.and_then(|i| a.store.tasks.get(i)).map(|t| t.title.clone()).unwrap_or_default();
    show_edit_at(hwnd, todo_edit_slot(), &text);
}

unsafe fn show_cal_edit(hwnd: HWND) {
    let a = app();
    a.edit_target = 1;
    a.edit_index = None;
    show_edit_at(hwnd, cal_edit_slot(), "");
}

unsafe fn show_edit_at(hwnd: HWND, slot: (i32, i32, i32, i32), text: &str) {
    let a = app();
    let (dw, dh) = expanded_size();
    let (pw, ph) = px_of((dw, dh), expanded_scale());
    let (fx, fy, fw, fh) = slot;
    // the edit box is its own small borderless topmost window, inset into
    // the field drawn on the notch (text only; the field is the notch's)
    let sc = expanded_scale();
    let fpx = 13 * sc / 100;
    let (x, ew) = (fx + 12, fw - 12 - fh - 4);
    let (ox, oy) = (a.win_pos.0 + a.notch_off.0, a.win_pos.1 + a.notch_off.1);
    let eh_px = fpx + 6;
    let y_px = oy + (fy * ph / dh) + ((fh * ph / dh) - eh_px) / 2;
    SendMessageW(a.edit_hwnd, WM_SETFONT, get_font("Segoe UI", fpx, 400) as WPARAM, 0);
    SetWindowPos(a.edit_hwnd, HWND_TOPMOST, ox + x * pw / dw, y_px, ew * pw / dw, eh_px, SWP_SHOWWINDOW);
    SetWindowTextW(a.edit_hwnd, wide(text).as_ptr());
    SendMessageW(a.edit_hwnd, 0x00B1 /* EM_SETSEL */, 0, -1isize as LPARAM);
    ShowWindow(a.edit_hwnd, SW_SHOW);
    SetForegroundWindow(a.edit_hwnd);
    SetFocus(a.edit_hwnd);
    inval(hwnd);
}

unsafe fn commit_edit() {
    let a = app();
    let mut buf = [0u16; 256];
    let n = GetWindowTextW(a.edit_hwnd, buf.as_mut_ptr(), 256);
    ShowWindow(a.edit_hwnd, SW_HIDE);
    if n > 0 {
        let title = String::from_utf16_lossy(&buf[..n as usize]).trim().to_string();
        if !title.is_empty() && a.edit_target == 1 {
            let (y, m, d) = a.cal_sel;
            let (min, t) = parse_event(&title);
            a.store.events.push(store::Event { date: date_key(y, m, d), min, title: t, fired: false });
            store::save(&a.store);
        } else if !title.is_empty() {
            match a.edit_index.and_then(|i| a.store.tasks.get_mut(i)) {
                Some(t) => t.title = title,
                None => a.store.tasks.push(store::Task { title, done: false }),
            }
            store::save(&a.store);
        }
    }
    SetFocus(a.hwnd);
    inval(a.hwnd);
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
unsafe fn client_to_design(_hwnd: HWND, px: i32, py: i32) -> (i32, i32) {
    let a = app();
    let k = a.notch_k.max(0.1);
    (((px - a.notch_off.0) as f32 / k).floor() as i32, ((py - a.notch_off.1) as f32 / k).floor() as i32)
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
    if !a.expanded && a.revealed && !a.drop_hover && a.shelf_press.is_none() {
        // the minimized notch stays a few seconds before hiding again
        a.hide_at = GetTickCount64() + HIDE_DELAY_MS;
        return;
    }
    if a.expanded && !edit_visible && !a.vol_drag && !a.drop_hover {
        a.hide_at = GetTickCount64() + HIDE_DELAY_MS;
        a.expanded = false;
        a.dots_hover = false;
        a.hover = (0, -1);
        a.water_settings = false;
        a.editing = false;
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
        // a compact window: 85% of the design size at the display scaling
        let s = (GetDpiForSystem() as i32 * 85 / 96).max(85);
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
        WS_POPUP | WS_SYSMENU,
        (sw - w) / 2, (sh - h) / 2, w, h,
        null_mut(), null_mut(), GetModuleHandleW(null()), null(),
    );
    if hwnd.is_null() {
        return;
    }
    a.settings_hwnd = hwnd;
    // Windows 11 rounds the corners smoothly (with its shadow); older
    // Windows keeps square corners.
    let pref: i32 = 2; // DWMWCP_ROUND
    windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(hwnd, 33, &pref as *const i32 as *const _, 4);
    let _ = sc;
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    inval(a.hwnd);
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
    polygon(hdc, &pts, cr(18, 18, 24));
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
    // a single close button
    let hov = a.settings_hover == -2;
    fill_circle(hdc, 26, 24, 11, if hov { cr(255, 69, 58) } else { cr(52, 52, 55) });
    stroke(hdc, &[POINT { x: 22, y: 20 }, POINT { x: 30, y: 28 }], 2, WHITE);
    stroke(hdc, &[POINT { x: 22, y: 28 }, POINT { x: 30, y: 20 }], 2, WHITE);
    add_action(12, 10, 28, 28, S_CLOSE, 0);
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
            s_group(hdc, x0, cw, &mut y, &[row("Hide until hovered", "Only a small shadow at the top centre until you hover it", RowKind::Switch(st.auto_hide, A_AUTO_HIDE, true))]);
            s_h2(hdc, x0, cw, &mut y, "Glow");
            s_group(hdc, x0, cw, &mut y, &[
                row("Glow", "A soft light around the notch", RowKind::Switch(st.glow, A_GLOW, true)),
                row("Colour", "", RowKind::Step(store::GLOW_COLORS[st.glow_color as usize].into(), A_GLOW_COLOR)),
                row("Size", "", seg(&[("Small", 10), ("Medium", 16), ("Large", 24)], st.glow_size, A_GLOW_SIZE)),
                row("Strength", "", seg(&[("Soft", 55), ("Normal", 100), ("Strong", 170)], st.glow_strength, A_GLOW_STRENGTH)),
            ]);
            s_h2(hdc, x0, cw, &mut y, "Motion");
            s_group(hdc, x0, cw, &mut y, &[row("Animation speed", "", seg(&[("Normal", 0), ("Fast", 1)], st.anim_fast as i32, A_ANIM))]);
        }
        3 => {
            s_h2(hdc, x0, cw, &mut y, "Lyrics");
            s_group(hdc, x0, cw, &mut y, &[row("Show lyrics", "From LRCLIB, a community-contributed lyrics database", RowKind::Switch(st.show_lyrics, A_SHOW_LYRICS, true)),
                row("Lyrics in English letters", "Tamil, Malayalam, Hindi and other lyrics written in English letters", RowKind::Switch(st.lyrics_english, A_LYRICS_EN, st.show_lyrics)),
            ]);
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
            s_group(hdc, x0, cw, &mut y, &[row("Water reminders", "A sip every 15 minutes in your active hours", RowKind::Switch(st.reminders, A_WATER_BELL, true))]);
            s_h2(hdc, x0, cw, &mut y, "Plan");
            s_group(hdc, x0, cw, &mut y, &[
                row("Daily goal", "", seg(&[("1.5 L", 1500), ("2 L", 2000), ("2.5 L", 2500), ("3 L", 3000)], st.water_goal, A_WATER_GOAL)),
                row("Bottle", "", RowKind::Step(format!("{} ml", st.bottle_ml), A_BOTTLE)),
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
        8 => {
            s_h2(hdc, x0, cw, &mut y, "Pages");
            s_group(hdc, x0, cw, &mut y, &[
                row("System Monitor", "CPU, RAM, GPU, network and disk", RowKind::Switch(st.show_sysmon, A_SHOW_SYSMON, true)),
                row("Calendar", "Month view with reminders for any day", RowKind::Switch(st.show_calendar, A_SHOW_CAL, true)),
            ]);
            s_h2(hdc, x0, cw, &mut y, "Popups in the minimized notch");
            s_group(hdc, x0, cw, &mut y, &[
                row("Status indicators", "Caps Lock, Num Lock, Wi-Fi and Bluetooth changes", RowKind::Switch(st.indicators, A_INDICATORS, true)),
                row("Other apps' notifications", "Pops up when a Windows notification appears", RowKind::Switch(st.notif_popup, A_NOTIF_POPUP, true)),
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
                        inval(hwnd);
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
            inval(hwnd);
            0
        }
        // hover highlight on the sidebar
        WM_MOUSEMOVE => {
            let (x, y) = settings_to_design(hwnd, (lparam & 0xFFFF) as i16 as i32, ((lparam >> 16) & 0xFFFF) as i16 as i32);
            let hov = match settings_hit(x, y) {
                Some((S_SECTION, p)) => p,
                Some((S_CLOSE, _)) => -2,
                _ => -1,
            };
            let a = app();
            if hov != a.settings_hover {
                a.settings_hover = hov;
                inval(hwnd);
            }
            if hov != -1 {
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
                inval(hwnd);
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
            inval(a.hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// Render a painter that draws in design units (dw x dh) into `dst` at
// (dx, dy, pw, ph) with Direct2D (anti-aliased shapes and text).
unsafe fn render_scaled(ref_dc: HDC, dst: HDC, dx: i32, dy: i32, pw: i32, ph: i32, dw: i32, dh: i32, painter: &dyn Fn(HDC)) {
    if pw <= 0 || ph <= 0 {
        return;
    }
    let mem = CreateCompatibleDC(ref_dc);
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = pw;
    bi.bmiHeader.biHeight = -ph;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut core::ffi::c_void = null_mut();
    let dib = CreateDIBSection(ref_dc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if dib.is_null() {
        DeleteDC(mem);
        return;
    }
    let old = SelectObject(mem, dib as HGDIOBJ);
    if gfx::begin(mem as _, pw, ph) {
        gfx::transform(pw as f32 / dw as f32, ph as f32 / dh as f32, 0.0, 0.0);
        painter(mem);
        gfx::end();
        BitBlt(dst, dx, dy, pw, ph, mem, 0, 0, SRCCOPY);
    }
    SelectObject(mem, old);
    DeleteObject(dib as HGDIOBJ);
    DeleteDC(mem);
}

// ---------- window proc ----------
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let edit = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                wide("Edit").as_ptr(),
                wide("").as_ptr(),
                WS_POPUP | 0x0080, // ES_AUTOHSCROLL, no border: the notch draws the field
                0, 0, 10, 10,
                hwnd,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            app().edit_hwnd = edit;
            let font = get_font("Segoe UI", 13, 400);
            SendMessageW(edit, WM_SETFONT, font as WPARAM, 1);
            SetWindowSubclass(edit, Some(edit_subclass), 1, 0);
            DragAcceptFiles(hwnd, 1);
            dnd::register(hwnd, dnd::Handlers { enter: drag_enter_cb, leave: drag_leave_cb, drop: drop_cb });
            tray_add(hwnd);
            {
                let a = app();
                a.caps = (GetKeyState(0x14) & 1) != 0;
                a.num = (GetKeyState(0x90) & 1) != 0;
                a.scroll = (GetKeyState(0x91) & 1) != 0;
                let (y, m, d) = today_ymd();
                a.cal_y = y;
                a.cal_m = m;
                a.cal_sel = (y, m, d);
            }
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_hook), GetModuleHandleW(null()), 0);
            SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, null_mut(), Some(win_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
            start_net_watch(hwnd);
            SetTimer(hwnd, TIMER_ID, 1000, None);
            0
        }
        WM_CTLCOLOREDIT => {
            let hdc = wparam as HDC;
            SetTextColor(hdc, WHITE);
            SetBkColor(hdc, FIELD_BG);
            static mut FIELD_BRUSH: isize = 0;
            if FIELD_BRUSH == 0 {
                FIELD_BRUSH = CreateSolidBrush(FIELD_BG) as isize;
            }
            FIELD_BRUSH as LRESULT
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            present();
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
            let (cpx, cpy) = ((lparam & 0xFFFF) as i16 as i32, ((lparam >> 16) & 0xFFFF) as i16 as i32);
            // pressed on a Shelf file and moved: drag it out to other apps
            if let Some((idx, sx, sy)) = a.shelf_press {
                if (wparam & 0x0001) != 0 {
                    let mut p = POINT { x: cpx, y: cpy };
                    ClientToScreen(hwnd, &mut p);
                    if (p.x - sx).abs() > GetSystemMetrics(SM_CXDRAG) || (p.y - sy).abs() > GetSystemMetrics(SM_CYDRAG) {
                        a.shelf_press = None;
                        ReleaseCapture();
                        if let Some(item) = a.shelf_items.get(idx) {
                            let path = item.path.clone();
                            dnd::drag_out(hwnd, &[path]);
                        }
                        a.tracking = false;
                        a.hover = (0, -1);
                        let mut c = POINT { x: 0, y: 0 };
                        GetCursorPos(&mut c);
                        let mut wr: RECT = std::mem::zeroed();
                        GetWindowRect(hwnd, &mut wr);
                        if !(c.x >= wr.left && c.x < wr.right && c.y >= wr.top && c.y < wr.bottom) {
                            collapse(hwnd);
                        }
                        inval(hwnd);
                        return 0;
                    }
                    return 0;
                } else {
                    a.shelf_press = None;
                    ReleaseCapture();
                }
            }
            if !in_notch(cpx, cpy) {
                // on the glow: counts as outside
                if a.expanded && !a.animating && !a.vol_drag {
                    if a.dots_hover || a.hover.0 != 0 {
                        a.dots_hover = false;
                        a.hover = (0, -1);
                    }
                    collapse(hwnd);
                }
                return 0;
            }
            if !a.expanded {
                if a.animating {
                    return 0;
                }
                a.hide_at = 0;
                if mini_mode() == Mini::Hidden {
                    // first hover at the top centre: show the minimized notch
                    a.revealed = true;
                    a.reveal_tick = GetTickCount64();
                    apply_size(hwnd);
                    return 0;
                }
                // hover again (keep moving on the minimized notch) to open it
                let settled = !a.store.auto_hide || GetTickCount64().saturating_sub(a.reveal_tick) > 450;
                if !a.store.open_click && settled {
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
                inval(hwnd);
                return 0;
            }
            // bottom dots <-> page icons on hover
            let (dw, dh) = cur_design();
            let z = dots_zone(dw, dh, a.dots_hover);
            let over = a.store.page_icons && x >= z.left && x < z.right && y >= z.top && y < z.bottom;
            if over != a.dots_hover {
                a.dots_hover = over;
                inval(hwnd);
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
                inval(hwnd);
            }
            0
        }
        WM_MOUSELEAVE_MSG => {
            let a = app();
            a.tracking = false;
            if a.dots_hover || a.hover.0 != 0 {
                a.dots_hover = false;
                a.hover = (0, -1);
                inval(hwnd);
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
            if hit == Some(A_SHELF_OPEN) {
                let mut idx = -1;
                for (r, code, param) in &a.actions {
                    if *code == A_SHELF_OPEN && x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                        idx = *param;
                    }
                }
                if idx >= 0 {
                    let mut p = POINT { x: (lparam & 0xFFFF) as i16 as i32, y: ((lparam >> 16) & 0xFFFF) as i16 as i32 };
                    ClientToScreen(hwnd, &mut p);
                    a.shelf_press = Some((idx as usize, p.x, p.y));
                    SetCapture(hwnd);
                }
            }
            if hit == Some(A_VOL_BAR) {
                a.vol_drag = true;
                SetCapture(hwnd);
                vol_set_from_x(x);
                inval(hwnd);
            }
            0
        }
        WM_SETCURSOR => {
            SetCursor(LoadCursorW(null_mut(), IDC_ARROW));
            1
        }
        WM_LBUTTONUP => {
            if app().shelf_press.take().is_some() {
                // a click, not a drag: falls through to open the file
                ReleaseCapture();
            }
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
                inval(hwnd);
                return 0;
            }
            if app().animating {
                return 0;
            }
            if !in_notch((lparam & 0xFFFF) as i16 as i32, ((lparam >> 16) & 0xFFFF) as i16 as i32) {
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
            // fallback when OLE drag and drop could not be registered
            let hdrop = wparam as HDROP;
            let count = DragQueryFileW(hdrop, 0xFFFFFFFF, null_mut(), 0);
            let mut paths = Vec::new();
            for i in 0..count {
                let len = DragQueryFileW(hdrop, i, null_mut(), 0);
                let mut buf = vec![0u16; len as usize + 1];
                DragQueryFileW(hdrop, i, buf.as_mut_ptr(), len + 1);
                paths.push(String::from_utf16_lossy(&buf[..len as usize]));
            }
            DragFinish(hdrop);
            drop_cb(paths);
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
                        inval(hwnd);
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
                    inval(hwnd);
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
                inval(hwnd);
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
                    let (title, text) = if a.pomo_mode == 0 {
                        ("Focus session done", "Nice work. Time for a break.")
                    } else {
                        ("Break is over", "Ready for the next focus session?")
                    };
                    notify(title, text, 2);
                    if !a.expanded {
                        show_notch(hwnd, Some(2));
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
                    // one sip reminder every 15 minutes (see water_plan)
                    let every_ms = 15 * 60_000u64;
                    if now.saturating_sub(a.last_water_tick) >= every_ms {
                        a.last_water_tick = now;
                        let (_, sip) = water_plan();
                        let left = (a.store.water_goal - a.store.water_ml).max(0);
                        notify("Time for a sip of water", &format!("About {} ml now \u{00b7} {:.1} L to go today", sip, left as f64 / 1000.0), 3);
                        if !a.expanded {
                            a.page = 3;
                            show_notch(hwnd, Some(3));
                        }
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
                    a.alert_kind = 0;
                    a.alert_until = now + 3800;
                }
                a.last_ac = ac;
            }
            if a.alert_until > 0 && (now >= a.alert_until || a.expanded) {
                a.alert_until = 0;
            }
            // auto-hide: the revealed minimized notch goes after a delay
            if a.hide_at > 0 && now >= a.hide_at {
                a.hide_at = 0;
                let mut p = POINT { x: 0, y: 0 };
                GetCursorPos(&mut p);
                let mut wr: RECT = std::mem::zeroed();
                GetWindowRect(hwnd, &mut wr);
                let inside = p.x >= wr.left && p.x < wr.right && p.y >= wr.top && p.y < wr.bottom;
                if !inside && !a.expanded {
                    a.revealed = false;
                    apply_size(hwnd);
                }
            }
            // Calendar reminders due now (within the last hour)
            {
                let (y, mo, d) = today_ymd();
                let key = date_key(y, mo, d);
                let mut t: SYSTEMTIME = std::mem::zeroed();
                GetLocalTime(&mut t);
                let now_min = t.wHour as i32 * 60 + t.wMinute as i32;
                let mut due: Option<(String, i32)> = None;
                for e in a.store.events.iter_mut() {
                    if !e.fired && e.min >= 0 && e.date == key && e.min <= now_min && now_min - e.min < 60 {
                        e.fired = true;
                        due = Some((e.title.clone(), e.min));
                    }
                }
                if let Some((title, m)) = due {
                    store::save(&a.store);
                    if a.store.sounds {
                        MessageBeep(0x40);
                    }
                    notify(&title, &format!("Reminder \u{00b7} {}", fmt_min(m)), 7);
                    a.cal_sel = (y, mo, d);
                    a.cal_y = y;
                    a.cal_m = mo;
                    if !a.expanded {
                        show_notch(hwnd, Some(7));
                    }
                }
            }
            // System Monitor readings while its page is open
            if a.expanded && a.page == 6 {
                a.sys = sysinfo::poll();
            }
            // water day rollover
            let today = today_string();
            if a.store.water_date != today {
                store::roll_water(&mut a.store, &today);
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
            inval(hwnd);
            0
        }
        WM_RBUTTONUP => {
            tray_menu(hwnd);
            0
        }
        WM_TRAY => {
            let ev = (lparam & 0xFFFF) as u32;
            match ev {
                WM_LBUTTONUP => show_notch(hwnd, None),
                WM_RBUTTONUP | WM_CONTEXTMENU => tray_menu(hwnd),
                _ if ev == NIN_BALLOONUSERCLICK => show_notch(hwnd, Some(app().notify_page)),
                _ => {}
            }
            0
        }
        WM_SHOW_NOTCH => {
            show_notch(hwnd, None);
            0
        }
        WM_LOCKKEY => {
            on_lock_key(wparam);
            0
        }
        WM_NETCHANGE => {
            on_net_change();
            0
        }
        WM_TOAST => {
            on_toast();
            0
        }
        WM_COMMAND => {
            let id = wparam & 0xFFFF;
            let code = (wparam >> 16) & 0xFFFF;
            if lparam as HWND == app().edit_hwnd && code == 0x0200 {
                // EN_KILLFOCUS: clicking away saves the task
                if IsWindowVisible(app().edit_hwnd) != 0 {
                    commit_edit();
                }
                return 0;
            }
            match id {
                TRAY_SHOW => show_notch(hwnd, None),
                TRAY_SETTINGS => open_settings(),
                TRAY_QUIT => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            tray_remove(hwnd);
            PostQuitMessage(0);
            0
        }
        _ if msg == taskbar_created_msg() && msg != 0 => {
            // Explorer restarted: put the tray icon back
            tray_add(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Isle";

// `Isle.exe --uninstall`, run from Settings > Apps (the installer registers
// it): close Isle, remove the Start menu shortcut, the startup entry and the
// uninstall entry, optionally the data, then delete the install folder.
unsafe fn uninstall() {
    let yes = 6;
    let q = MessageBoxW(null_mut(), wide("Remove Isle from this PC?").as_ptr(), wide("Uninstall Isle").as_ptr(), MB_YESNO | MB_ICONQUESTION);
    if q != yes {
        return;
    }
    let h = FindWindowW(wide("IsleNativeNotch").as_ptr(), null());
    if !h.is_null() {
        PostMessageW(h, WM_CLOSE, 0, 0);
        for _ in 0..30 {
            if IsWindow(h) == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    set_autostart(false);
    RegDeleteTreeW(HKEY_CURRENT_USER, wide(UNINSTALL_KEY).as_ptr());
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let lnk = std::path::PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs\\Isle.lnk");
        let _ = std::fs::remove_file(lnk);
    }
    let d = MessageBoxW(null_mut(), wide("Also delete your Isle data (tasks, water history, Shelf list)?").as_ptr(), wide("Uninstall Isle").as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2);
    if d == yes {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let _ = std::fs::remove_dir_all(std::path::PathBuf::from(local).join("Isle"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            use std::os::windows::process::CommandExt;
            let cmd = format!("ping 127.0.0.1 -n 3 > NUL & rmdir /s /q \"{}\"", dir.display());
            let _ = std::process::Command::new("cmd").args(["/C", &cmd]).creation_flags(0x0800_0000).spawn();
        }
    }
    MessageBoxW(null_mut(), wide("Isle was removed.").as_ptr(), wide("Uninstall Isle").as_ptr(), MB_OK | MB_ICONINFORMATION);
}

fn main() {
    unsafe {
        if std::env::args().any(|a| a == "--uninstall") {
            uninstall();
            return;
        }
        // one Isle at a time: a second launch just opens the running notch
        CreateMutexW(null(), 0, wide("Local\\IsleNotchSingleInstance").as_ptr());
        if GetLastError() == 183 {
            let h = FindWindowW(wide("IsleNativeNotch").as_ptr(), null());
            if !h.is_null() {
                PostMessageW(h, WM_SHOW_NOTCH, 0, 0);
            }
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        media::com_init_ui();
        gfx::init();
        let mut st = store::load();
        let today = today_string();
        if st.water_date != today {
            store::roll_water(&mut st, &today);
            store::save(&st);
        }
        if st.empty_shelf && !st.shelf.is_empty() {
            st.shelf.clear();
            store::save(&st);
        }
        let shelf_items = build_shelf_items(&st);
        lyrics::ENGLISH.store(st.lyrics_english, std::sync::atomic::Ordering::Relaxed);
        *(&raw mut APP) = Some(App {
            hwnd: null_mut(),
            expanded: false,
            page: 0,
            hover: (0, -1),
            seek_bar: (0, 1),
            notch_off: (0, 0),
            notch_k: 1.0,
            win_pos: (0, 0),
            presenting: false,
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
            edit_index: None,
            water_settings: false,
            water_view: 0,
            drop_hover: false,
            drag_preview: Vec::new(),
            shelf_press: None,
            sliding: false,
            slide_from: 0,
            slide_dir: 1,
            slide_start: 0,
            revealed: false,
            reveal_tick: 0,
            tray_icon: 0,
            notify_page: 3,
            last_own_notify: 0,
            hide_at: 0,
            alert_kind: 0,
            ind_glyph: String::new(),
            ind_title: String::new(),
            ind_sub: String::new(),
            ind_color: 0,
            caps: false,
            num: false,
            scroll: false,
            wifi: None,
            bt: None,
            net_known: false,
            sys: sysinfo::Reading::default(),
            cal_y: 2026,
            cal_m: 1,
            cal_sel: (2026, 1, 1),
            edit_target: 0,
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
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
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
        present();
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        media::spawn_media_thread();

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

// Isle for Windows — native Rust v0.3.0
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
const BLACK: u32 = cr(6, 6, 7);

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

// Settings window, in design units (96-dpi pixels).
const SET_W: i32 = 760;
const SET_H: i32 = 540;
const SIDEBAR_W: i32 = 236;
// (name, icon glyph, tile colour); Water's droplet is drawn by hand.
const SECTIONS: [(&str, &str, u32); 6] = [
    ("General", "\u{e713}", cr(142, 142, 147)),
    ("Music and lyrics", "\u{e8d6}", cr(255, 55, 95)),
    ("Water", "", cr(10, 132, 255)),
    ("Shelf", "\u{e8b5}", cr(255, 179, 0)),
    ("To-do", "\u{e73e}", cr(52, 199, 89)),
    ("About", "\u{e946}", cr(99, 99, 108)),
];

const PAGES: usize = 5; // 0 Music, 1 Pomodoro, 2 Water, 3 Shelf, 4 Todo

// Real-pixel scale of the design (percent). The idle tab stays small; the
// opened notch uses the Settings > Notch size choice (see expanded_scale()).
const COLLAPSED_SCALE: i32 = 85;

// The opened notch has the same height on every page.
const EXPANDED_H: i32 = 232;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "Isle";

// Page icons shown in place of the bottom dots while hovering them.
// Water has no suitable glyph, so it is drawn by hand (empty string).
const PAGE_ICONS: [&str; PAGES] = ["\u{e8d6}", "\u{e916}", "", "\u{e8b5}", "\u{e8fd}"];

struct ShelfItem {
    path: String,
    name: String,
    size: String,
}

struct App {
    hwnd: HWND,
    expanded: bool,
    page: usize,
    lyrics_open: bool,
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

fn clock_strings() -> (String, String) {
    unsafe {
        let mut t: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut t);
        let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let mons = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let date = format!(
            "{} {} {} {}",
            days[t.wDayOfWeek as usize], t.wDay, mons[(t.wMonth - 1) as usize], t.wYear
        );
        let ap = if t.wHour >= 12 { "PM" } else { "AM" };
        let h = if t.wHour % 12 == 0 { 12 } else { t.wHour % 12 };
        let time = format!("{}:{:02} {}", h, t.wMinute, ap);
        (date, time)
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

unsafe fn draw_bolt(hdc: HDC, cx: i32, cy: i32, r: i32) {
    fill_circle(hdc, cx, cy, r, GREEN);
    // bolt as a small polygon, centred
    let s = r as f64 / 10.0;
    let pts = [
        POINT { x: cx + (1.5 * s) as i32, y: cy - (7.0 * s) as i32 },
        POINT { x: cx - (4.0 * s) as i32, y: cy + (1.0 * s) as i32 },
        POINT { x: cx - (0.5 * s) as i32, y: cy + (1.0 * s) as i32 },
        POINT { x: cx - (1.5 * s) as i32, y: cy + (7.0 * s) as i32 },
        POINT { x: cx + (4.0 * s) as i32, y: cy - (1.0 * s) as i32 },
        POINT { x: cx + (0.5 * s) as i32, y: cy - (1.0 * s) as i32 },
    ];
    let brush = CreateSolidBrush(WHITE);
    let pen = CreatePen(PS_NULL as i32, 0, 0);
    let ob = SelectObject(hdc, brush as HGDIOBJ);
    let op = SelectObject(hdc, pen as HGDIOBJ);
    Polygon(hdc, pts.as_ptr(), pts.len() as i32);
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
// Minimized notch: a mini player pill while music plays (when enabled),
// otherwise just the tiny tab.
fn mini_active() -> bool {
    if !app().store.mini_player {
        return false;
    }
    let m = media::snapshot();
    m.present && m.playing
}

fn collapsed_size() -> (i32, i32) {
    // Design units. Idle is just a tiny tab at the top edge; hovering it
    // opens the notch (Ram, 2026-11-01). While music plays it is a wider
    // pill: album art, the live lyric line and a sound wave.
    if mini_active() {
        (340, 34)
    } else {
        (120, 16)
    }
}

fn expanded_size() -> (i32, i32) {
    // Fixed size on every page, Lyrics included.
    (544, EXPANDED_H)
}

// Size of the opened notch in percent: the Settings choice, or for Auto
// the Windows display scaling (clamped to the offered range).
fn expanded_scale() -> i32 {
    match app().store.scale {
        0 => unsafe { (GetDpiForSystem() as i32 * 100 / 96).clamp(100, 150) },
        s => s,
    }
}

// Inline volume slider geometry (design units): sits on the controls row
// between the next-track button and the speaker button.
fn vol_bar_geom() -> (i32, i32) {
    let w = expanded_size().0;
    let bx = w / 2 + 100;
    (bx, (w - 80) - bx)
}

// Hover zone of the bottom dots (small) / page icons (wider once shown).
fn dots_zone(w: i32, h: i32, hover: bool) -> RECT {
    let (zw, zh) = if hover { (170, 34) } else { (90, 26) };
    RECT { left: (w - zw) / 2, top: h - zh, right: (w + zw) / 2, bottom: h }
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
        px_of(collapsed_size(), COLLAPSED_SCALE)
    }
}

// Corner radius (px) of the minimized look for the current state.
fn collapsed_radius() -> i32 {
    let (_, h) = px_of(collapsed_size(), COLLAPSED_SCALE);
    if mini_active() { h } else { 16 }
}

// Top edge stays square (it touches the screen edge); the bottom corners
// are rounded with radius `rad`.
unsafe fn apply_region(hwnd: HWND, w: i32, h: i32, rad: i32) {
    let top = CreateRectRgn(0, 0, w, (h - rad / 2).max(1));
    let round = CreateRoundRectRgn(0, 0, w + 1, h + 1, rad * 2, rad * 2);
    CombineRgn(top, top, round, RGN_OR);
    SetWindowRgn(hwnd, top, 1);
    DeleteObject(round as HGDIOBJ);
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

unsafe fn place_window(hwnd: HWND, w: i32, h: i32) {
    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    SetWindowPos(hwnd, HWND_TOPMOST, (screen_w - w) / 2, 0, w, h, SWP_NOACTIVATE);
}

// Move the notch towards the size of its current state. Size changes
// (open, close, mini player appearing) animate instead of jumping.
unsafe fn apply_size(hwnd: HWND) {
    let a = app();
    let target = cur_size();
    let open_to = if a.expanded { 1.0 } else { 0.0 };
    if target == a.cur_px && !a.animating {
        a.open_level = open_to;
        let rad = if a.expanded { 30 } else { collapsed_radius() };
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
    let rad = lerp_i(collapsed_radius(), 30, a.open_level);
    place_window(hwnd, w, h);
    apply_region(hwnd, w, h, rad);
    a.cur_px = (w, h);
    if t >= 1.0 {
        a.animating = false;
        a.cur_px = a.anim_to;
        a.open_level = a.open_to;
        KillTimer(hwnd, TIMER_ANIM);
        place_window(hwnd, a.cur_px.0, a.cur_px.1);
        let rad = if a.expanded { 30 } else { collapsed_radius() };
        apply_region(hwnd, a.cur_px.0, a.cur_px.1, rad);
    }
    InvalidateRect(hwnd, null(), 0);
    UpdateWindow(hwnd);
}

// ---------- painting ----------
unsafe fn paint(hdc: HDC, w: i32, h: i32, expanded: bool) {
    fill_rect_c(hdc, 0, 0, w, h, BLACK);
    app().actions.clear();
    if !expanded {
        paint_collapsed(hdc, w, h);
        return;
    }
    paint_strip(hdc, w);
    paint_page(hdc, w, h);
    // chevrons
    for (is_prev, cx) in [(true, 22), (false, w - 22)] {
        fill_circle(hdc, cx, h / 2, 13, cr(35, 35, 39));
        draw_text(hdc, if is_prev { "‹" } else { "›" }, cx - 13, h / 2 - 13, 26, 26, "Segoe UI", 17, 400, cr(201, 201, 209), DT_CENTER);
        add_action(cx - 14, h / 2 - 14, 28, 28, if is_prev { A_PREV_PAGE } else { A_NEXT_PAGE }, 0);
    }
    if app().dots_hover {
        paint_page_icons(hdc, w, h);
        return;
    }
    // dots
    let dot_w = 5 * 5 + 4 * 6 + 14; // 5 dots + gaps + elongated active
    let mut dx = (w - dot_w) / 2;
    let dy = h - 15;
    for i in 0..PAGES {
        let dw = if i == app().page { 19 } else { 5 };
        fill_round(hdc, dx, dy, dw, 5, 5, if i == app().page { WHITE } else { cr(67, 67, 75) });
        add_action(dx - 4, dy - 6, dw + 8, 17, A_DOT, i as i32);
        dx += dw + 6;
    }
}

unsafe fn paint_page(hdc: HDC, w: i32, h: i32) {
    match app().page {
        0 => paint_music(hdc, w, h),
        1 => paint_pomo(hdc, w, h),
        2 => paint_water(hdc, w, h),
        3 => paint_shelf(hdc, w, h),
        _ => paint_todo(hdc, w, h),
    }
}

// Hovering the bottom dots swaps them for one icon per page.
unsafe fn paint_page_icons(hdc: HDC, w: i32, h: i32) {
    const SLOT: i32 = 30;
    let x0 = (w - SLOT * PAGES as i32) / 2;
    let cy = h - 17;
    for i in 0..PAGES {
        let x = x0 + i as i32 * SLOT;
        let active = i == app().page;
        if active {
            fill_round(hdc, x + 2, cy - 11, SLOT - 4, 22, 22, cr(52, 52, 58));
        }
        let col = if active { WHITE } else { cr(135, 135, 145) };
        if PAGE_ICONS[i].is_empty() {
            // droplet: round belly + pointed top
            let (dx, dy) = (x + SLOT / 2, cy + 1);
            fill_circle(hdc, dx, dy + 2, 5, col);
            let pts = [
                POINT { x: dx, y: dy - 7 },
                POINT { x: dx - 4, y: dy - 1 },
                POINT { x: dx + 4, y: dy - 1 },
            ];
            let brush = CreateSolidBrush(col);
            let pen = CreatePen(PS_NULL as i32, 0, 0);
            let ob = SelectObject(hdc, brush as HGDIOBJ);
            let op = SelectObject(hdc, pen as HGDIOBJ);
            Polygon(hdc, pts.as_ptr(), pts.len() as i32);
            SelectObject(hdc, ob);
            SelectObject(hdc, op);
            DeleteObject(brush as HGDIOBJ);
            DeleteObject(pen as HGDIOBJ);
        } else {
            draw_text(hdc, PAGE_ICONS[i], x, cy - 11, SLOT, 22, "Segoe MDL2 Assets", 14, 400, col, DT_CENTER);
        }
        add_action(x, h - 31, SLOT, 29, A_DOT, i as i32);
    }
}

unsafe fn paint_collapsed(hdc: HDC, w: i32, h: i32) {
    if mini_active() {
        paint_mini(hdc, w, h);
        return;
    }
    // Just the tab: black sliver + one small green dot as the cue.
    fill_circle(hdc, w / 2, h / 2 + 1, 3, GREEN);
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

// Minimized mini player (design 340 x 34): album art on the left, the live
// lyric line (or the song) in the middle, a moving sound wave on the right.
unsafe fn paint_mini(hdc: HDC, w: i32, h: i32) {
    let m = media::snapshot();
    let lyric = if app().store.mini_lyrics { live_lyric(&m) } else { None };
    if lyric.is_some() {
        // soft red glow behind the lyric, fading out to the black edges
        let (x0, x1) = (40, w - 46);
        let steps = 32;
        for i in 0..steps {
            let t = (i as f64 + 0.5) / steps as f64;
            let k = 1.0 - (2.0 * t - 1.0).abs(); // 0 at the edges, 1 in the middle
            let k = k.powf(0.8);
            let col = cr((6.0 + 56.0 * k) as u32, (6.0 + 8.0 * k) as u32, (7.0 + 16.0 * k) as u32);
            let sx = x0 + (x1 - x0) * i / steps;
            let ex = x0 + (x1 - x0) * (i + 1) / steps;
            fill_rect_c(hdc, sx, 4, ex - sx, h - 8, col);
        }
    }
    draw_art(hdc, 8, 5, h - 10, 7, 12, true);
    let text = match lyric {
        Some(l) => l,
        None => {
            if m.artist.is_empty() { m.title.clone() } else { format!("{} \u{2014} {}", m.title, m.artist) }
        }
    };
    draw_text(hdc, &text, 44, 0, w - 96, h, "Segoe UI", 13, 600, cr(236, 232, 238), DT_CENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
    // sound wave: five bars bouncing at different speeds
    let t = GetTickCount64() as f64 / 1000.0;
    let speeds = [7.1, 9.3, 6.2, 8.4, 5.6];
    let base_x = w - 42;
    for (i, sp) in speeds.iter().enumerate() {
        let k = ((t * sp + i as f64 * 1.7).sin() * 0.5 + 0.5) * 0.85 + 0.15;
        let bh = (4.0 + 14.0 * k) as i32;
        let bx = base_x + i as i32 * 6;
        fill_round(hdc, bx, (h - bh) / 2, 3, bh, 3, cr(196, 164, 214));
    }
}

unsafe fn paint_strip(hdc: HDC, w: i32) {
    let a = app();
    // left title
    match a.page {
        0 if a.lyrics_open => {
            // back to the player
            draw_text(hdc, "\u{e72b}", 16, 0, 20, 38, "Segoe MDL2 Assets", 12, 400, DIM, DT_CENTER);
            draw_text(hdc, "Lyrics", 40, 0, 80, 38, "Segoe UI", 15, 650, WHITE, DT_LEFT);
            add_action(10, 4, 110, 30, A_LYRICS, 0);
        }
        1 => draw_text(hdc, "Pomodoro", 20, 0, 130, 38, "Segoe UI", 15, 650, WHITE, DT_LEFT),
        2 => {
            draw_text(hdc, "Water", 20, 0, 60, 38, "Segoe UI", 15, 650, CYAN, DT_LEFT);
            let ww = text_width("Water ", "Segoe UI", 15, 650, hdc);
            draw_text(hdc, "Reminder", 20 + ww, 0, 90, 38, "Segoe UI", 15, 650, WHITE, DT_LEFT);
        }
        3 => draw_text(hdc, "Shelf", 20, 0, 80, 38, "Segoe UI", 15, 650, WHITE, DT_LEFT),
        4 => {
            draw_text(hdc, "Todo", 20, 0, 50, 38, "Segoe UI", 15, 650, WHITE, DT_LEFT);
            let ww = text_width("Todo", "Segoe UI", 15, 650, hdc);
            draw_text(hdc, "List", 20 + ww, 0, 50, 38, "Segoe UI", 15, 650, GREEN, DT_LEFT);
        }
        _ => {}
    }
    // centre: bolt + date + time
    let (date, time) = clock_strings();
    let dw = text_width(&date, "Segoe UI", 13, 400, hdc);
    let tw = text_width(&time, "Segoe UI", 13, 700, hdc);
    let total = 17 + 7 + dw + 8 + tw;
    let mut x = (w - total) / 2;
    draw_bolt(hdc, x + 8, 19, 8);
    x += 24;
    draw_text(hdc, &date, x, 0, dw + 4, 38, "Segoe UI", 13, 400, cr(232, 232, 236), DT_LEFT);
    x += dw + 8;
    draw_text(hdc, &time, x, 0, tw + 4, 38, "Segoe UI", 13, 700, WHITE, DT_LEFT);
    // right actions (shifted left to make room for the settings gear)
    {
        if a.page == 2 {
            fill_round(hdc, w - 110, 6, 44, 26, 26, CARD2);
            draw_text(hdc, "\u{ea8f}", w - 110, 6, 44, 26, "Segoe MDL2 Assets", 12, 400, WHITE, DT_CENTER);
            add_action(w - 110, 6, 44, 26, A_WATER_BELL, 0);
        } else if a.page == 3 {
            draw_text(hdc, "\u{ea8f}", w - 90, 0, 26, 38, "Segoe MDL2 Assets", 14, 400, WHITE, DT_CENTER);
            fill_circle(hdc, w - 68, 9, 3, RED);
        }
    }
    // settings gear, top right on every page
    let (gx, gy) = (w - 34, 19);
    fill_circle(hdc, gx, gy, 13, if !a.settings_hwnd.is_null() { cr(58, 58, 65) } else { cr(35, 35, 39) });
    draw_text(hdc, "\u{e713}", gx - 13, gy - 13, 26, 26, "Segoe MDL2 Assets", 13, 400, WHITE, DT_CENTER);
    add_action(gx - 14, gy - 14, 28, 28, A_SETTINGS, 0);
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

// Album art square: the real thumbnail when the media session has one,
// otherwise a tinted tile with a music note.
unsafe fn draw_art(hdc: HDC, x: i32, y: i32, size: i32, rad: i32, glyph_size: i32, present: bool) {
    if present {
        if let Some((bmp, bsize)) = thumb_bitmap(hdc) {
            let src = CreateCompatibleDC(hdc);
            let old = SelectObject(src, bmp as HGDIOBJ);
            SetStretchBltMode(hdc, HALFTONE);
            SetBrushOrgEx(hdc, 0, 0, null_mut());
            StretchBlt(hdc, x, y, size, size, src, 0, 0, bsize, bsize, SRCCOPY);
            SelectObject(src, old);
            DeleteDC(src);
            return;
        }
    }
    fill_round(hdc, x, y, size, size, rad, if present { cr(45, 60, 160) } else { CARD2 });
    draw_text(hdc, "\u{e8d6}", x, y, size, size, "Segoe MDL2 Assets", glyph_size, 400, if present { WHITE } else { cr(142, 142, 152) }, DT_CENTER);
}

unsafe fn paint_music(hdc: HDC, w: i32, h: i32) {
    let m = media::snapshot();
    if app().lyrics_open {
        paint_lyrics(hdc, w, h, &m);
        return;
    }
    // art
    draw_art(hdc, 40, 50, 88, 16, 34, m.present);
    // title / artist
    let title = if m.present && !m.title.is_empty() { m.title.clone() } else if m.present { "Playing".into() } else { "Not Playing".into() };
    draw_text(hdc, &title, 146, 62, w - 190, 30, "Segoe UI", 20, 700, WHITE, DT_LEFT);
    if m.present && !m.artist.is_empty() {
        draw_text(hdc, &m.artist.clone(), 146, 94, w - 190, 22, "Segoe UI", 13, 400, DIM, DT_LEFT);
    }
    // seek
    let pos = media::live_position(&m);
    let frac = if m.duration_secs > 0.0 { (pos / m.duration_secs).clamp(0.0, 1.0) } else { 0.0 };
    draw_text(hdc, &fmt_mmss(pos), 40, 148, 40, 20, "Segoe UI", 13, 400, DIM, DT_LEFT);
    draw_text(hdc, &format!("-{}", fmt_mmss(m.duration_secs - pos)), w - 80, 148, 40, 20, "Segoe UI", 13, 400, DIM, DT_RIGHT);
    fill_round(hdc, 88, 155, w - 176, 5, 5, cr(43, 43, 49));
    if frac > 0.0 {
        fill_round(hdc, 88, 155, ((w - 176) as f64 * frac) as i32, 5, 5, cr(233, 233, 238));
    }
    add_action(88, 146, w - 176, 22, A_SEEK, 0);
    // bottom row: lyrics btn / controls / volume
    if m.present {
        draw_text(hdc, "\u{e8f2}", 38, 176, 30, 26, "Segoe MDL2 Assets", 17, 400, cr(142, 142, 152), DT_CENTER);
        add_action(34, 172, 38, 32, A_LYRICS, 0);
    }
    let cx = w / 2;
    let col = if m.present { cr(215, 215, 222) } else { FAINT };
    draw_text(hdc, "\u{e892}", cx - 78, 176, 30, 26, "Segoe MDL2 Assets", 17, 400, col, DT_CENTER);
    draw_text(hdc, if m.playing { "\u{e769}" } else { "\u{e768}" }, cx - 15, 176, 30, 26, "Segoe MDL2 Assets", 18, 400, col, DT_CENTER);
    draw_text(hdc, "\u{e893}", cx + 48, 176, 30, 26, "Segoe MDL2 Assets", 17, 400, col, DT_CENTER);
    if m.present {
        add_action(cx - 82, 172, 38, 32, A_PREV_TRACK, 0);
        add_action(cx - 19, 172, 38, 32, A_PLAYPAUSE, 0);
        add_action(cx + 44, 172, 38, 32, A_NEXT_TRACK, 0);
    }
    // speaker button (click = mute) with the slider on the same row,
    // between the next-track button and the speaker
    let (lvl, muted) = (app().vol_level, app().vol_muted);
    let silent = muted || lvl <= 0.0;
    draw_text(hdc, if silent { "\u{e74f}" } else { "\u{e767}" }, w - 68, 176, 30, 26, "Segoe MDL2 Assets", 17, 400, col, DT_CENTER);
    if m.present {
        add_action(w - 72, 172, 38, 32, A_VOL_MUTE, 0);
        let (bx, bw) = vol_bar_geom();
        let by = 186; // 6 tall, centred on the row (centre y = 189)
        fill_round(hdc, bx, by, bw, 6, 6, cr(43, 43, 49));
        let fw = if silent { 0 } else { (bw as f32 * lvl) as i32 };
        if fw > 0 {
            fill_round(hdc, bx, by, fw.max(6), 6, 6, cr(233, 233, 238));
        }
        fill_circle(hdc, bx + fw.clamp(3, bw - 3), by + 3, 5, WHITE);
        add_action(bx - 6, 172, bw + 12, 32, A_VOL_BAR, 0);
    }
}

// Lyrics page: only the lyrics, across the whole notch. The current line
// is big and white; click any line to jump there.
unsafe fn paint_lyrics(hdc: HDC, w: i32, h: i32, m: &media::MediaInfo) {
    let (px, py) = (48, 42);
    let (pw, ph) = (w - 96, h - 42 - 28);
    // dark red wash, strongest in the middle row
    let steps = 24;
    for i in 0..steps {
        let t = (i as f64 + 0.5) / steps as f64;
        let k = (1.0 - (2.0 * t - 1.0).abs()).powf(0.7);
        let col = cr((10.0 + 42.0 * k) as u32, (8.0 + 6.0 * k) as u32, (9.0 + 14.0 * k) as u32);
        fill_rect_c(hdc, px - 34, py + ph * i / steps, pw + 68, ph / steps + 1, col);
    }
    match lyrics::state() {
        lyrics::LyricsState::Loading | lyrics::LyricsState::Idle => {
            draw_text(hdc, "Loading lyrics", px, py, pw, ph, "Segoe UI", 15, 600, WHITE, DT_CENTER);
        }
        lyrics::LyricsState::NotAvailable => {
            draw_text(hdc, "Lyrics not available", px, py + ph / 2 - 26, pw, 28, "Segoe UI", 15, 600, WHITE, DT_CENTER);
            let why = lyrics::info();
            if !why.is_empty() {
                draw_text(hdc, &why, px, py + ph / 2 + 4, pw, 22, "Segoe UI", 11, 400, DIM, DT_CENTER | DT_END_ELLIPSIS);
            }
        }
        lyrics::LyricsState::Ready(lines) => {
            if lyrics::info() == "unsynced lyrics" {
                draw_text(hdc, "unsynced", px - 26, py + 2, 90, 18, "Segoe UI", 10, 400, FAINT, DT_LEFT);
            }
            let cur = lyrics::current_line(&lines, media::live_position(m));
            let row_h = 30;
            let mid_y = py + ph / 2 - row_h / 2;
            for slot in -2i32..=2 {
                let idx = cur as i32 + slot;
                if idx < 0 || idx as usize >= lines.len() {
                    continue;
                }
                let (t, line) = &lines[idx as usize];
                let (size, weight, col) = match slot.abs() {
                    0 => (19, 700, WHITE),
                    1 => (15, 550, cr(190, 170, 178)),
                    _ => (14, 500, cr(120, 108, 116)),
                };
                let y = mid_y + slot * row_h;
                let text = if line.trim().is_empty() { "\u{266a}" } else { line.as_str() };
                draw_text(hdc, text, px, y, pw, row_h, "Segoe UI", size, weight, col, DT_CENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
                add_action(px, y, pw, row_h, A_LYRIC_LINE, (*t * 10.0) as i32);
            }
        }
    }
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

unsafe fn paint_pomo(hdc: HDC, w: i32, _h: i32) {
    let a = app();
    // glowing red disc with stopwatch glyph
    fill_circle(hdc, 104, 118, 60, cr(58, 13, 11));
    fill_circle(hdc, 104, 118, 55, cr(214, 32, 39));
    fill_circle(hdc, 104, 118, 48, cr(232, 37, 27));
    draw_text(hdc, "\u{e916}", 104 - 30, 118 - 30, 60, 60, "Segoe MDL2 Assets", 38, 400, WHITE, DT_CENTER);
    // time + reset
    let t = fmt_mmss(a.pomo_left as f64);
    let tw = text_width(&t, "Segoe UI", 40, 750, hdc);
    let start_x = 240 + (w - 280 - tw - 34) / 2;
    draw_text(hdc, &t, start_x, 52, tw + 6, 50, "Segoe UI", 40, 750, WHITE, DT_LEFT);
    draw_text(hdc, "\u{e777}", start_x + tw + 8, 62, 28, 34, "Segoe MDL2 Assets", 18, 400, cr(215, 215, 222), DT_CENTER);
    add_action(start_x + tw + 4, 58, 34, 40, A_POMO_RESET, 0);
    let label = a.pomo_label.clone();
    draw_text(hdc, &label, 240, 106, w - 280, 22, "Segoe UI", 14, 400, DIM, DT_CENTER);
    // actions row
    let y = 140;
    let (short_on, focus_on) = (a.pomo_total == 300, a.pomo_total == 1500);
    fill_round(hdc, 196, y, 110, 34, 34, if short_on { cr(58, 58, 65) } else { CARD2 });
    draw_text(hdc, "Short Break", 196, y, 110, 34, "Segoe UI", 13, 500, cr(232, 232, 236), DT_CENTER);
    add_action(196, y, 110, 34, A_POMO_MODE, 5);
    fill_round(hdc, 314, y, 64, 34, 34, RED);
    draw_text(hdc, if a.pomo_running { "\u{e769}" } else { "\u{e768}" }, 314, y, 64, 34, "Segoe MDL2 Assets", 15, 400, WHITE, DT_CENTER);
    add_action(314, y, 64, 34, A_POMO_PLAY, 0);
    fill_round(hdc, 386, y, 110, 34, 34, if focus_on { cr(58, 58, 65) } else { CARD2 });
    draw_text(hdc, "Focus Time", 386, y, 110, 34, "Segoe UI", 13, 500, cr(232, 232, 236), DT_CENTER);
    add_action(386, y, 110, 34, A_POMO_MODE, 25);
}

unsafe fn paint_water(hdc: HDC, w: i32, _h: i32) {
    let a = app();
    let ml = a.store.water_ml;
    let pct = ((ml as f64 / 1500.0) * 100.0).round() as i32;
    let amt = format!("{:.1}", ml as f64 / 1000.0);
    let aw = text_width(&amt, "Segoe UI", 36, 750, hdc);
    draw_text(hdc, &amt, 40, 46, aw + 6, 46, "Segoe UI", 36, 750, WHITE, DT_LEFT);
    draw_text(hdc, "/ 1.5 L", 46 + aw, 58, 70, 32, "Segoe UI", 16, 400, DIM, DT_LEFT);
    fill_round(hdc, 40, 100, 290, 6, 6, CARD2);
    if pct > 0 {
        fill_round(hdc, 40, 100, (290.0 * pct as f64 / 100.0) as i32, 6, 6, BLUE);
    }
    let sub = format!("Today · {}% · Reminders {}", pct, if a.store.reminders { "on" } else { "off" });
    draw_text(hdc, &sub, 40, 112, 290, 20, "Segoe UI", 12, 400, DIM, DT_LEFT);
    // Drink button with droplet
    fill_round(hdc, 40, 140, 104, 34, 34, CARD2);
    let pts = [
        POINT { x: 58, y: 148 },
        POINT { x: 50, y: 162 },
        POINT { x: 53, y: 168 },
        POINT { x: 63, y: 168 },
        POINT { x: 66, y: 162 },
    ];
    let brush = CreateSolidBrush(CYAN);
    let pen = CreatePen(PS_NULL as i32, 0, 0);
    let ob = SelectObject(hdc, brush as HGDIOBJ);
    let op = SelectObject(hdc, pen as HGDIOBJ);
    Polygon(hdc, pts.as_ptr(), pts.len() as i32);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    DeleteObject(brush as HGDIOBJ);
    DeleteObject(pen as HGDIOBJ);
    draw_text(hdc, "Drink", 70, 140, 64, 34, "Segoe UI", 13, 600, cr(124, 196, 255), DT_LEFT);
    add_action(40, 140, 104, 34, A_DRINK, 0);
    fill_round(hdc, 152, 140, 118, 34, 34, CARD2);
    draw_text(hdc, "\u{e713}", 162, 140, 24, 34, "Segoe MDL2 Assets", 13, 400, WHITE, DT_CENTER);
    draw_text(hdc, "Settings", 186, 140, 78, 34, "Segoe UI", 13, 500, WHITE, DT_LEFT);
    add_action(152, 140, 118, 34, A_WATER_BELL, 0);
    // gauge
    let (cx, cy, r) = (w - 112, 116, 50);
    draw_arc(hdc, cx, cy, r, 135.0, 270.0, 9, CARD2);
    if pct > 0 {
        draw_arc(hdc, cx, cy, r, 135.0, 270.0 * pct as f64 / 100.0, 9, BLUE);
    }
    let ptxt = format!("{}", pct);
    draw_text(hdc, &ptxt, cx - 30, cy - 26, 60, 36, "Segoe UI", 30, 750, WHITE, DT_CENTER);
    draw_text(hdc, "%", cx - 30, cy + 12, 60, 18, "Segoe UI", 12, 400, DIM, DT_CENTER);
}

unsafe fn paint_shelf(hdc: HDC, w: i32, h: i32) {
    let a = app();
    if a.shelf_items.is_empty() {
        let pen = CreatePen(PS_DASH as i32, 1, cr(61, 61, 70));
        let brush = GetStockObject(NULL_BRUSH as i32);
        let op = SelectObject(hdc, pen as HGDIOBJ);
        let ob = SelectObject(hdc, brush as HGDIOBJ);
        RoundRect(hdc, 40, 50, w - 40, h - 34, 18, 18);
        SelectObject(hdc, op);
        SelectObject(hdc, ob);
        DeleteObject(pen as HGDIOBJ);
        draw_text(hdc, "\u{e8b5}", 40, 66, w - 80, 34, "Segoe MDL2 Assets", 26, 400, cr(201, 201, 209), DT_CENTER);
        draw_text(hdc, "Drop files here", 40, 104, w - 80, 24, "Segoe UI", 16, 650, WHITE, DT_CENTER);
        draw_text(hdc, "Drag out anytime · click to open or copy", 40, 130, w - 80, 20, "Segoe UI", 12, 400, DIM, DT_CENTER);
        return;
    }
    draw_text(hdc, "Drop more files anywhere on the notch", 40, 44, w - 80, 20, "Segoe UI", 12, 400, FAINT, DT_CENTER);
    let mut y = 70;
    let mut shown = 0;
    for (i, item) in a.shelf_items.iter().enumerate() {
        shown += 1;
        fill_round(hdc, 44, y, w - 88, 34, 11, CARD);
        draw_text(hdc, "\u{e8a5}", 54, y, 24, 34, "Segoe MDL2 Assets", 14, 400, DIM, DT_CENTER);
        draw_text(hdc, &item.name.clone(), 82, y, w - 200, 34, "Segoe UI", 13, 500, WHITE, DT_LEFT);
        draw_text(hdc, &item.size.clone(), w - 130, y, 62, 34, "Segoe UI", 11, 400, FAINT, DT_RIGHT);
        draw_text(hdc, "✕", w - 66, y, 20, 34, "Segoe UI", 11, 400, FAINT, DT_CENTER);
        add_action(44, y, w - 112, 34, A_SHELF_OPEN, i as i32);
        add_action(w - 68, y, 24, 34, A_SHELF_REMOVE, i as i32);
        y += 40;
        if y > h - 50 {
            break;
        }
    }
    // the notch height is fixed, so say when more files don't fit
    if shown < a.shelf_items.len() {
        let more = format!("+{} more", a.shelf_items.len() - shown);
        draw_text(hdc, &more, 40, h - 50, w - 80, 16, "Segoe UI", 11, 400, FAINT, DT_CENTER);
    }
}

unsafe fn paint_todo(hdc: HDC, w: i32, h: i32) {
    let a = app();
    let done = a.store.tasks.iter().filter(|t| t.done).count();
    let count = format!("{}/{}", done, a.store.tasks.len());
    draw_text(hdc, &count, 40, 48, 120, 52, "Segoe UI", 42, 750, WHITE, DT_LEFT);
    let cw = text_width(&count, "Segoe UI", 42, 750, hdc);
    fill_circle(hdc, 52 + cw + 18, 74, 18, CARD2);
    draw_text(hdc, "\u{e787}", 52 + cw, 56, 36, 36, "Segoe MDL2 Assets", 15, 400, WHITE, DT_CENTER);
    fill_circle(hdc, 52 + cw + 30, 62, 4, BLUE);
    draw_text(hdc, "Task Done Today", 40, 104, 170, 20, "Segoe UI", 13, 400, DIM, DT_LEFT);
    fill_round(hdc, 40, 136, 112, 34, 34, CARD2);
    draw_text(hdc, "\u{e710}", 50, 136, 22, 34, "Segoe MDL2 Assets", 12, 400, WHITE, DT_CENTER);
    draw_text(hdc, "Add Task", 72, 136, 76, 34, "Segoe UI", 13, 500, WHITE, DT_LEFT);
    add_action(40, 136, 112, 34, A_TODO_ADD, 0);
    fill_round(hdc, 160, 136, 112, 34, 34, CARD2);
    draw_text(hdc, "\u{e70f}", 170, 136, 22, 34, "Segoe MDL2 Assets", 12, 400, WHITE, DT_CENTER);
    draw_text(hdc, "Edit Task", 192, 136, 76, 34, "Segoe UI", 13, 500, WHITE, DT_LEFT);
    add_action(160, 136, 112, 34, A_TODO_EDIT, 0);
    // right panel
    let px = 292;
    let pw = w - px - 40;
    fill_round(hdc, px, 50, pw, h - 84, 18, cr(27, 27, 30));
    if a.store.tasks.is_empty() {
        draw_text(hdc, "\u{e8fd}", px, 76, pw, 34, "Segoe MDL2 Assets", 26, 400, cr(85, 85, 95), DT_CENTER);
        draw_text(hdc, "No tasks.", px, 116, pw, 22, "Segoe UI", 13, 400, DIM, DT_CENTER);
        return;
    }
    let mut y = 60;
    // while the inline editor is open it covers the bottom of the panel
    let editing_box = !a.edit_hwnd.is_null() && IsWindowVisible(a.edit_hwnd) != 0;
    let y_limit = if editing_box { h - 120 } else { h - 76 };
    for (i, t) in a.store.tasks.iter().enumerate() {
        if y > y_limit {
            break;
        }
        fill_round(hdc, px + 10, y, pw - 20, 34, 11, if t.done { cr(30, 58, 36) } else { CARD2 });
        if t.done {
            draw_text(hdc, "\u{e73e}", px + 16, y, 22, 34, "Segoe MDL2 Assets", 12, 400, GREEN, DT_CENTER);
        } else {
            let pen = CreatePen(PS_SOLID as i32, 1, cr(90, 90, 100));
            let brush = GetStockObject(NULL_BRUSH as i32);
            let op = SelectObject(hdc, pen as HGDIOBJ);
            let ob = SelectObject(hdc, brush as HGDIOBJ);
            RoundRect(hdc, px + 19, y + 8, px + 35, y + 24, 5, 5);
            SelectObject(hdc, op);
            SelectObject(hdc, ob);
            DeleteObject(pen as HGDIOBJ);
        }
        draw_text(hdc, &t.title.clone(), px + 44, y, pw - 90, 34, "Segoe UI", 13, 500, if t.done { DIM } else { WHITE }, DT_LEFT);
        add_action(px + 10, y, pw - 20, 34, A_TODO_TOGGLE, i as i32);
        if a.editing {
            draw_text(hdc, "✕", px + pw - 36, y, 20, 34, "Segoe UI", 11, 400, cr(255, 107, 97), DT_CENTER);
            add_action(px + pw - 38, y, 24, 34, A_TODO_DELETE, i as i32);
        }
        y += 40;
    }
}

// ---------- actions ----------
unsafe fn hide_edit() {
    let a = app();
    if !a.edit_hwnd.is_null() && IsWindowVisible(a.edit_hwnd) != 0 {
        ShowWindow(a.edit_hwnd, SW_HIDE);
    }
}

unsafe fn goto_page(hwnd: HWND, page: usize) {
    let a = app();
    a.page = page % PAGES;
    a.lyrics_open = false;
    // the inline todo editor belongs to the todo page only
    if a.page != 4 {
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
        A_PREV_PAGE => goto_page(hwnd, (a.page + PAGES - 1) % PAGES),
        A_NEXT_PAGE => goto_page(hwnd, (a.page + 1) % PAGES),
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
            store::save(&a.store);
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
                let (bx, bw) = (88, expanded_size().0 - 176);
                let frac = ((x - bx) as f64 / bw as f64).clamp(0.0, 1.0);
                media::seek(frac * m.duration_secs);
            }
        }
        A_LYRICS => {
            a.lyrics_open = !a.lyrics_open;
            if a.lyrics_open {
                let m = media::snapshot();
                if matches!(lyrics::state(), lyrics::LyricsState::NotAvailable) {
                    lyrics::refetch(&m.title, &m.artist, m.duration_secs);
                } else {
                    lyrics::ensure_fetched(&m.title, &m.artist, m.duration_secs);
                }
            }
            apply_size(hwnd);
        }
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
        A_POMO_MODE => {
            a.pomo_total = (param as u32) * 60;
            a.pomo_left = a.pomo_total;
            a.pomo_running = false;
            a.pomo_label = if param == 5 { "Break time".into() } else { "Focus time".into() };
        }
        A_DRINK => {
            if a.store.water_ml < 1500 {
                a.store.water_ml = (a.store.water_ml + 250).min(1500);
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
    if a.page != 4 {
        a.page = 4;
        apply_size(hwnd);
    }
    let (dw, dh) = expanded_size();
    let (pw, ph) = cur_size();
    // place the edit inside the right panel, bottom area (design -> physical)
    SetWindowPos(a.edit_hwnd, null_mut(), 302 * pw / dw, (dh - 78) * ph / dh, (dw - 352) * pw / dw, 34 * ph / dh, SWP_NOZORDER | SWP_SHOWWINDOW);
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
        a.lyrics_open = false;
        a.dots_hover = false;
        apply_size(hwnd);
    }
}

// ---------- settings window ----------
// Its own window (sidebar of sections + cards on the right), opened from
// the gear on the notch. Design units are 96-dpi pixels, scaled to the
// Windows display scaling.
const CARD_S: u32 = cr(40, 40, 44);
const ROW_H: i32 = 56;

fn settings_scale() -> i32 {
    unsafe { (GetDpiForSystem() as i32 * 100 / 96).max(100) }
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
    let rad = 24 * sc / 100;
    SetWindowRgn(hwnd, CreateRoundRectRgn(0, 0, w + 1, h + 1, rad, rad), 1);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    InvalidateRect(a.hwnd, null(), 0);
}

unsafe fn draw_droplet(hdc: HDC, dx: i32, dy: i32, col: u32) {
    fill_circle(hdc, dx, dy + 2, 5, col);
    let pts = [
        POINT { x: dx, y: dy - 7 },
        POINT { x: dx - 4, y: dy - 1 },
        POINT { x: dx + 4, y: dy - 1 },
    ];
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

unsafe fn set_header(hdc: HDC, x0: i32, cw: i32, y: &mut i32, label: &str) {
    draw_text(hdc, label, x0 + 4, *y, cw, 20, "Segoe UI", 12, 600, DIM, DT_LEFT);
    *y += 24;
}

unsafe fn row_text(hdc: HDC, x: i32, y: i32, w: i32, title: &str, sub: &str, enabled: bool) {
    let tc = if enabled { WHITE } else { FAINT };
    if sub.is_empty() {
        draw_text(hdc, title, x + 16, y, w - 32, ROW_H, "Segoe UI", 14, 500, tc, DT_LEFT | DT_END_ELLIPSIS);
    } else {
        draw_text(hdc, title, x + 16, y + 9, w - 32, 20, "Segoe UI", 14, 500, tc, DT_LEFT | DT_END_ELLIPSIS);
        draw_text(hdc, sub, x + 16, y + 29, w - 32, 18, "Segoe UI", 12, 400, if enabled { DIM } else { FAINT }, DT_LEFT | DT_END_ELLIPSIS);
    }
}

unsafe fn toggle_switch(hdc: HDC, tx: i32, ty: i32, on: bool, enabled: bool) {
    let track = match (on, enabled) {
        (true, true) => GREEN,
        (true, false) => cr(38, 96, 54),
        _ => cr(64, 64, 70),
    };
    fill_round(hdc, tx, ty, 44, 24, 24, track);
    fill_circle(hdc, if on { tx + 32 } else { tx + 12 }, ty + 12, 9, if enabled { WHITE } else { cr(150, 150, 155) });
}

// One toggle row inside a card that starts at `y`.
unsafe fn toggle_row(hdc: HDC, x0: i32, cw: i32, y: i32, title: &str, sub: &str, on: bool, enabled: bool, code: i32) {
    row_text(hdc, x0, y, cw - 70, title, sub, enabled);
    toggle_switch(hdc, x0 + cw - 16 - 44, y + 16, on, enabled);
    if enabled {
        add_action(x0, y, cw, ROW_H, code, 0);
    }
}

unsafe fn toggle_card(hdc: HDC, x0: i32, cw: i32, y: &mut i32, title: &str, sub: &str, on: bool, code: i32) {
    fill_round(hdc, x0, *y, cw, ROW_H, 14, CARD_S);
    toggle_row(hdc, x0, cw, *y, title, sub, on, true, code);
    *y += ROW_H + 18;
}

unsafe fn button_card(hdc: HDC, x0: i32, cw: i32, y: &mut i32, title: &str, sub: &str, btn: &str, code: i32) {
    fill_round(hdc, x0, *y, cw, ROW_H, 14, CARD_S);
    let bw = text_width(btn, "Segoe UI", 13, 500, hdc) + 32;
    row_text(hdc, x0, *y, cw - bw - 24, title, sub, true);
    let bx = x0 + cw - 16 - bw;
    fill_round(hdc, bx, *y + 13, bw, 30, 30, cr(62, 62, 68));
    draw_text(hdc, btn, bx, *y + 13, bw, 30, "Segoe UI", 13, 500, WHITE, DT_CENTER);
    add_action(bx, *y + 13, bw, 30, code, 0);
    *y += ROW_H + 18;
}

unsafe fn paint_settings_window(hdc: HDC, w: i32, h: i32) {
    let a = app();
    a.set_actions.clear();
    a.painting_settings = true;
    fill_rect_c(hdc, 0, 0, w, h, cr(24, 24, 26));
    fill_rect_c(hdc, 0, 0, SIDEBAR_W, h, cr(32, 32, 35));
    fill_rect_c(hdc, SIDEBAR_W, 0, 1, h, cr(46, 46, 50));
    // close
    fill_circle(hdc, 26, 24, 7, cr(255, 95, 87));
    add_action(14, 12, 26, 26, S_CLOSE, 0);
    // sidebar: sections, About pinned to the bottom
    let sec = a.settings_section.min(SECTIONS.len() - 1);
    for (i, (name, glyph, col)) in SECTIONS.iter().enumerate() {
        let y = if i == SECTIONS.len() - 1 { h - 58 } else { 52 + i as i32 * 42 };
        let selected = i == sec;
        if selected {
            fill_round(hdc, 12, y, SIDEBAR_W - 24, 38, 12, cr(52, 52, 57));
        }
        fill_round(hdc, 22, y + 6, 26, 26, 9, *col);
        if glyph.is_empty() {
            draw_droplet(hdc, 35, y + 19, WHITE);
        } else {
            draw_text(hdc, glyph, 22, y + 6, 26, 26, "Segoe MDL2 Assets", 13, 400, WHITE, DT_CENTER);
        }
        draw_text(hdc, name, 60, y, SIDEBAR_W - 72, 38, "Segoe UI", 14, if selected { 600 } else { 400 }, WHITE, DT_LEFT);
        add_action(12, y, SIDEBAR_W - 24, 38, S_SECTION, i as i32);
    }
    // content
    let x0 = SIDEBAR_W + 28;
    let cw = w - x0 - 28;
    draw_text(hdc, SECTIONS[sec].0, x0, 22, cw, 40, "Segoe UI", 22, 700, WHITE, DT_LEFT);
    let mut y = 76;
    match sec {
        0 => {
            set_header(hdc, x0, cw, &mut y, "Startup");
            toggle_card(hdc, x0, cw, &mut y, "Start with Windows", "Open Isle when you sign in", a.autostart, A_AUTOSTART);
            set_header(hdc, x0, cw, &mut y, "Notch");
            fill_round(hdc, x0, y, cw, 100, 14, CARD_S);
            row_text(hdc, x0, y, cw, "Size", "How big the opened notch is", true);
            let (sx0, sw_all) = (x0 + 16, cw - 32);
            fill_round(hdc, sx0, y + 56, sw_all, 30, 30, cr(26, 26, 29));
            let choices: [(i32, &str); 5] = [(0, "Auto"), (100, "100%"), (115, "115%"), (130, "130%"), (150, "150%")];
            for (i, (val, label)) in choices.iter().enumerate() {
                let sx = sx0 + sw_all * i as i32 / 5;
                let segw = sx0 + sw_all * (i as i32 + 1) / 5 - sx;
                let on = a.store.scale == *val;
                if on {
                    fill_round(hdc, sx + 2, y + 58, segw - 4, 26, 26, cr(88, 88, 96));
                }
                draw_text(hdc, label, sx, y + 56, segw, 30, "Segoe UI", 12, if on { 650 } else { 500 }, if on { WHITE } else { DIM }, DT_CENTER);
                add_action(sx, y + 56, segw, 30, A_SET_SCALE, *val);
            }
            y += 100 + 18;
            set_header(hdc, x0, cw, &mut y, "Sounds");
            toggle_card(hdc, x0, cw, &mut y, "Reminder sounds", "Chimes for Pomodoro and Water", a.store.sounds, A_SOUNDS);
            set_header(hdc, x0, cw, &mut y, "Full screen");
            toggle_card(hdc, x0, cw, &mut y, "Hide in full screen", "Games, videos and slides that fill the screen", a.store.hide_fs, A_HIDE_FS);
        }
        1 => {
            set_header(hdc, x0, cw, &mut y, "Minimized notch");
            fill_round(hdc, x0, y, cw, ROW_H * 2 + 1, 14, CARD_S);
            toggle_row(hdc, x0, cw, y, "Mini player", "Album art and a sound wave while music plays", a.store.mini_player, true, A_MINI_PLAYER);
            fill_rect_c(hdc, x0 + 16, y + ROW_H, cw - 32, 1, cr(54, 54, 59));
            toggle_row(hdc, x0, cw, y + ROW_H + 1, "Lyrics in the mini player", "The current lyric line instead of the song name", a.store.mini_lyrics, a.store.mini_player, A_MINI_LYRICS);
            y += ROW_H * 2 + 1 + 18;
            set_header(hdc, x0, cw, &mut y, "Lyrics");
            fill_round(hdc, x0, y, cw, ROW_H, 14, CARD_S);
            row_text(hdc, x0, y, cw, "Synced lyrics from LRCLIB", "Open Lyrics from the player; click a line to jump to it", true);
        }
        2 => {
            set_header(hdc, x0, cw, &mut y, "Reminders");
            toggle_card(hdc, x0, cw, &mut y, "Water reminders", "Daily goal 1.5 L, 250 ml per drink", a.store.reminders, A_WATER_BELL);
            set_header(hdc, x0, cw, &mut y, "Today");
            let today = format!("{:.2} L of 1.5 L", a.store.water_ml as f64 / 1000.0);
            button_card(hdc, x0, cw, &mut y, &today, "Starts again at zero every day", "Reset", A_WATER_RESET);
        }
        3 => {
            set_header(hdc, x0, cw, &mut y, "Files");
            let n = a.shelf_items.len();
            let t = format!("{} file{} on the shelf", n, if n == 1 { "" } else { "s" });
            button_card(hdc, x0, cw, &mut y, &t, "Drop files on the notch to add them", "Clear shelf", A_SHELF_CLEAR);
        }
        4 => {
            set_header(hdc, x0, cw, &mut y, "Tasks");
            let done = a.store.tasks.iter().filter(|t| t.done).count();
            let t = format!("{} of {} done", done, a.store.tasks.len());
            button_card(hdc, x0, cw, &mut y, &t, "Add, tick and edit tasks on the notch", "Clear completed", A_TODO_CLEAR_DONE);
        }
        _ => {
            draw_text(hdc, "Isle", x0, y, cw, 44, "Segoe UI", 30, 750, WHITE, DT_LEFT);
            y += 44;
            let ver = format!("Version {}", env!("CARGO_PKG_VERSION"));
            draw_text(hdc, &ver, x0, y, cw, 20, "Segoe UI", 13, 400, DIM, DT_LEFT);
            y += 22;
            draw_text(hdc, "A living notch for Windows, native Rust.", x0, y, cw, 20, "Segoe UI", 13, 400, DIM, DT_LEFT);
            y += 40;
            button_card(hdc, x0, cw, &mut y, "Source code", "github.com/Sree-Ram-MS/Isle-Dynamic-Island-Extension", "Open", A_OPEN_REPO);
            draw_text(hdc, "Inspired by Nusa for Windows. Right-click the notch to quit.", x0 + 4, y, cw, 20, "Segoe UI", 12, 400, FAINT, DT_LEFT);
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
                    S_SECTION => {
                        app().settings_section = param as usize;
                        if param == 0 {
                            app().autostart = autostart_enabled();
                        }
                        InvalidateRect(hwnd, null(), 0);
                    }
                    _ => do_action(code, param, x, y, app().hwnd),
                }
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
    painter(mem);
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
    let (fw, fh) = if expanded { px_of((dw, dh), expanded_scale()) } else { px_of((dw, dh), COLLAPSED_SCALE) };
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
                a.expanded = true;
                apply_size(hwnd);
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
            let over = x >= z.left && x < z.right && y >= z.top && y < z.bottom;
            if over != a.dots_hover {
                a.dots_hover = over;
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_MOUSELEAVE_MSG => {
            let a = app();
            a.tracking = false;
            if a.dots_hover {
                a.dots_hover = false;
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
            if a.page != 3 {
                a.page = 3;
            }
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
            if msg == WM_MOUSEWHEEL && a.page == 0 && !a.lyrics_open && media::snapshot().present {
                let (bx, bw) = vol_bar_geom();
                if y >= 168 && y < 208 && x >= bx - 16 && x < bx + bw + 16 {
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
                    goto_page(hwnd, (a.page as i32 + dir).rem_euclid(PAGES as i32) as usize);
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
                    // auto-switch focus <-> break, paused, like the original cycle
                    if a.pomo_total == 1500 {
                        a.pomo_total = 300;
                        a.pomo_label = "Break time".into();
                    } else {
                        a.pomo_total = 1500;
                        a.pomo_label = "Focus time".into();
                    }
                    a.pomo_left = a.pomo_total;
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
            // water day rollover
            let today = today_string();
            if a.store.water_date != today {
                a.store.water_date = today;
                a.store.water_ml = 0;
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
            if m.present && !m.title.is_empty() {
                lyrics::ensure_fetched(&m.title, &m.artist, m.duration_secs);
            }
            // the minimized look follows playback: the mini player slides
            // in when music starts and back to the tab when it stops
            if !a.expanded && !a.animating && cur_size() != a.cur_px {
                apply_size(hwnd);
            }
            let want_wave = !a.expanded && mini_active();
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
        }
        let shelf_items = build_shelf_items(&st);
        *(&raw mut APP) = Some(App {
            hwnd: null_mut(),
            expanded: false,
            page: 0,
            lyrics_open: false,
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
            pomo_total: 1500,
            pomo_left: 1500,
            pomo_running: false,
            pomo_label: "Focus time".into(),
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
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            wide("Isle").as_ptr(),
            WS_POPUP,
            (screen_w - w) / 2, 0, w, h,
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

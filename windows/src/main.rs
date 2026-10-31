// Isle for Windows — native Rust v0.2.0
// The full app on the spike's skeleton: one top-edge window, one UI thread,
// GDI drawing (double-buffered), real functionality per page:
//   Music    — WinRT GSMTC (media.rs) + LRCLIB synced lyrics split-pane
//   Pomodoro — real countdown, Focus 25 / Short Break 5
//   Water    — 250 ml per Drink against a 1.5 L goal, persisted per day
//   Shelf    — real drag-in (WM_DROPFILES), click to open, persisted
//   Todo     — add (inline edit) / toggle done / edit-delete, persisted
// Data: %LOCALAPPDATA%\Isle\data.txt (store.rs). No web engine anywhere.

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
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetFocus, TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE};
use windows_sys::Win32::UI::Shell::{
    DefSubclassProc, DragAcceptFiles, DragFinish, DragQueryFileW, SetWindowSubclass, ShellExecuteW,
    HDROP,
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
const A_VOL_TOGGLE: i32 = 16;
const A_VOL_BAR: i32 = 17;
const A_VOL_MUTE: i32 = 18;
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

const PAGES: usize = 5; // 0 Music, 1 Pomodoro, 2 Water, 3 Shelf, 4 Todo

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
    tracking: bool,
    fonts: Vec<(String, i32, i32, isize)>,
    actions: Vec<(RECT, i32, i32)>,
    // pomodoro
    pomo_total: u32,
    pomo_left: u32,
    pomo_running: bool,
    pomo_label: String,
    // water
    reminders: bool,
    vol_open: bool,
    vol_level: f32,
    vol_muted: bool,
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
    app().actions.push((RECT { left: x, top: y, right: x + w, bottom: y + h }, code, param));
}

// ---------- sizing ----------
fn collapsed_size() -> (i32, i32) {
    // Design units. Idle is just a tiny tab at the top edge — no clock,
    // no mini player. Hovering it opens the notch (Ram, 2026-11-01).
    (120, 16)
}

fn expanded_size() -> (i32, i32) {
    let a = app();
    if a.page == 0 && a.lyrics_open {
        return (664, 208);
    }
    let h = match a.page {
        0 => if a.vol_open { 260 } else { 218 },
        1 => 198,
        2 => 198,
        3 => {
            if a.shelf_items.is_empty() {
                196
            } else {
                (150 + a.shelf_items.len() as i32 * 40).min(330)
            }
        }
        _ => 198,
    };
    (544, h)
}

fn cur_design() -> (i32, i32) {
    if app().expanded {
        expanded_size()
    } else {
        collapsed_size()
    }
}

// The window's real pixels: the design at 0.85 scale (DPI-aware, so this
// is true device pixels, not Windows' blurry bitmap scaling).
fn cur_size() -> (i32, i32) {
    let (w, h) = cur_design();
    ((w * 85 + 50) / 100, (h * 85 + 50) / 100)
}

unsafe fn apply_region(hwnd: HWND, w: i32, h: i32) {
    let rad = if app().expanded { 30 } else { 16 };
    let top = CreateRectRgn(0, 0, w, h - rad / 2);
    let round = CreateRoundRectRgn(0, 0, w + 1, h + 1, rad * 2, rad * 2);
    CombineRgn(top, top, round, RGN_OR);
    SetWindowRgn(hwnd, top, 1);
    DeleteObject(round as HGDIOBJ);
}

unsafe fn apply_size(hwnd: HWND) {
    let (w, h) = cur_size();
    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    SetWindowPos(hwnd, HWND_TOPMOST, (screen_w - w) / 2, 0, w, h, SWP_NOACTIVATE);
    apply_region(hwnd, w, h);
    InvalidateRect(hwnd, null(), 0);
}

// ---------- painting ----------
unsafe fn paint(hdc: HDC, w: i32, h: i32) {
    fill_rect_c(hdc, 0, 0, w, h, BLACK);
    app().actions.clear();
    if !app().expanded {
        paint_collapsed(hdc, w, h);
        return;
    }
    paint_strip(hdc, w);
    match app().page {
        0 => paint_music(hdc, w, h),
        1 => paint_pomo(hdc, w, h),
        2 => paint_water(hdc, w, h),
        3 => paint_shelf(hdc, w, h),
        _ => paint_todo(hdc, w, h),
    }
    // chevrons
    for (is_prev, cx) in [(true, 22), (false, w - 22)] {
        fill_circle(hdc, cx, h / 2, 13, cr(35, 35, 39));
        draw_text(hdc, if is_prev { "‹" } else { "›" }, cx - 13, h / 2 - 13, 26, 26, "Segoe UI", 17, 400, cr(201, 201, 209), DT_CENTER);
        add_action(cx - 14, h / 2 - 14, 28, 28, if is_prev { A_PREV_PAGE } else { A_NEXT_PAGE }, 0);
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

unsafe fn paint_collapsed(hdc: HDC, w: i32, h: i32) {
    // Just the tab: black sliver + one small green dot as the cue.
    fill_circle(hdc, w / 2, h / 2 + 1, 3, GREEN);
}

unsafe fn paint_strip(hdc: HDC, w: i32) {
    let a = app();
    // left title
    match a.page {
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
    // right actions
    if a.page == 2 {
        fill_round(hdc, w - 64, 6, 44, 26, 26, CARD2);
        draw_text(hdc, "\u{ea8f}", w - 64, 6, 44, 26, "Segoe MDL2 Assets", 12, 400, WHITE, DT_CENTER);
        add_action(w - 64, 6, 44, 26, A_WATER_BELL, 0);
    } else if a.page == 3 {
        draw_text(hdc, "\u{ea8f}", w - 44, 0, 26, 38, "Segoe MDL2 Assets", 14, 400, WHITE, DT_CENTER);
        fill_circle(hdc, w - 22, 9, 3, RED);
    }
}

unsafe fn paint_music(hdc: HDC, w: i32, h: i32) {
    let m = media::snapshot();
    if app().lyrics_open {
        paint_lyrics(hdc, w, h, &m);
        return;
    }
    // art
    if m.present {
        fill_round(hdc, 40, 50, 88, 88, 16, cr(45, 60, 160));
    } else {
        fill_round(hdc, 40, 50, 88, 88, 16, CARD2);
    }
    draw_text(hdc, "\u{e8d6}", 40, 50, 88, 88, "Segoe MDL2 Assets", 34, 400, if m.present { WHITE } else { cr(142, 142, 152) }, DT_CENTER);
    // title / artist
    let title = if m.present && !m.title.is_empty() { m.title.clone() } else if m.present { "Playing".into() } else { "Not Playing".into() };
    draw_text(hdc, &title, 146, 62, w - 190, 30, "Segoe UI", 20, 700, WHITE, DT_LEFT);
    if m.present && !m.artist.is_empty() {
        draw_text(hdc, &m.artist.clone(), 146, 94, w - 190, 22, "Segoe UI", 13, 400, DIM, DT_LEFT);
    }
    // seek
    let frac = if m.duration_secs > 0.0 { (m.position_secs / m.duration_secs).clamp(0.0, 1.0) } else { 0.0 };
    draw_text(hdc, &fmt_mmss(m.position_secs), 40, 148, 40, 20, "Segoe UI", 13, 400, DIM, DT_LEFT);
    draw_text(hdc, &format!("-{}", fmt_mmss(m.duration_secs - m.position_secs)), w - 80, 148, 40, 20, "Segoe UI", 13, 400, DIM, DT_RIGHT);
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
    draw_text(hdc, "\u{e767}", w - 68, 176, 30, 26, "Segoe MDL2 Assets", 17, 400, col, DT_CENTER);
    if m.present {
        add_action(w - 72, 172, 38, 32, A_VOL_TOGGLE, 0);
    }
    if app().vol_open {
        let cx = w / 2;
        let (lvl, muted) = (app().vol_level, app().vol_muted);
        draw_text(hdc, if muted { "\u{e74f}" } else { "\u{e767}" }, cx - 116, 208, 26, 24, "Segoe MDL2 Assets", 14, 400, cr(215, 215, 222), DT_CENTER);
        add_action(cx - 118, 206, 30, 28, A_VOL_MUTE, 0);
        fill_round(hdc, cx - 76, 217, 152, 6, 6, cr(43, 43, 49));
        if lvl > 0.0 && !muted {
            fill_round(hdc, cx - 76, 217, (152.0 * lvl) as i32, 6, 6, WHITE);
        }
        add_action(cx - 80, 206, 160, 28, A_VOL_BAR, 0);
        let pct = format!("{}%", (lvl * 100.0).round() as i32);
        draw_text(hdc, &pct, cx + 84, 208, 44, 24, "Segoe UI", 12, 400, DIM, DT_LEFT);
    }
}

unsafe fn paint_lyrics(hdc: HDC, w: i32, h: i32, m: &media::MediaInfo) {
    // left compact player
    if m.present {
        fill_round(hdc, 26, 52, 56, 56, 12, cr(45, 60, 160));
    } else {
        fill_round(hdc, 26, 52, 56, 56, 12, CARD2);
    }
    draw_text(hdc, "\u{e8d6}", 26, 52, 56, 56, "Segoe MDL2 Assets", 22, 400, WHITE, DT_CENTER);
    draw_text(hdc, &m.title.clone(), 94, 58, 190, 24, "Segoe UI", 15, 700, WHITE, DT_LEFT);
    if !m.artist.is_empty() {
        draw_text(hdc, &m.artist.clone(), 94, 82, 190, 20, "Segoe UI", 12, 400, DIM, DT_LEFT);
    }
    let frac = if m.duration_secs > 0.0 { (m.position_secs / m.duration_secs).clamp(0.0, 1.0) } else { 0.0 };
    fill_round(hdc, 26, 128, 250, 5, 5, cr(43, 43, 49));
    if frac > 0.0 {
        fill_round(hdc, 26, 128, (250.0 * frac) as i32, 5, 5, cr(233, 233, 238));
    }
    add_action(26, 120, 250, 20, A_SEEK, 0);
    let cx = 150;
    draw_text(hdc, "\u{e892}", cx - 66, 148, 30, 26, "Segoe MDL2 Assets", 15, 400, cr(215, 215, 222), DT_CENTER);
    draw_text(hdc, if m.playing { "\u{e769}" } else { "\u{e768}" }, cx - 15, 148, 30, 26, "Segoe MDL2 Assets", 16, 400, WHITE, DT_CENTER);
    draw_text(hdc, "\u{e893}", cx + 36, 148, 30, 26, "Segoe MDL2 Assets", 15, 400, cr(215, 215, 222), DT_CENTER);
    add_action(cx - 70, 144, 38, 32, A_PREV_TRACK, 0);
    add_action(cx - 19, 144, 38, 32, A_PLAYPAUSE, 0);
    add_action(cx + 32, 144, 38, 32, A_NEXT_TRACK, 0);
    // right red pane
    let px = 292;
    let pw = w - px - 14;
    // simple vertical gradient
    for i in 0..24 {
        let t = i as f64 / 24.0;
        let col = cr((61.0 - 33.0 * t) as u32, (18.0 - 10.0 * t) as u32, (32.0 - 16.0 * t) as u32);
        fill_rect_c(hdc, px, 46 + (i * 140) / 24, pw, 140 / 24 + 1, col);
    }
    // close
    draw_text(hdc, "✕", px + pw - 30, 48, 24, 22, "Segoe UI", 12, 400, DIM, DT_CENTER);
    add_action(px + pw - 32, 46, 28, 26, A_LYRICS, 0);
    match lyrics::state() {
        lyrics::LyricsState::Loading | lyrics::LyricsState::Idle => {
            draw_text(hdc, "Loading lyrics", px, 100, pw, 26, "Segoe UI", 14, 600, WHITE, DT_CENTER);
        }
        lyrics::LyricsState::NotAvailable => {
            draw_text(hdc, "Lyrics not available", px, 100, pw, 26, "Segoe UI", 14, 600, WHITE, DT_CENTER);
        }
        lyrics::LyricsState::Ready(lines) => {
            let cur = lyrics::current_line(&lines, m.position_secs);
            let mut row_y = 52;
            for slot in 0..5i32 {
                let idx = cur as i32 + slot - 2;
                if idx >= 0 && (idx as usize) < lines.len() {
                    let (t, line) = &lines[idx as usize];
                    let (size, weight, col) = if slot == 2 {
                        (15, 700, WHITE)
                    } else if slot == 1 || slot == 3 {
                        (13, 550, cr(185, 167, 174))
                    } else {
                        (13, 550, cr(139, 127, 134))
                    };
                    draw_text(hdc, line, px + 10, row_y, pw - 20, 24, "Segoe UI", size, weight, col, DT_CENTER);
                    let ly = row_y;
                    add_action(px, ly, pw - 36, 24, A_LYRIC_LINE, (*t * 10.0) as i32);
                }
                row_y += 27;
            }
        }
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
    let sub = format!("Today · {}% · Reminders {}", pct, if a.reminders { "on" } else { "off" });
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
    for (i, item) in a.shelf_items.iter().enumerate() {
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
    for (i, t) in a.store.tasks.iter().enumerate() {
        if y > h - 76 {
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
unsafe fn do_action(code: i32, param: i32, x: i32, _y: i32, hwnd: HWND) {
    let a = app();
    match code {
        A_PREV_PAGE => {
            a.page = (a.page + PAGES - 1) % PAGES;
            a.lyrics_open = false;
            apply_size(hwnd);
        }
        A_NEXT_PAGE => {
            a.page = (a.page + 1) % PAGES;
            a.lyrics_open = false;
            apply_size(hwnd);
        }
        A_DOT => {
            a.page = param as usize;
            a.lyrics_open = false;
            apply_size(hwnd);
        }
        A_PLAYPAUSE => media::play_pause(media::snapshot().playing),
        A_NEXT_TRACK => media::next_track(),
        A_PREV_TRACK => media::prev_track(),
        A_SEEK => {
            let m = media::snapshot();
            if m.duration_secs > 0.0 {
                // seek bar geometry differs in lyrics mode; both start at known x
                let (bx, bw) = if a.lyrics_open { (26, 250) } else { (88, expanded_size().0 - 176) };
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
        A_VOL_TOGGLE => {
            a.vol_open = !a.vol_open;
            if let Some((lvl, muted)) = media::volume_get() {
                a.vol_level = lvl;
                a.vol_muted = muted;
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
        A_VOL_BAR => {
            let cx = expanded_size().0 / 2;
            let frac = ((x - (cx - 76)) as f64 / 152.0).clamp(0.0, 1.0) as f32;
            media::volume_set(frac);
            a.vol_level = frac;
            if a.vol_muted {
                media::volume_toggle_mute();
                a.vol_muted = false;
            }
        }
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
            a.reminders = !a.reminders;
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
            let (dw, dh) = cur_design();
            // Render the design into a 2x buffer (all drawing in design
            // units via anisotropic mapping), then halftone-downscale to
            // the real window size: smooth ClearType text and shapes.
            const SS: i32 = 2;
            let mem = CreateCompatibleDC(hdc);
            let bmp = CreateCompatibleBitmap(hdc, pw * SS, ph * SS);
            let old = SelectObject(mem, bmp as HGDIOBJ);
            // v0.2.1 used MM_ANISOTROPIC window/viewport exts here and on
            // Ram's machine the mapping silently did not apply: everything
            // drew 1:1 into the 2x buffer and StretchBlt halved it, leaving
            // the UI squished into the top half of the card. A world
            // transform (GM_ADVANCED) scales every draw call reliably and
            // is not affected by StretchBlt's source-rect transformation.
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
            paint(mem, dw, dh);
            // Reset to identity BEFORE StretchBlt: a transform left active
            // on the source DC can be applied to the source rectangle too.
            ModifyWorldTransform(mem, std::ptr::null(), MWT_IDENTITY);
            SetStretchBltMode(hdc, HALFTONE);
            StretchBlt(hdc, 0, 0, pw, ph, mem, 0, 0, pw * SS, ph * SS, SRCCOPY);
            SelectObject(mem, old);
            DeleteObject(bmp as HGDIOBJ);
            DeleteDC(mem);
            EndPaint(hwnd, &ps);
            0
        }
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
            }
            0
        }
        WM_MOUSELEAVE_MSG => {
            let a = app();
            a.tracking = false;
            let edit_visible = !a.edit_hwnd.is_null() && IsWindowVisible(a.edit_hwnd) != 0;
            if a.expanded && !edit_visible {
                a.expanded = false;
                a.lyrics_open = false;
                apply_size(hwnd);
            }
            0
        }
        WM_SETCURSOR => {
            SetCursor(LoadCursorW(null_mut(), IDC_ARROW));
            1
        }
        WM_LBUTTONUP => {
            let mut crc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut crc);
            let (dw, dh) = cur_design();
            let x = (lparam & 0xFFFF) as i16 as i32 * dw / crc.right.max(1);
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32 * dh / crc.bottom.max(1);
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
        WM_MOUSEWHEEL => {
            let a = app();
            if a.expanded && a.page == 0 && media::snapshot().present {
                let delta = ((wparam >> 16) & 0xFFFF) as i16 as f32 / 120.0;
                if let Some((lvl, muted)) = media::volume_get() {
                    let next = (lvl + delta * 0.05).clamp(0.0, 1.0);
                    media::volume_set(next);
                    a.vol_level = next;
                    a.vol_muted = muted;
                    if !a.vol_open {
                        a.vol_open = true;
                        apply_size(hwnd);
                    }
                    InvalidateRect(hwnd, null(), 0);
                }
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
            // water day rollover
            let today = today_string();
            if a.store.water_date != today {
                a.store.water_date = today;
                a.store.water_ml = 0;
                store::save(&a.store);
            }
            if a.page == 0 {
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
            tracking: false,
            fonts: Vec::new(),
            actions: Vec::new(),
            pomo_total: 1500,
            pomo_left: 1500,
            pomo_running: false,
            pomo_label: "Focus time".into(),
            reminders: false,
            vol_open: false,
            vol_level: 0.0,
            vol_muted: false,
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

        let (w, h) = collapsed_size();
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
        apply_region(hwnd, w, h);
        ShowWindow(hwnd, SW_SHOW);
        media::spawn_media_thread();

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

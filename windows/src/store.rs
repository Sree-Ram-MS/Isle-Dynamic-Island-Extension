// Tiny plain-text persistence in %LOCALAPPDATA%\Isle\data.txt
// (same idea as the original's plain-text files in %LOCALAPPDATA%\Nusa).
// Format: one key=value per line; todo= and shelf= may repeat.

use std::path::PathBuf;

#[derive(Default, Clone)]
pub struct Task {
    pub title: String,
    pub done: bool,
}

#[derive(Default)]
pub struct Store {
    pub water_date: String, // YYYY-MM-DD of the water counter
    pub water_ml: i32,
    pub tasks: Vec<Task>,
    pub shelf: Vec<String>,
    /// Notch size setting in percent: 0 = Auto (follow Windows scaling),
    /// otherwise 100 / 115 / 130 / 150.
    pub scale: i32,
    /// Chime when a Pomodoro session ends.
    pub sounds: bool,
    /// Hide the notch while a full-screen app (game, video, slides) runs.
    pub hide_fs: bool,
    /// Minimized notch turns into a mini player (art + wave) while music plays.
    pub mini_player: bool,
    /// The mini player shows the current lyric line instead of the title.
    pub mini_lyrics: bool,
    /// Water reminders on/off.
    pub reminders: bool,
    /// Display the notch sits on (index in monitor enumeration order);
    /// -1 = the main display.
    pub monitor: i32,
    /// Idle look: a tiny tab (true) instead of the clock pill (false).
    pub tiny_tab: bool,
    /// Today's drinks by hour: (hour, ml).
    pub water_log: Vec<(i32, i32)>,
    /// Open the notch on click instead of on hover.
    pub open_click: bool,
    /// Swap the page dots for page icons on hover.
    pub page_icons: bool,
    /// Faster open / close animation.
    pub anim_fast: bool,
    /// Lyrics button, Lyrics page and lyric fetching.
    pub show_lyrics: bool,
    /// Pomodoro durations in minutes.
    pub pomo_focus: i32,
    pub pomo_short: i32,
    pub pomo_long: i32,
    /// Water plan.
    pub water_goal: i32, // ml per day
    pub drink_ml: i32,   // ml added per Drink
    pub active_from: i32, // hour, reminders start
    pub active_until: i32, // hour, reminders stop
    /// Clear the Shelf every time Isle starts.
    pub empty_shelf: bool,
}

pub const GOALS: [i32; 4] = [1500, 2000, 2500, 3000];

fn data_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Isle").join("data.txt")
}

pub fn load() -> Store {
    let mut st = Store::default();
    st.scale = 100;
    st.sounds = true;
    st.hide_fs = true;
    st.mini_player = true;
    st.mini_lyrics = true;
    st.monitor = -1;
    st.page_icons = true;
    st.show_lyrics = true;
    st.pomo_focus = 25;
    st.pomo_short = 5;
    st.pomo_long = 15;
    st.water_goal = 1500;
    st.drink_ml = 250;
    st.active_from = 8;
    st.active_until = 17;
    let path = data_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k {
                    "water_date" => st.water_date = v.to_string(),
                    "water_ml" => st.water_ml = v.parse().unwrap_or(0),
                    "scale" => {
                        let s: i32 = v.parse().unwrap_or(100);
                        st.scale = if matches!(s, 0 | 100 | 115 | 130 | 150) { s } else { 100 };
                    }
                    "sounds" => st.sounds = v != "0",
                    "hide_fs" => st.hide_fs = v != "0",
                    "mini_player" => st.mini_player = v != "0",
                    "mini_lyrics" => st.mini_lyrics = v != "0",
                    "reminders" => st.reminders = v != "0",
                    "monitor" => st.monitor = v.parse().unwrap_or(-1),
                    "tiny_tab" => st.tiny_tab = v != "0",
                    "water_log" => {
                        for part in v.split(',') {
                            if let Some((h, m)) = part.split_once(':') {
                                if let (Ok(h), Ok(m)) = (h.trim().parse(), m.trim().parse()) {
                                    st.water_log.push((h, m));
                                }
                            }
                        }
                    }
                    "open_click" => st.open_click = v != "0",
                    "page_icons" => st.page_icons = v != "0",
                    "anim_fast" => st.anim_fast = v != "0",
                    "show_lyrics" => st.show_lyrics = v != "0",
                    "pomo_focus" => st.pomo_focus = v.parse().unwrap_or(25).clamp(5, 90),
                    "pomo_short" => st.pomo_short = v.parse().unwrap_or(5).clamp(1, 30),
                    "pomo_long" => st.pomo_long = v.parse().unwrap_or(15).clamp(5, 60),
                    "water_goal" => {
                        let g: i32 = v.parse().unwrap_or(1500);
                        st.water_goal = if GOALS.contains(&g) { g } else { 1500 };
                    }
                    "drink_ml" => st.drink_ml = v.parse().unwrap_or(250).clamp(50, 3000),
                    "active_from" => st.active_from = v.parse().unwrap_or(8).clamp(0, 23),
                    "active_until" => st.active_until = v.parse().unwrap_or(17).clamp(0, 23),
                    "empty_shelf" => st.empty_shelf = v != "0",
                    "todo" => {
                        let (t, d) = v.rsplit_once('|').unwrap_or((v, "0"));
                        st.tasks.push(Task { title: t.to_string(), done: d == "1" });
                    }
                    "shelf" => {
                        if !v.is_empty() {
                            st.shelf.push(v.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    st
}

pub fn save(st: &Store) {
    let path = data_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut out = String::new();
    out.push_str(&format!("water_date={}\n", st.water_date));
    out.push_str(&format!("water_ml={}\n", st.water_ml));
    out.push_str(&format!("scale={}\n", st.scale));
    out.push_str(&format!("sounds={}\n", if st.sounds { 1 } else { 0 }));
    out.push_str(&format!("hide_fs={}\n", if st.hide_fs { 1 } else { 0 }));
    out.push_str(&format!("mini_player={}\n", if st.mini_player { 1 } else { 0 }));
    out.push_str(&format!("mini_lyrics={}\n", if st.mini_lyrics { 1 } else { 0 }));
    out.push_str(&format!("reminders={}\n", if st.reminders { 1 } else { 0 }));
    let b = |x: bool| if x { 1 } else { 0 };
    out.push_str(&format!("monitor={}\ntiny_tab={}\nopen_click={}\npage_icons={}\nanim_fast={}\nshow_lyrics={}\n",
        st.monitor, b(st.tiny_tab), b(st.open_click), b(st.page_icons), b(st.anim_fast), b(st.show_lyrics)));
    out.push_str(&format!("pomo_focus={}\npomo_short={}\npomo_long={}\n", st.pomo_focus, st.pomo_short, st.pomo_long));
    out.push_str(&format!("water_goal={}\ndrink_ml={}\nactive_from={}\nactive_until={}\nempty_shelf={}\n",
        st.water_goal, st.drink_ml, st.active_from, st.active_until, b(st.empty_shelf)));
    let log: Vec<String> = st.water_log.iter().map(|(h, m)| format!("{}:{}", h, m)).collect();
    out.push_str(&format!("water_log={}\n", log.join(",")));
    for t in &st.tasks {
        out.push_str(&format!("todo={}|{}\n", t.title.replace(['\n', '\r', '|'], " "), if t.done { 1 } else { 0 }));
    }
    for s in &st.shelf {
        out.push_str(&format!("shelf={}\n", s));
    }
    let _ = std::fs::write(&path, out);
}

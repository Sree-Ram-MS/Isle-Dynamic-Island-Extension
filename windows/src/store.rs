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
}

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
    for t in &st.tasks {
        out.push_str(&format!("todo={}|{}\n", t.title.replace(['\n', '\r', '|'], " "), if t.done { 1 } else { 0 }));
    }
    for s in &st.shelf {
        out.push_str(&format!("shelf={}\n", s));
    }
    let _ = std::fs::write(&path, out);
}

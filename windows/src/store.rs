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
}

fn data_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Isle").join("data.txt")
}

pub fn load() -> Store {
    let mut st = Store::default();
    let path = data_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k {
                    "water_date" => st.water_date = v.to_string(),
                    "water_ml" => st.water_ml = v.parse().unwrap_or(0),
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
    for t in &st.tasks {
        out.push_str(&format!("todo={}|{}\n", t.title.replace(['\n', '\r', '|'], " "), if t.done { 1 } else { 0 }));
    }
    for s in &st.shelf {
        out.push_str(&format!("shelf={}\n", s));
    }
    let _ = std::fs::write(&path, out);
}

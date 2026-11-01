// LRCLIB synced lyrics over WinHTTP — same source and query shape as the
// original: GET https://lrclib.net/api/search?track_name=&artist_name=
// with the closest-duration synced result winning. Runs on a short-lived
// thread per track; results land in a shared snapshot the UI reads.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Clone, PartialEq)]
pub enum LyricsState {
    Idle,
    Loading,
    Ready(Vec<(f64, String)>), // (seconds, line)
    NotAvailable,
}

pub static LYRICS: Mutex<LyricsState> = Mutex::new(LyricsState::Idle);
pub static LYRICS_FOR: Mutex<String> = Mutex::new(String::new()); // "title\0artist" key

/// Show lyrics in English letters: prefer an LRCLIB version written in
/// Latin script, and transliterate Tamil / Malayalam / Hindi / ... lines.
pub static ENGLISH: AtomicBool = AtomicBool::new(true);

pub fn state() -> LyricsState {
    let st = LYRICS.lock().map(|l| l.clone()).unwrap_or(LyricsState::Idle);
    match st {
        LyricsState::Ready(lines) if ENGLISH.load(Ordering::Relaxed) => {
            LyricsState::Ready(lines.into_iter().map(|(t, l)| (t, crate::translit::to_latin(&l))).collect())
        }
        other => other,
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

// GET https://host/path -> Ok((http_status, body)) or Err(win32_error).
// Tries the automatic (user/WPAD/PAC) proxy mode first, then the machine
// default, so it works behind a system proxy as well as on a plain line.
#[cfg(windows)]
fn http_get(host: &str, path: &str) -> Result<(u32, String), u32> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Networking::WinHttp::*;
    unsafe {
        let ua = wide("Isle/0.2 (https://github.com/Sree-Ram-MS/Isle-Dynamic-Island-Extension)");
        let mut session = WinHttpOpen(ua.as_ptr(), 4, std::ptr::null(), std::ptr::null(), 0);
        if session.is_null() {
            session = WinHttpOpen(ua.as_ptr(), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, std::ptr::null(), std::ptr::null(), 0);
        }
        if session.is_null() {
            return Err(GetLastError());
        }
        // resolve / connect / send / receive timeouts so a dead network
        // can't leave the fetch thread hanging forever
        WinHttpSetTimeouts(session, 5000, 5000, 8000, 8000);
        let conn = WinHttpConnect(session, wide(host).as_ptr(), 443, 0);
        if conn.is_null() {
            let e = GetLastError();
            WinHttpCloseHandle(session);
            return Err(e);
        }
        let req = WinHttpOpenRequest(
            conn,
            wide("GET").as_ptr(),
            wide(path).as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            WINHTTP_FLAG_SECURE,
        );
        if req.is_null() {
            let e = GetLastError();
            WinHttpCloseHandle(conn);
            WinHttpCloseHandle(session);
            return Err(e);
        }
        let ok = WinHttpSendRequest(req, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) != 0
            && WinHttpReceiveResponse(req, std::ptr::null_mut()) != 0;
        let err = if ok { 0 } else { GetLastError() };
        let mut status: u32 = 0;
        let mut body = Vec::new();
        if ok {
            // 19 = WINHTTP_QUERY_STATUS_CODE, 0x20000000 = ..._FLAG_NUMBER
            let mut len: u32 = 4;
            WinHttpQueryHeaders(
                req,
                19 | 0x2000_0000,
                std::ptr::null(),
                &mut status as *mut u32 as *mut core::ffi::c_void,
                &mut len,
                std::ptr::null_mut(),
            );
            loop {
                let mut avail: u32 = 0;
                if WinHttpQueryDataAvailable(req, &mut avail) == 0 || avail == 0 {
                    break;
                }
                let mut buf = vec![0u8; avail as usize];
                let mut read: u32 = 0;
                if WinHttpReadData(req, buf.as_mut_ptr() as *mut _, avail, &mut read) == 0 {
                    break;
                }
                buf.truncate(read as usize);
                body.extend_from_slice(&buf);
            }
        }
        WinHttpCloseHandle(req);
        WinHttpCloseHandle(conn);
        WinHttpCloseHandle(session);
        if ok {
            Ok((status, String::from_utf8_lossy(&body).into_owned()))
        } else {
            Err(err)
        }
    }
}

// Non-Windows stub so the parsing code can be unit-tested anywhere.
#[cfg(not(windows))]
fn http_get(_host: &str, _path: &str) -> Result<(u32, String), u32> {
    Err(0)
}

fn json_number_after(hay: &str, key: &str, from: usize) -> Option<f64> {
    let kpos = hay[from..].find(key)? + from + key.len();
    let rest = &hay[kpos..];
    let start = rest.find(|c: char| c.is_ascii_digit() || c == '.')?;
    let num: String = rest[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    num.parse().ok()
}

fn parse_lrc(text: &str) -> Vec<(f64, String)> {
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.starts_with('[') {
            if let Some(end) = line.find(']') {
                let stamp = &line[1..end];
                if let Some((mm, ss)) = stamp.split_once(':') {
                    if let (Ok(m), Ok(s)) = (mm.parse::<f64>(), ss.parse::<f64>()) {
                        lines.push((m * 60.0 + s, line[end + 1..].trim().to_string()));
                        continue;
                    }
                }
            }
        }
        // skip metadata tags like [ar:Artist] / [length:03:20]
        if line.starts_with('[') {
            continue;
        }
        if !line.trim().is_empty() {
            lines.push((0.0, line.trim().to_string()));
        }
    }
    lines
}

fn strip_brackets(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

// Strip YouTube-style decoration from a title/artist: everything from
// "feat."/"ft." onwards, then trailing words like "Official Video". Only
// *trailing* words go, so real titles ("Video Games", "Love Song") survive.
fn clean_words(s: &str) -> String {
    const TRAIL: &[&str] = &[
        "lyrical", "lyric", "lyrics", "official", "video", "audio", "music", "hd", "4k",
        "full", "mv", "vevo", "visualizer",
    ];
    let mut words: Vec<&str> = s.split_whitespace().collect();
    if let Some(i) = words.iter().position(|w| {
        matches!(w.to_lowercase().trim_end_matches('.'), "feat" | "ft" | "featuring")
    }) {
        if i > 0 {
            words.truncate(i);
        }
    }
    while words.len() > 1 && TRAIL.contains(&words[words.len() - 1].to_lowercase().as_str()) {
        words.pop();
    }
    words.join(" ").trim().to_string()
}

fn title_candidates(title: &str, artist: &str) -> Vec<(String, String)> {
    let first = title.split('|').next().unwrap_or(title);
    let t_full = clean_words(&strip_brackets(first));
    let a_clean = clean_words(&strip_brackets(artist.trim().trim_end_matches(" - Topic")));
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |t: String, a: String, out: &mut Vec<(String, String)>| {
        if !t.is_empty() && !out.iter().any(|(x, y)| *x == t && *y == a) {
            out.push((t, a));
        }
    };
    if let Some((before, after)) = t_full.split_once(" - ") {
        push(clean_words(after), a_clean.clone(), &mut out);
        push(clean_words(before), a_clean.clone(), &mut out);
    }
    push(t_full.clone(), a_clean.clone(), &mut out);
    if let Some((_, after)) = t_full.split_once(" - ") {
        push(clean_words(after), String::new(), &mut out);
    }
    push(t_full, String::new(), &mut out);
    out
}

// Parse a JSON string whose opening quote is at byte index `q`.
// Returns (value, index just past the closing quote).
fn parse_json_string_at(hay: &str, q: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut chars = hay[q + 1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, q + 1 + i + 1)),
            '\\' => match chars.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, 'r')) => out.push('\r'),
                Some((_, '/')) => out.push('/'),
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                Some((_, 'u')) => {
                    let hex: String = chars.by_ref().take(4).map(|(_, c)| c).collect();
                    if let Ok(n) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(n) {
                            out.push(ch);
                        }
                    }
                }
                _ => {}
            },
            _ => out.push(c),
        }
    }
    None
}

// Find the best string value of `key` (e.g. "syncedLyrics") in an
// /api/search response body: skips null / empty values and picks the hit
// whose duration is closest to the playing track. Returns (value, duration).
fn best_field(body: &str, key: &str, duration_secs: f64) -> Option<(String, f64)> {
    let mut best: Option<(f64, String, f64)> = None; // (diff, value, duration)
    let mut pos = 0usize;
    while let Some(key_rel) = body[pos..].find(key) {
        let key_pos = pos + key_rel;
        let after_key = key_pos + key.len();
        let colon = match body[after_key..].find(':') {
            Some(c) => after_key + c,
            None => break,
        };
        let mut vi = colon + 1;
        while vi < body.len() && (body.as_bytes()[vi] as char).is_whitespace() {
            vi += 1;
        }
        // not a string (e.g. null): skip this result
        if vi >= body.len() || body.as_bytes()[vi] != b'"' {
            pos = after_key;
            continue;
        }
        let (value, next) = match parse_json_string_at(body, vi) {
            Some(v) => v,
            None => break,
        };
        let dur = {
            let before = &body[..key_pos];
            match before.rfind("\"duration\"") {
                Some(dp) => json_number_after(before, "\"duration\"", dp).unwrap_or(0.0),
                None => 0.0,
            }
        };
        if !value.trim().is_empty() {
            let mut diff = if duration_secs > 0.0 { (dur - duration_secs).abs() } else { 9999.0 };
            // English letters wanted: a Latin-script version wins over the
            // original script whenever LRCLIB has one
            if ENGLISH.load(Ordering::Relaxed) && crate::translit::has_indic(&value) {
                diff += 100_000.0;
            }
            if best.as_ref().map(|(d, _, _)| diff < *d).unwrap_or(true) {
                best = Some((diff, value, dur));
            }
        }
        pos = next;
    }
    best.map(|(_, v, d)| (v, d))
}

fn find_synced(body: &str, duration_secs: f64) -> Option<Vec<(f64, String)>> {
    best_field(body, "\"syncedLyrics\"", duration_secs)
        .map(|(s, _)| parse_lrc(&s))
        .filter(|v| !v.is_empty())
}

// Unsynced lyrics: no timestamps, so spread the lines evenly across the
// track so the pane still follows the song roughly.
fn find_plain(body: &str, duration_secs: f64) -> Option<Vec<(f64, String)>> {
    let (text, dur) = best_field(body, "\"plainLyrics\"", duration_secs)?;
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let total = if duration_secs > 0.0 { duration_secs } else { dur.max(lines.len() as f64 * 3.0) };
    let step = total / lines.len() as f64;
    Some(lines.into_iter().enumerate().map(|(i, l)| (i as f64 * step, l)).collect())
}

// When every request failed at the network level (offline, DNS, TLS), wait
// this long before the once-a-second UI timer is allowed to try again.
const RETRY_AFTER_FAIL_MS: u64 = 15_000;
static LAST_NET_FAIL_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// An empty artist_name= filters LRCLIB down to nothing, so leave the
// parameter out entirely when there is no artist.
fn search_path(track: &str, artist: &str) -> String {
    if artist.is_empty() {
        format!("/api/search?track_name={}", url_encode(track))
    } else {
        format!(
            "/api/search?track_name={}&artist_name={}",
            url_encode(track),
            url_encode(artist)
        )
    }
}

/// Short human-readable reason for the current lyrics state, shown under
/// "Lyrics not available" so a failure can be diagnosed from the screen.
pub static INFO: Mutex<String> = Mutex::new(String::new());

pub fn info() -> String {
    INFO.lock().map(|i| i.clone()).unwrap_or_default()
}

fn set_info(s: String) {
    if let Ok(mut i) = INFO.lock() {
        *i = s;
    }
}

pub fn refetch(title: &str, artist: &str, duration_secs: f64) {
    if let Ok(mut k) = LYRICS_FOR.lock() {
        k.clear();
    }
    LAST_NET_FAIL_MS.store(0, Ordering::Relaxed);
    ensure_fetched(title, artist, duration_secs);
}

pub fn ensure_fetched(title: &str, artist: &str, duration_secs: f64) {
    let key = format!("{}\u{0}{}", title, artist);
    {
        let cur = LYRICS_FOR.lock().map(|k| k.clone()).unwrap_or_default();
        if cur == key {
            return; // already loading/loaded for this track
        }
        let failed_at = LAST_NET_FAIL_MS.load(Ordering::Relaxed);
        if failed_at != 0 && now_ms().saturating_sub(failed_at) < RETRY_AFTER_FAIL_MS {
            return; // recent network failure: don't hammer, retry shortly
        }
        if let Ok(mut k) = LYRICS_FOR.lock() {
            *k = key.clone();
        }
        if let Ok(mut l) = LYRICS.lock() {
            *l = LyricsState::Loading;
        }
        set_info(String::new());
    }
    let candidates = title_candidates(title, artist);
    std::thread::spawn(move || {
        // structured searches first, then one free-text search as a net
        let mut paths: Vec<String> = candidates.iter().map(|(t, a)| search_path(t, a)).collect();
        let (t0, a0) = candidates.first().cloned().unwrap_or_default();
        let q = format!("{} {}", t0, a0).trim().to_string();
        if !q.is_empty() {
            paths.push(format!("/api/search?q={}", url_encode(&q)));
        }

        let mut synced: Option<Vec<(f64, String)>> = None;
        let mut plain: Option<Vec<(f64, String)>> = None;
        let mut responses = 0;
        let mut last_status = 0u32;
        let mut last_err = 0u32;
        for path in &paths {
            match http_get("lrclib.net", path) {
                Ok((status, body)) => {
                    responses += 1;
                    last_status = status;
                    if status == 200 {
                        if let Some(lines) = find_synced(&body, duration_secs) {
                            synced = Some(lines);
                            break;
                        }
                        if plain.is_none() {
                            plain = find_plain(&body, duration_secs);
                        }
                    }
                }
                Err(e) => last_err = e,
            }
        }

        // the track may have changed while we were fetching: don't let a
        // stale answer overwrite the newer track's state
        let still_current = LYRICS_FOR.lock().map(|k| *k == key).unwrap_or(false);
        if !still_current {
            return;
        }
        let (state, note) = if let Some(lines) = synced {
            (LyricsState::Ready(lines), String::new())
        } else if let Some(lines) = plain {
            (LyricsState::Ready(lines), "unsynced lyrics".to_string())
        } else if responses == 0 {
            // network failure, not "no lyrics exist": allow a retry soon
            LAST_NET_FAIL_MS.store(now_ms(), Ordering::Relaxed);
            if let Ok(mut k) = LYRICS_FOR.lock() {
                k.clear();
            }
            (
                LyricsState::NotAvailable,
                format!("can't reach lrclib.net (error {}), retrying", last_err),
            )
        } else {
            (
                LyricsState::NotAvailable,
                format!("no match on lrclib.net (HTTP {}) for \"{}\"", last_status, q),
            )
        };
        set_info(note);
        if let Ok(mut l) = LYRICS.lock() {
            *l = state;
        }
    });
}

pub fn current_line(lines: &[(f64, String)], pos_secs: f64) -> usize {
    let mut idx = 0;
    for (i, (t, _)) in lines.iter().enumerate() {
        if *t <= pos_secs + 0.2 {
            idx = i;
        } else {
            break;
        }
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape of a real LRCLIB /api/search response: several hits, one with
    // "syncedLyrics": null, escapes and unicode inside the lyric text.
    const BODY: &str = r#"[
 {"id":1,"name":"Song","trackName":"Song","artistName":"A","albumName":"X","duration":200.0,"instrumental":false,
  "plainLyrics":"wrong one\nsecond","syncedLyrics":null},
 {"id":2,"name":"Song","trackName":"Song","artistName":"A","albumName":"Y","duration":231.4,"instrumental":false,
  "plainLyrics":"hello \"world\"\nline two\n\nline three",
  "syncedLyrics":"[ar:A]\n[00:12.50] hello \"world\"\n[00:20.00] caf\u00e9 line\n[01:02.25] last"}
]"#;

    #[test]
    fn synced_picks_closest_duration_and_skips_null() {
        let l = find_synced(BODY, 230.0).expect("synced lyrics");
        assert_eq!(l.len(), 3, "metadata tag line must be skipped: {:?}", l);
        assert_eq!(l[0], (12.5, "hello \"world\"".to_string()));
        assert_eq!(l[1].1, "café line");
        assert!((l[2].0 - 62.25).abs() < 1e-9);
    }

    #[test]
    fn plain_fallback_spreads_lines_over_track() {
        let body = r#"[{"duration":100.0,"plainLyrics":"a\nb\n\nc\nd","syncedLyrics":null}]"#;
        assert!(find_synced(body, 100.0).is_none());
        let l = find_plain(body, 100.0).expect("plain lyrics");
        assert_eq!(l.len(), 4);
        assert_eq!(l[0].0, 0.0);
        assert!((l[3].0 - 75.0).abs() < 1e-9);
        assert_eq!(current_line(&l, 50.0), 2);
    }

    #[test]
    fn empty_and_garbage_bodies() {
        assert!(find_synced("[]", 200.0).is_none());
        assert!(find_synced("", 200.0).is_none());
        assert!(find_plain("<html>502</html>", 200.0).is_none());
    }

    #[test]
    fn candidates_for_youtube_style_titles() {
        let c = title_candidates("Blinding Lights (Official Video) | The Weeknd", "The Weeknd - Topic");
        assert_eq!(c[0], ("Blinding Lights".to_string(), "The Weeknd".to_string()));
        assert!(c.iter().any(|(_, a)| a.is_empty()), "needs an artist-less fallback");
        let c = title_candidates("Artist X - Song Y (Lyrical Video)", "SomeChannel");
        assert_eq!(c[0].0, "Song Y");
        let c = title_candidates("Video Games", "Lana Del Rey");
        assert_eq!(c[0].0, "Video Games");
        let c = title_candidates("Song Name Official Video feat. Someone", "X VEVO");
        assert_eq!(c[0], ("Song Name".to_string(), "X".to_string()));
    }

    #[test]
    fn search_path_omits_empty_artist() {
        assert_eq!(search_path("a b", ""), "/api/search?track_name=a%20b");
        assert_eq!(search_path("a", "b"), "/api/search?track_name=a&artist_name=b");
    }
}

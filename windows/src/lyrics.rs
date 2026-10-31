// LRCLIB synced lyrics over WinHTTP — same source and query shape as the
// original: GET https://lrclib.net/api/search?track_name=&artist_name=
// with the closest-duration synced result winning. Runs on a short-lived
// thread per track; results land in a shared snapshot the UI reads.

use std::sync::atomic::{AtomicU64, Ordering};
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

pub fn state() -> LyricsState {
    LYRICS.lock().map(|l| l.clone()).unwrap_or(LyricsState::Idle)
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

fn wide(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn http_get(host: &str, path: &str) -> Option<String> {
    use windows_sys::Win32::Networking::WinHttp::*;
    unsafe {
        let session = WinHttpOpen(
            wide("Isle/0.2.0 (+https://lrclib.net)").as_ptr(),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        );
        if session.is_null() {
            return None;
        }
        // resolve / connect / send / receive timeouts so a dead network
        // can't leave the fetch thread hanging forever
        WinHttpSetTimeouts(session, 5000, 5000, 8000, 8000);
        let conn = WinHttpConnect(session, wide(host).as_ptr(), 443, 0);
        if conn.is_null() {
            WinHttpCloseHandle(session);
            return None;
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
            WinHttpCloseHandle(conn);
            WinHttpCloseHandle(session);
            return None;
        }
        let ok = WinHttpSendRequest(req, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) != 0
            && WinHttpReceiveResponse(req, std::ptr::null_mut()) != 0;
        let mut body = Vec::new();
        if ok {
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
            String::from_utf8(body).ok()
        } else {
            None
        }
    }
}

// Extract a JSON string field value (with escapes resolved) following `key`.
fn json_string_after(hay: &str, key: &str, from: usize) -> Option<(String, usize)> {
    let kpos = hay[from..].find(key)? + from + key.len();
    let start = hay[kpos..].find('"')? + kpos + 1;
    let mut out = String::new();
    let mut chars = hay[start..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, start + i + 1)),
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

fn clean_words(s: &str) -> String {
    const STOP: &[&str] = &[
        "lyrical", "lyric", "lyrics", "official", "video", "audio", "song", "songs",
        "music", "hd", "4k", "full", "mv", "feat.", "ft.", "feat", "ft", "vevo",
    ];
    s.split_whitespace()
        .filter(|w| !STOP.contains(&w.to_lowercase().as_str()))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn title_candidates(title: &str, artist: &str) -> Vec<(String, String)> {
    let first = title.split('|').next().unwrap_or(title);
    let t_full = clean_words(&strip_brackets(first));
    let a_clean = clean_words(&strip_brackets(artist));
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

// Find the best syncedLyrics in an /api/search response body.
// Handles "syncedLyrics": null correctly (skips those results) and scores
// each hit by how close its duration is to the playing track.
fn find_synced(body: &str, duration_secs: f64) -> Option<Vec<(f64, String)>> {
    let mut best: Option<(f64, String)> = None;
    let mut pos = 0usize;
    loop {
        let key_rel = match body[pos..].find("\"syncedLyrics\"") {
            Some(v) => v,
            None => break,
        };
        let key_pos = pos + key_rel;
        let after_key = key_pos + "\"syncedLyrics\"".len();
        // value starts after ':' and whitespace; if it is not a string
        // (e.g. null), skip this result.
        let colon = match body[after_key..].find(':') {
            Some(c) => after_key + c,
            None => break,
        };
        let mut vi = colon + 1;
        while vi < body.len() && (body.as_bytes()[vi] as char).is_whitespace() {
            vi += 1;
        }
        if vi >= body.len() || body.as_bytes()[vi] != b'"' {
            pos = after_key;
            continue;
        }
        let (synced, next) = match parse_json_string_at(body, vi) {
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
        if !synced.trim().is_empty() {
            let diff = if duration_secs > 0.0 { (dur - duration_secs).abs() } else { 9999.0 };
            if best.as_ref().map(|(d, _)| diff < *d).unwrap_or(true) {
                best = Some((diff, synced));
            }
        }
        pos = next;
    }
    best.map(|(_, s)| parse_lrc(&s)).filter(|v| !v.is_empty())
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

        let mut found: Option<Vec<(f64, String)>> = None;
        let mut got_response = false;
        for path in &paths {
            if let Some(body) = http_get("lrclib.net", path) {
                got_response = true;
                if let Some(lines) = find_synced(&body, duration_secs) {
                    found = Some(lines);
                    break;
                }
            }
        }

        // the track may have changed while we were fetching: don't let a
        // stale answer overwrite the newer track's state
        let still_current = LYRICS_FOR.lock().map(|k| *k == key).unwrap_or(false);
        if !still_current {
            return;
        }
        if found.is_none() && !got_response {
            // network failure, not "no lyrics exist": allow a retry soon
            LAST_NET_FAIL_MS.store(now_ms(), Ordering::Relaxed);
            if let Ok(mut k) = LYRICS_FOR.lock() {
                k.clear();
            }
        }
        if let Ok(mut l) = LYRICS.lock() {
            *l = match found {
                Some(lines) => LyricsState::Ready(lines),
                None => LyricsState::NotAvailable,
            };
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

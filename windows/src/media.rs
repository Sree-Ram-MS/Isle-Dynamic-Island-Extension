// Real media via WinRT GSMTC (Global System Media Transport Controls) —
// the same Windows API the original Nusa uses. No PowerShell bridges.
// A single background thread polls the current session every 2 s and writes
// into a shared snapshot; the UI thread only reads the snapshot and fires
// commands (fire-and-forget async calls, never blocking the UI).

use std::sync::Mutex;

#[derive(Clone, Default, PartialEq)]
pub struct MediaInfo {
    pub present: bool,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub position_secs: f64,
    pub duration_secs: f64,
    /// When the player last updated `position_secs` (timeline LastUpdatedTime,
    /// 100 ns FILETIME ticks, UTC). 0 = unknown. Lets the UI run the position
    /// forward between the 2 s polls so lyrics stay in sync.
    pub updated_ft: i64,
}

pub static MEDIA: Mutex<MediaInfo> = Mutex::new(MediaInfo {
    present: false,
    playing: false,
    title: String::new(),
    artist: String::new(),
    album: String::new(),
    position_secs: 0.0,
    duration_secs: 0.0,
    updated_ft: 0,
});

pub fn snapshot() -> MediaInfo {
    MEDIA.lock().map(|m| m.clone()).unwrap_or_default()
}

/// Playback position right now: the last polled position plus the time
/// since the player reported it (while playing), capped at the duration.
pub fn live_position(m: &MediaInfo) -> f64 {
    if !m.playing || m.updated_ft <= 0 {
        return m.position_secs;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
        + 11_644_473_600.0; // 1601 -> 1970
    let elapsed = now - m.updated_ft as f64 / 10_000_000.0;
    if !(0.0..=6.0 * 3600.0).contains(&elapsed) {
        return m.position_secs;
    }
    let p = m.position_secs + elapsed;
    if m.duration_secs > 0.0 {
        p.min(m.duration_secs)
    } else {
        p
    }
}

// ---------- album art ----------
// The media session's thumbnail, decoded on the media thread into a square
// BGRA pixel buffer (corners pre-rounded onto the notch background). The UI
// thread turns it into a GDI bitmap when `gen` changes. Empty = no art.
pub struct Thumb {
    pub gen: u64,
    pub size: i32,
    pub bgra: Vec<u8>,
}

pub static THUMB: Mutex<Thumb> = Mutex::new(Thumb { gen: 0, size: 0, bgra: Vec::new() });

const THUMB_PX: u32 = 176; // 2x the 88-unit art slot, so it stays sharp

fn set_thumb(size: i32, bgra: Vec<u8>) {
    if let Ok(mut t) = THUMB.lock() {
        t.gen += 1;
        t.size = size;
        t.bgra = bgra;
    }
}

fn thumb_ready() -> bool {
    THUMB.lock().map(|t| !t.bgra.is_empty()).unwrap_or(false)
}

// Decode the current session's thumbnail with the WinRT BitmapDecoder
// (handles JPEG/PNG/etc.), cover-scale to a square, centre-crop, and round
// the corners onto the notch's black background.
fn fetch_thumb() -> Option<(i32, Vec<u8>)> {
    use windows::core::Interface;
    use windows::Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat,
        BitmapTransform, ColorManagementMode, ExifOrientationMode,
    };
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
    use windows::Storage::Streams::IRandomAccessStream;

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok()?.get().ok()?;
    let session = manager.GetCurrentSession().ok()?;
    let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let thumb_ref = props.Thumbnail().ok()?;
    let stream = thumb_ref.OpenReadAsync().ok()?.get().ok()?;
    let stream: IRandomAccessStream = stream.cast().ok()?;
    let decoder = BitmapDecoder::CreateAsync(&stream).ok()?.get().ok()?;
    let (w, h) = (decoder.PixelWidth().ok()?, decoder.PixelHeight().ok()?);
    if w == 0 || h == 0 {
        return None;
    }
    let side = THUMB_PX;
    // scale so the shorter edge equals `side` (cover), crop the rest
    let (sw, sh) = if w >= h {
        (((w as u64 * side as u64 + h as u64 / 2) / h as u64) as u32, side)
    } else {
        (side, ((h as u64 * side as u64 + w as u64 / 2) / w as u64) as u32)
    };
    let transform = BitmapTransform::new().ok()?;
    transform.SetScaledWidth(sw).ok()?;
    transform.SetScaledHeight(sh).ok()?;
    transform.SetInterpolationMode(BitmapInterpolationMode::Fant).ok()?;
    let provider = decoder
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Premultiplied,
            &transform,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )
        .ok()?
        .get()
        .ok()?;
    let data = provider.DetachPixelData().ok()?;
    let px: &[u8] = &data;
    if px.len() < (sw as usize) * (sh as usize) * 4 {
        return None;
    }
    let (ox, oy) = ((sw - side) / 2, (sh - side) / 2);
    let n = side as usize;
    let mut out = Vec::with_capacity(n * n * 4);
    for row in 0..n {
        let start = ((oy as usize + row) * sw as usize + ox as usize) * 4;
        out.extend_from_slice(&px[start..start + n * 4]);
    }
    // rounded corners: blend pixels outside a rounded square into the
    // notch background colour (6, 6, 7), anti-aliased
    let half = side as f32 / 2.0;
    let radius = side as f32 * 0.14;
    for y in 0..n {
        for x in 0..n {
            let qx = ((x as f32 + 0.5) - half).abs() - (half - radius);
            let qy = ((y as f32 + 0.5) - half).abs() - (half - radius);
            let dist = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
            let cov = (0.5 - dist).clamp(0.0, 1.0);
            let i = (y * n + x) * 4;
            let bg = [7.0f32, 6.0, 6.0]; // B, G, R
            for c in 0..3 {
                out[i + c] = (out[i + c] as f32 * cov + bg[c] * (1.0 - cov)).round() as u8;
            }
            out[i + 3] = 255;
        }
    }
    Some((side as i32, out))
}

fn fetch_once() -> Option<MediaInfo> {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok()?.get().ok()?;
    let session = manager.GetCurrentSession().ok()?;
    let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let timeline = session.GetTimelineProperties().ok()?;
    let playback = session.GetPlaybackInfo().ok()?;
    let status = playback.PlaybackStatus().ok()?;
    let pos = timeline.Position().ok()?.Duration as f64 / 10_000_000.0;
    let end = timeline.EndTime().ok()?.Duration as f64 / 10_000_000.0;
    let start = timeline.StartTime().ok()?.Duration as f64 / 10_000_000.0;
    let updated = timeline.LastUpdatedTime().map(|d| d.UniversalTime).unwrap_or(0);
    Some(MediaInfo {
        present: true,
        playing: status == Status::Playing,
        title: props.Title().map(|s| s.to_string()).unwrap_or_default(),
        artist: props.Artist().map(|s| s.to_string()).unwrap_or_default(),
        album: props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default(),
        position_secs: pos,
        duration_secs: (end - start).max(0.0),
        updated_ft: updated,
    })
}

pub fn spawn_media_thread() {
    std::thread::spawn(|| unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut last_key = String::new();
        let mut thumb_tries = 0;
        loop {
            let info = fetch_once().unwrap_or_default();
            let key = if info.present { format!("{}\u{0}{}", info.title, info.artist) } else { String::new() };
            let present = info.present;
            if let Ok(mut m) = MEDIA.lock() {
                *m = info;
            }
            // album art: refetch when the track changes; some players publish
            // the thumbnail a moment after the title, so retry a few polls
            if key != last_key {
                last_key = key;
                thumb_tries = 0;
                set_thumb(0, Vec::new());
            }
            if present && !thumb_ready() && thumb_tries < 8 {
                thumb_tries += 1;
                if let Some((size, bgra)) = fetch_thumb() {
                    set_thumb(size, bgra);
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
}

enum Cmd {
    PlayPause(bool),
    Next,
    Prev,
    Seek(f64),
}

fn run_command(cmd: Cmd) {
    // Fire on a short-lived thread so an async COM call can never stall the UI.
    std::thread::spawn(move || unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if let Ok(op) = windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            if let Ok(manager) = op.get() {
                if let Ok(s) = manager.GetCurrentSession() {
                    match cmd {
                        Cmd::PlayPause(true) => {
                            let _ = s.TryPauseAsync();
                        }
                        Cmd::PlayPause(false) => {
                            let _ = s.TryPlayAsync();
                        }
                        Cmd::Next => {
                            let _ = s.TrySkipNextAsync();
                        }
                        Cmd::Prev => {
                            let _ = s.TrySkipPreviousAsync();
                        }
                        Cmd::Seek(secs) => {
                            let _ = s.TryChangePlaybackPositionAsync((secs * 10_000_000.0) as i64);
                        }
                    }
                }
            }
        }
    });
}

// ---------- system volume via CoreAudio (IAudioEndpointVolume) ----------
// Called from the UI thread only; main() initialises that thread's COM
// apartment once via com_init_ui().

pub fn com_init_ui() {
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

fn endpoint_volume() -> Option<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
    }
}

pub fn volume_get() -> Option<(f32, bool)> {
    let v = endpoint_volume()?;
    unsafe {
        let level = v.GetMasterVolumeLevelScalar().ok()?;
        let muted = v.GetMute().ok()?.as_bool();
        Some((level, muted))
    }
}

pub fn volume_set(level: f32) {
    if let Some(v) = endpoint_volume() {
        unsafe {
            let _ = v.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null());
        }
    }
}

pub fn volume_toggle_mute() {
    if let Some(v) = endpoint_volume() {
        unsafe {
            if let Ok(m) = v.GetMute() {
                let _ = v.SetMute(!m.as_bool(), std::ptr::null());
            }
        }
    }
}

pub fn play_pause(playing_now: bool) {
    run_command(Cmd::PlayPause(playing_now));
}
pub fn next_track() {
    run_command(Cmd::Next);
}
pub fn prev_track() {
    run_command(Cmd::Prev);
}
pub fn seek(secs: f64) {
    run_command(Cmd::Seek(secs));
}

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
}

pub static MEDIA: Mutex<MediaInfo> = Mutex::new(MediaInfo {
    present: false,
    playing: false,
    title: String::new(),
    artist: String::new(),
    album: String::new(),
    position_secs: 0.0,
    duration_secs: 0.0,
});

pub fn snapshot() -> MediaInfo {
    MEDIA.lock().map(|m| m.clone()).unwrap_or_default()
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
    Some(MediaInfo {
        present: true,
        playing: status == Status::Playing,
        title: props.Title().map(|s| s.to_string()).unwrap_or_default(),
        artist: props.Artist().map(|s| s.to_string()).unwrap_or_default(),
        album: props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default(),
        position_secs: pos,
        duration_secs: (end - start).max(0.0),
    })
}

pub fn spawn_media_thread() {
    std::thread::spawn(|| unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        loop {
            let info = fetch_once().unwrap_or_default();
            if let Ok(mut m) = MEDIA.lock() {
                *m = info;
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

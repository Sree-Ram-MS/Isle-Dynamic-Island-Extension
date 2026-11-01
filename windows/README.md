# Isle for Windows — native Rust

A native Windows clone of the Nusa living notch, written in pure Rust on
Win32/WinRT — no Electron, no WebView, no Chromium. Built for the lowest
possible RAM and a tiny single-file `.exe` (~380 KB).

Current version: **0.5.0**

**Lowest possible RAM is a hard requirement:**
- One process, one UI thread; media polled on one background thread
- Event-driven Win32 message loop; timers only where a clock needs one
- Media: WinRT GSMTC called directly from Rust — no PowerShell bridges
- Release profile: opt-level=z, LTO, codegen-units=1, panic=abort, strip

## What it does

- Idle: a tiny tab at the top edge of the screen; hover to open the notch.
- **Mini player** — while music plays, the minimized notch becomes a pill
  with the album art, the current synced lyric line (or the song name) and a
  moving sound wave. Both can be turned off in Settings.
- Every size change (open, close, mini player appearing) animates smoothly:
  the shape springs open and the page fades in, no stretching.
- **Music** — real playback via the Windows media session (GSMTC): title,
  artist, seek bar, play/pause, next/previous, seek, a master volume
  slider (Core Audio), and a **Lyrics** page that shows only the synced
  lyrics from LRCLIB across the whole notch (click a line to seek).
- **Pomodoro** — 25 min focus / 5 min break countdown that switches
  automatically.
- **Water** — +250 ml towards a 1.5 L daily goal, persisted per day.
- **Shelf** — drag files in, click to open, remove with the × button.
- **Todo** — add, check off, edit and delete tasks.
- **Settings** (gear, top right) — its own window, laid out like
  `design/settings-design.html`: a sidebar (General, Display, Appearance,
  Music and lyrics, To-do, Pomodoro, Water, Shelf, About) and grouped cards.
  - General: start with Windows, notch size (Auto / 100–150 %, live),
    minimal notch (tiny tab, or a pill with the time), reminder sounds,
    hide in full screen.
  - Display: pick the screen the notch sits on.
  - Appearance: open on hover or click, page icons, animation speed.
  - Music and lyrics: show lyrics, mini player, lyrics in the mini player.
  - Pomodoro: focus / short / long break lengths (every 4th break is long).
  - Water: reminders spread over active hours (the notch pops open on the
    Water page), daily goal, amount per drink, reset today.
  - Shelf: start with an empty Shelf, clear. To-do: clear completed.

Data is stored as plain text in `%LOCALAPPDATA%\Isle\data.txt`.
Right-click the notch or press Esc to quit.

## Build

Needs Rust (with the `x86_64-pc-windows-gnu` target) and mingw-w64:

```
cargo build --release --target x86_64-pc-windows-gnu
```

Output: `target/x86_64-pc-windows-gnu/release/isle.exe` — a portable
single file, just run it. (Unsigned, so Windows SmartScreen may warn on
first run: More info → Run anyway.)

## Layout

- `src/main.rs` — window, rendering (GDI), pages, hit-testing
- `src/media.rs` — WinRT media session + Core Audio volume
- `src/lyrics.rs` — LRCLIB synced lyrics over WinHTTP
- `src/store.rs` — plain-text persistence
- `design/notch-design.html`, `design/settings-design.html` — the approved HTML design prototypes
  (design reference only; the shipped app is native)

## v0.5.0: new notch design

The notch now follows the "Nusa Notch" design (`design/notch-design.html`):

- Minimized: a floating black pill with a battery ring (green dot on PCs without a battery) and the time. While music plays it widens with a red sound wave; while the Pomodoro runs it shows an orange timer ring; when closed on the Lyrics page (or with "Lyrics in the minimized notch" on) it shows the live lyric line. Plugging in the charger pops a "Charging" card.
- Opened: a 560 x 232 rounded card with the page name in its colour, the clock and date in the header, round chevrons and page dots with a gear (Settings) at the bottom. Hovering the dots shows page icons.
- Six pages: Music, Lyrics, Focus timer, Water (with today's drinks by hour), Shelf (files next to a drop zone) and To-do (round check boxes, done count, Add task).

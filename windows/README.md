# Isle for Windows — native Rust

A native Windows clone of the Nusa living notch, written in pure Rust on
Win32/WinRT — no Electron, no WebView, no Chromium. Built for the lowest
possible RAM and a tiny single-file `.exe` (~380 KB).

Current version: **0.2.2**

**Lowest possible RAM is a hard requirement:**
- One process, one UI thread; media polled on one background thread
- Event-driven Win32 message loop; timers only where a clock needs one
- Media: WinRT GSMTC called directly from Rust — no PowerShell bridges
- Release profile: opt-level=z, LTO, codegen-units=1, panic=abort, strip

## What it does

- Idle: a tiny tab at the top edge of the screen; hover to open the notch.
- **Music** — real playback via the Windows media session (GSMTC): title,
  artist, seek bar, play/pause, next/previous, seek, synced lyrics from
  LRCLIB in a split pane (click a line to seek), and a master volume
  slider (Core Audio).
- **Pomodoro** — 25 min focus / 5 min break countdown that switches
  automatically.
- **Water** — +250 ml towards a 1.5 L daily goal, persisted per day.
- **Shelf** — drag files in, click to open, remove with the × button.
- **Todo** — add, check off, edit and delete tasks.

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
- `design/notch-design.html` — the approved HTML design prototype
  (design reference only; the shipped app is native)

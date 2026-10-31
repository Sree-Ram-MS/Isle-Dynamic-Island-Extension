# Isle for Windows

The Windows version of **Isle** — modelled on the **Nusa** app
([rama2555.github.io/nusa](https://rama2555.github.io/nusa/)): a small
black notch hanging from the top edge of the screen that shows your
music, synced lyrics, timers and files, then quietly gets out of the
way. The GNOME Shell version lives on this repo's `main` branch; this
`exe` branch holds the Windows app under `windows/`. It is an
[Electron](https://www.electronjs.org/) app.

## What it does (v0.2.0)

- **Hover to open, move away to close.** Page dots at the bottom, hover
  them for the page icon bar, mouse wheel or arrow keys switch pages.
- **Music from any player** — Spotify, Apple Music, Media Player,
  YouTube in a browser — read from the real Windows media sessions
  (via `scripts/gsm.ps1`), with album art, seek bar, transport controls
  and a volume slider on the Music page.
- **Synced lyrics** — the Lyrics button splits the notch in two: a
  second pill shows each line as it's sung (from LRCLIB). Click a line
  to jump there.
- **Pomodoro** — Focus / Short / Long sessions (lengths in Settings);
  the countdown shows in the closed notch, with a soft chime and a
  notification at the end.
- **Water** — Drink button with your bottle size, daily goal, an hourly
  chart, and gentle once-an-hour reminders while you're behind.
- **Shelf** — drag files onto the notch, drag them back out into any
  app, click to open; text cards copy on click; Ctrl/Shift+click
  selects several. Files are referenced, never copied.
- **To-do with reminders** — give a task a time and the notch taps you
  when it's due, with Snooze. Tasks can carry a colour and be edited
  by double-clicking.
- **Calendar** — simple month view.
- **The mascot** — when the PC is quiet, the clock fades and a pair of
  eyes appears; they doze off after a while and wave when you return.
- **Popups** — plugging in the charger, losing Wi-Fi and friends show
  briefly in the closed notch.
- **Tray icon + Settings window** — Start with Windows, notch size,
  Minimal notch (a thin black line), mascot, hide-in-full-screen,
  page switches, Pomodoro durations and water goal, all applied
  instantly.

Not yet: volume/brightness/Caps-Lock key overlays. AI Usage
(Antigravity) also stays a GNOME-side feature for now. No page shows
fake data — everything is read live from the machine.

## Run it

Download `Isle-0.2.0.exe` (portable — no install) and double-click it,
or build it yourself:

```
npm install
npm start          # run in place
npm run dist       # build the portable exe into dist/
```

Data is stored in `%APPDATA%/isle-windows/isle-data.json`.

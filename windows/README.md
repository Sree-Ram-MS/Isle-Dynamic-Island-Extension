# Isle for Windows

The Windows version of **Isle** — the living notch that lives in this repo
as a GNOME Shell extension (see the `main` branch). Same pages, same look,
same behaviour: a small pill that sits on your screen, opens on hover, and
switches pages from the dots or the hover icon bar.

This branch (`exe`) holds the Windows app under `windows/`. It is an
[Electron](https://www.electronjs.org/) app, so the UI is the same
HTML/CSS/JS style as the notch itself.

## Pages in v0.1.0

- **Power** — real battery level/state from the system, with the Isle ring
- **Hardware** — live CPU, memory and C: disk usage
- **Calendar** — month view
- **Todo** — tasks, saved locally
- **Focus** — 25/5/15 minute timer with a Windows notification at the end
- **Water** — daily intake tracker (+250 ml), resets each day
- **Shelf** — pin files, click to open
- **Wallpaper** — thumbnails from your Pictures and the built-in Windows
  wallpapers; click to apply
- **Settings** (gear in the notch) — choose which pages show, notch
  position (top centre / right / left), size, and Quit

Music, Lyrics and AI Usage are not in this build yet: on Windows they need
access to the system media sessions, which lands in a later version. No
page shows fake data — what you see is read live from the machine.

## Run it

Download `Isle-0.1.0.exe` (portable — no install) and double-click it, or
build it yourself:

```
npm install
npm start          # run in place
npm run dist       # build the portable exe into dist/
```

Data is stored in `%APPDATA%/isle-windows/isle-data.json`.

Position defaults to top centre (the classic Dynamic Island spot);
Ram's GNOME build sits right-middle, and that position is one click away
in Settings.

# Isle

Isle is an unofficial Ubuntu/GNOME clone of the Nusa notch app: a notch at the top of the screen with
music, synced lyrics, focus timer, water reminders, file/text shelf, to-do, calendar reminders,
wallpaper switching, battery/headphones, a hardware monitor and status peeks.
Built as a GNOME Shell extension (GNOME 45–50, tested target: Ubuntu 26.04 / GNOME 50).

**Inspired by [Nusa for Windows](https://rama2555.github.io/nusa/)** — Isle is an independent
re-creation of its notch idea for Ubuntu/GNOME. No code or assets from the original are included.

## Highlights (v1.6.1)
- Hover the page dots for a **page icon bar**; page changes **slide** smoothly.
- **Lyrics pill** on the Lyrics page, and closing the notch from Lyrics puts the
  **current lyric line in the minimized notch** (it widens to fit the line).
- **Wallpapers page**: tap a thumbnail (from `~/Pictures` or `/usr/share/backgrounds`)
  to apply it. Thumbnails load asynchronously at thumbnail size, so the page stays light.
- **Settings → Pages**: choose which pages appear in the notch.
- Battery ring: **blue when fully charged, red when low**, with a centred bolt while charging.
- Synced lyrics come from [LRCLIB](https://lrclib.net) — free, no account or API key;
  only the song's title, artist, album and length are sent.

## Layout
- `extension/extension.js`   entry point (enable/disable)
- `extension/notch.js`       notch window, expand/collapse, paging, alerts, mascot
- `extension/pages/*.js`     one file per page (music, lyrics, focus, water, shelf, todo, calendar, wallpaper, power, hardware)
- `extension/lib/*.js`       MPRIS watcher, LRCLIB client, services (timer, water, todo, battery, Wi-Fi, Bluetooth, idle), storage, UI helpers
- `extension/stylesheet.css` colours, sizes, accent (`#ffb454`)
- `extension/schemas/`       settings keys; `extension/prefs.js` is the settings window
- `build-deb.sh`             builds `dist/nusa-notch_<version>_all.deb`
- `dev-install.sh`           installs to `~/.local/share/gnome-shell/extensions` for quick testing

## Customising
- Look and colours: `stylesheet.css`. Sizes: `BASE_W/BASE_H/OPEN_W/OPEN_H` at the top of `notch.js`; overall scale is the `notch-scale` setting (default 0.85).
- Add a page: copy a file in `pages/` (needs `id`, `icon`, `actor`, `onShow`, `onHide`, `destroy`) and add it to the `_pages` list in `notch.js`.
- Add a setting: add a `<key>` in the schema XML, a control in `prefs.js`, read it with `settings.get_*`.

## Build and install
    ./build-deb.sh && sudo apt install ./dist/nusa-notch_*_all.deb
    # log out/in once, then:
    nusa-notch-enable

## Debugging
    journalctl -b -o cat /usr/bin/gnome-shell | grep -iE "nusa|JS ERROR"
    gnome-extensions info nusa-notch@linux-clone

GNOME Shell keeps loaded JavaScript in memory: after editing JS, log out and back in (disable/enable is not enough).
If you change `version` in `metadata.json`, bump `VERSION` in `build-deb.sh` too.

## Limits
Other apps' drag-and-drop onto a shell extension is not possible on GNOME, so the Shelf uses Add files / Paste / Copy.
GNOME already shows volume, brightness and keyboard-layout OSDs, so those are not duplicated.

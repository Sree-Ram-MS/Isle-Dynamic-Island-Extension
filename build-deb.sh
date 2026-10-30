#!/bin/sh
# Usage: ./build-deb.sh   ->  dist/nusa-notch_<version>_all.deb
set -e
cd "$(dirname "$0")"
VERSION=1.6.1
UUID=nusa-notch@linux-clone
STAGE=$(mktemp -d)
EXT="$STAGE/usr/share/gnome-shell/extensions/$UUID"

mkdir -p "$EXT" "$STAGE/DEBIAN" "$STAGE/usr/bin" "$STAGE/usr/share/doc/nusa-notch"
glib-compile-schemas extension/schemas
cp -r extension/. "$EXT/"
install -m 755 packaging/nusa-notch-enable "$STAGE/usr/bin/nusa-notch-enable"
install -m 644 packaging/README.Debian "$STAGE/usr/share/doc/nusa-notch/README.Debian"
install -m 755 packaging/postinst "$STAGE/DEBIAN/postinst"
cat > "$STAGE/usr/share/doc/nusa-notch/copyright" <<C
Nusa Notch for Linux: original code, released under the MIT license.
Inspired by "Nusa" for Windows by its independent developer; no code or assets from that app are included.
C

SIZE=$(du -sk "$STAGE/usr" | cut -f1)
cat > "$STAGE/DEBIAN/control" <<C
Package: nusa-notch
Version: $VERSION
Section: gnome
Priority: optional
Architecture: all
Depends: gnome-shell (>= 45)
Recommends: gnome-shell-extension-manager, upower, bluez, network-manager
Installed-Size: $SIZE
Maintainer: Sreeram <sreeram@tecfuge.com>
Homepage: https://rama2555.github.io/nusa/
Description: Notch at the top of the screen for music, lyrics, timers and more
 An unofficial Linux clone of the Nusa notch app, built as a GNOME Shell
 extension. Shows what is playing (any MPRIS player) with synced lyrics from
 LRCLIB, a focus timer, water reminders, a file/text shelf, a to-do list,
 a calendar with reminders, a hardware monitor, battery and headphone status,
 and peeks for Wi-Fi and Caps Lock. Opens when you hover over it.
C

find "$STAGE" -type d -exec chmod 755 {} +
find "$EXT" -type f -exec chmod 644 {} +
mkdir -p dist
dpkg-deb -Zgzip --root-owner-group --build "$STAGE" "dist/nusa-notch_${VERSION}_all.deb"
rm -rf "$STAGE"

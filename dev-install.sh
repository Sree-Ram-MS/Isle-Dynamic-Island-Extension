#!/bin/sh
# Install the working copy for your user only (no sudo, no .deb), so you can edit and test fast.
set -e
cd "$(dirname "$0")"
UUID=nusa-notch@linux-clone
DEST="$HOME/.local/share/gnome-shell/extensions/$UUID"
glib-compile-schemas extension/schemas
mkdir -p "$DEST"
cp -r extension/. "$DEST/"
echo "Copied to $DEST"
echo "GNOME Shell on Wayland caches JS: after the FIRST install, or when you change JS, log out/in."
echo "To try changes without logging out, run a nested shell:"
echo "    dbus-run-session gnome-shell --devkit --wayland   (GNOME 49+)"
echo "Then enable with: gnome-extensions enable $UUID"

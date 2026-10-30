import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

import {emptyState, hbox, makeLabel, vbox} from '../lib/ui.js';

const EXTS = ['.jpg', '.jpeg', '.png', '.webp'];
const MAX = 12;
const COLS = 6;

// Switch the GNOME wallpaper from the notch: thumbnails of the images in
// ~/Pictures (and its Wallpapers subfolder) plus /usr/share/backgrounds.
// Clicking one sets picture-uri and picture-uri-dark immediately.
// Performance: thumbnails are loaded asynchronously by GNOME's texture
// cache and downscaled to thumbnail size while loading, so full-size
// wallpapers are never decoded into the shell. The grid is also built only
// when the folder contents actually change; reopening the page just updates
// which wallpaper is marked current.
export class WallpaperPage {
    constructor(ctx) {
        this.id = 'wallpaper';
        this.icon = 'preferences-desktop-wallpaper-symbolic';
        this.name = 'Wallpapers';
        this.title = 'Wallpapers';
        this._ctx = ctx;
        this._bg = null; // Gio.Settings for org.gnome.desktop.background, lazily created
        this._items = [];
        this._builtKey = null;
        this._build();
    }

    _build() {
        this.actor = new St.Bin({x_expand: true, y_expand: true});
        this._root = vbox(this._ctx.px(6), {x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this.actor.set_child(this._root);
    }

    _settings() {
        if (!this._bg) this._bg = new Gio.Settings({schema_id: 'org.gnome.desktop.background'});
        return this._bg;
    }

    _scan() {
        const dirs = [
            GLib.build_filenamev([GLib.get_home_dir(), 'Pictures', 'Wallpapers']),
            GLib.build_filenamev([GLib.get_home_dir(), 'Pictures']),
            '/usr/share/backgrounds',
        ];
        const found = [];
        const seen = new Set();
        for (const dir of dirs) {
            if (found.length >= MAX) break;
            try {
                const en = Gio.File.new_for_path(dir).enumerate_children(
                    'standard::name,standard::type', Gio.FileQueryInfoFlags.NONE, null);
                const names = [];
                let info;
                while ((info = en.next_file(null))) {
                    if (info.get_file_type() !== Gio.FileType.REGULAR) continue;
                    const name = info.get_name();
                    if (EXTS.some(e => name.toLowerCase().endsWith(e))) names.push(name);
                }
                names.sort();
                for (const name of names) {
                    if (found.length >= MAX) break;
                    const path = GLib.build_filenamev([dir, name]);
                    if (seen.has(path)) continue;
                    seen.add(path);
                    found.push(path);
                }
            } catch (_e) { /* folder does not exist */ }
        }
        return found;
    }

    _current() {
        try {
            return Gio.uri_parse(this._settings().get_string('picture-uri'), GLib.UriFlags.NONE)?.get_path() ?? '';
        } catch (_e) {
            return this._settings().get_string('picture-uri').replace('file://', '');
        }
    }

    _apply(path) {
        const uri = Gio.File.new_for_path(path).get_uri();
        const bg = this._settings();
        bg.set_string('picture-uri', uri);
        bg.set_string('picture-uri-dark', uri);
        this._markSelected();
        if (this._hint) this._hint.text = `Applied · ${GLib.path_get_basename(path)}`;
    }

    _markSelected() {
        const cur = this._current();
        for (const {path, btn} of this._items) {
            if (path === cur) btn.add_style_class_name('sel');
            else btn.remove_style_class_name('sel');
        }
    }

    _thumbnail(path, tw, th) {
        const holder = new St.Bin({style_class: 'nusa-wpthumb', width: tw, height: th});
        try {
            const scale = St.ThemeContext.get_for_stage(global.stage).scale_factor;
            const image = St.TextureCache.get_default().load_file_async(
                Gio.File.new_for_path(path), tw, th, scale, scale);
            image.set_size(tw, th);
            image.content_gravity = Clutter.ContentGravity.RESIZE_FILL;
            holder.set_child(image);
        } catch (_e) {
            // Fallback only if the shell texture cache is unavailable.
            holder.set_style(`background-image: url("file://${path}"); background-size: ${tw}px ${th}px;`);
        }
        return holder;
    }

    refresh() {
        const {px} = this._ctx;
        const paths = this._scan();
        const key = paths.join('\n');
        if (key === this._builtKey) {
            this._markSelected();
            return;
        }
        this._builtKey = key;
        this._root.destroy_all_children();
        this._items = [];
        if (!paths.length) {
            this._root.add_child(emptyState(this._ctx, 'preferences-desktop-wallpaper-symbolic',
                'No wallpapers found', 'Put images in ~/Pictures'));
            return;
        }
        const tw = px(74);
        const th = px(46);
        let row = null;
        paths.forEach((path, i) => {
            if (i % COLS === 0) {
                row = hbox(px(6), {x_align: Clutter.ActorAlign.CENTER});
                this._root.add_child(row);
            }
            const thumb = this._thumbnail(path, tw, th);
            const btn = new St.Button({style_class: 'nusa-wpcard', reactive: true, can_focus: true, track_hover: true, child: thumb});
            btn.connect('clicked', () => {
                this._apply(path);
                return Clutter.EVENT_STOP;
            });
            row.add_child(btn);
            this._items.push({path, btn});
        });
        this._hint = makeLabel('Tap a wallpaper to apply it', 'nusa-dim', {ellipsize: false});
        this._hint.x_align = Clutter.ActorAlign.CENTER;
        this._root.add_child(this._hint);
        this._markSelected();
    }

    onShow() { this.refresh(); }
    onHide() {}

    destroy() {
        this.actor.destroy();
    }
}

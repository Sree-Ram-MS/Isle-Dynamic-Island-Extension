import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Pango from 'gi://Pango';
import St from 'gi://St';

import {emptyState, hbox, iconButton, makeIcon, makeLabel, pill, spacer, vbox} from '../lib/ui.js';

Gio._promisify(Gio.DBusConnection.prototype, 'call', 'call_finish');

// Ask the desktop file chooser (xdg-desktop-portal) for files
function pickFiles(title) {
    return new Promise(resolve => {
        const bus = Gio.DBus.session;
        const token = `nusa${Math.floor(Math.random() * 1e9)}`;
        const sender = bus.get_unique_name().slice(1).replace(/\./g, '_');
        const reqPath = `/org/freedesktop/portal/desktop/request/${sender}/${token}`;
        const sub = bus.signal_subscribe('org.freedesktop.portal.Desktop', 'org.freedesktop.portal.Request',
            'Response', reqPath, null, Gio.DBusSignalFlags.NONE, (_c, _s, _p, _i, _sig, params) => {
                bus.signal_unsubscribe(sub);
                const [code, results] = params.deepUnpack();
                resolve(code === 0 && results.uris ? results.uris.deepUnpack() : []);
            });
        bus.call('org.freedesktop.portal.Desktop', '/org/freedesktop/portal/desktop',
            'org.freedesktop.portal.FileChooser', 'OpenFile',
            new GLib.Variant('(ssa{sv})', ['', title, {
                handle_token: new GLib.Variant('s', token),
                multiple: new GLib.Variant('b', true),
            }]), new GLib.VariantType('(o)'), Gio.DBusCallFlags.NONE, -1, null)
            .catch(() => {
                bus.signal_unsubscribe(sub);
                resolve([]);
            });
    });
}

function setClipboardFiles(uris) {
    const text = `copy\n${uris.join('\n')}`;
    St.Clipboard.get_default().set_content(St.ClipboardType.CLIPBOARD,
        'x-special/gnome-copied-files', new GLib.Bytes(new TextEncoder().encode(text)));
}

export class ShelfPage {
    constructor(ctx) {
        this.id = 'shelf';
        this.icon = 'folder-symbolic';
        this.name = 'Shelf';
        this._ctx = ctx;
        this._selected = new Set();
        this._build();
    }

    get _items() {
        return this._ctx.store.data.shelf;
    }

    _build() {
        const {px} = this._ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});

        this._cards = hbox(px(8), {y_align: Clutter.ActorAlign.CENTER});
        this._scroll = new St.ScrollView({
            x_expand: true, y_expand: true, hscrollbar_policy: St.PolicyType.AUTOMATIC,
            vscrollbar_policy: St.PolicyType.NEVER, overlay_scrollbars: true,
        });
        this._scroll.add_child(this._cards);

        this._count = makeLabel('', 'nusa-dim', {ellipsize: false});
        const bar = hbox(px(6), {x_expand: true});
        bar.add_child(pill('Add files', () => this._addFiles()));
        bar.add_child(pill('Paste', () => this._paste()));
        bar.add_child(this._count);
        bar.add_child(spacer());
        this._copyBtn = pill('Copy selected', () => this._copySelected());
        bar.add_child(this._copyBtn);
        bar.add_child(pill('Clear all', () => this._clear()));

        this._empty = emptyState(this._ctx, 'folder-symbolic', 'Shelf is empty', 'Add files or paste text to keep them handy');
        this._body = vbox(px(6), {x_expand: true, y_expand: true});
        this._body.add_child(bar);
        this._stack = new St.Bin({x_expand: true, y_expand: true});
        this._body.add_child(this._stack);
        this.actor.set_child(this._body);
        this.refresh();
    }

    refresh() {
        const {px} = this._ctx;
        this._cards.destroy_all_children();
        const items = this._items;
        for (const it of items) this._cards.add_child(this._makeCard(it, px));
        this._stack.set_child(items.length ? this._scroll : this._empty);
        this._count.text = items.length ? `${items.length} item${items.length > 1 ? 's' : ''}` : '';
        this._copyBtn.visible = this._selected.size > 0;
    }

    _makeCard(it, px) {
        const card = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL, style_class: `nusa-card${this._selected.has(it.id) ? ' selected' : ''}`,
            reactive: true, track_hover: true, height: px(92),
        });
        card.set_style(`width: ${px(104)}px;`);

        if (it.type === 'file') {
            let gicon = null;
            try {
                const info = Gio.File.new_for_uri(it.uri).query_info('standard::icon,standard::display-name', Gio.FileQueryInfoFlags.NONE, null);
                gicon = info.get_icon();
                it._name ??= info.get_display_name();
            } catch (_e) {
                it._name ??= decodeURIComponent(it.uri.split('/').pop() || it.uri);
            }
            card.add_child(gicon ? new St.Icon({gicon, icon_size: px(32), x_align: Clutter.ActorAlign.CENTER}) : makeIcon('text-x-generic-symbolic', px(32)));
            card.add_child(makeLabel(it._name, 'nusa-card-text'));
        } else {
            const t = makeLabel(it.text.slice(0, 120), 'nusa-card-text', {ellipsize: true});
            t.clutter_text.line_wrap = true;
            t.clutter_text.line_wrap_mode = Pango.WrapMode.WORD_CHAR;
            t.clutter_text.ellipsize = Pango.EllipsizeMode.END;
            card.add_child(t);
        }

        const row = hbox(px(2), {x_align: Clutter.ActorAlign.CENTER});
        row.add_child(iconButton(this._ctx, 'document-open-symbolic', () => this._open(it), {size: 13, cls: 'nusa-ctl'}));
        row.add_child(iconButton(this._ctx, 'edit-copy-symbolic', () => this._copy([it]), {size: 13}));
        row.add_child(iconButton(this._ctx, 'window-close-symbolic', () => this._remove(it), {size: 13}));
        card.add_child(row);

        card.connect('button-press-event', (_a, ev) => {
            if (ev.get_button() !== 1) return Clutter.EVENT_PROPAGATE;
            if (this._selected.has(it.id)) this._selected.delete(it.id);
            else this._selected.add(it.id);
            this.refresh();
            return Clutter.EVENT_STOP;
        });
        return card;
    }

    _open(it) {
        if (it.type === 'file') {
            try {
                Gio.AppInfo.launch_default_for_uri(it.uri, global.create_app_launch_context(0, -1));
            } catch (e) {
                console.error(`[nusa-notch] open failed: ${e.message}`);
            }
        } else {
            this._copy([it]);
        }
    }

    _copy(items) {
        const files = items.filter(i => i.type === 'file').map(i => i.uri);
        if (files.length) setClipboardFiles(files);
        else St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, items.map(i => i.text).join('\n'));
        this._ctx.notch.peek('edit-copy-symbolic', files.length ? 'Copied — paste in any folder' : 'Text copied', 1800);
    }

    _copySelected() {
        this._copy(this._items.filter(i => this._selected.has(i.id)));
    }

    _remove(it) {
        const i = this._items.indexOf(it);
        if (i >= 0) this._items.splice(i, 1);
        this._selected.delete(it.id);
        this._ctx.store.save();
        this.refresh();
    }

    _clear() {
        this._items.length = 0;
        this._selected.clear();
        this._ctx.store.save();
        this.refresh();
    }

    _addUris(uris) {
        for (const uri of uris) {
            if (this._items.some(i => i.type === 'file' && i.uri === uri)) continue;
            this._items.push({id: this._ctx.store.nextId(), type: 'file', uri});
        }
        this._ctx.store.save();
        this.refresh();
    }

    async _addFiles() {
        this._ctx.notch.setPinned(true);
        const uris = await pickFiles('Add to Shelf');
        this._ctx.notch.setPinned(false);
        if (uris.length) this._addUris(uris);
    }

    _paste() {
        const cb = St.Clipboard.get_default();
        cb.get_content(St.ClipboardType.CLIPBOARD, 'x-special/gnome-copied-files', (_c, bytes) => {
            const raw = bytes ? new TextDecoder().decode(bytes.get_data()) : '';
            const uris = raw.split('\n').map(s => s.trim()).filter(s => s.startsWith('file://'));
            if (uris.length) {
                this._addUris(uris);
                return;
            }
            cb.get_text(St.ClipboardType.CLIPBOARD, (_c2, text) => {
                if (!text?.trim()) return;
                const lines = text.trim().split('\n').map(s => s.trim());
                const asUris = lines.map(l => (l.startsWith('/') ? Gio.File.new_for_path(l).get_uri() : l));
                if (asUris.every(u => u.startsWith('file://') && GLib.file_test(Gio.File.new_for_uri(u).get_path(), GLib.FileTest.EXISTS))) {
                    this._addUris(asUris);
                } else {
                    this._items.push({id: this._ctx.store.nextId(), type: 'text', text: text.trim()});
                    this._ctx.store.save();
                    this.refresh();
                }
            });
        });
    }

    onShow() { this.refresh(); }
    onHide() {}

    destroy() {
        this.actor.destroy();
    }
}

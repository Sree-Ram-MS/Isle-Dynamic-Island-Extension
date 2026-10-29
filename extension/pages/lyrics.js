import Clutter from 'gi://Clutter';
import Pango from 'gi://Pango';
import St from 'gi://St';

import {lineIndexAt} from '../lib/lyrics.js';
import {emptyState, vbox} from '../lib/ui.js';

const ROWS = 5;
const MID = 2;

export class LyricsPage {
    constructor(ctx) {
        this.id = 'lyrics';
        this.icon = 'format-justify-left-symbolic';
        this.name = 'Lyrics';
        this._ctx = ctx;
        this._lines = [];
        this._key = '';
        this._status = 'idle'; // idle | loading | none | ok
        this._tick = 0;
        this._shown = false;
        this._build();
    }

    _build() {
        const {px} = this._ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});
        this._empty = emptyState(this._ctx, 'format-justify-left-symbolic', 'Lyrics', '');
        this._list = vbox(px(2), {x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this._rows = [];
        for (let i = 0; i < ROWS; i++) {
            const btn = new St.Button({style_class: 'nusa-lyric', reactive: true, can_focus: true, track_hover: true, x_expand: true});
            btn.set_x_align(Clutter.ActorAlign.FILL);
            const label = new St.Label({text: ''});
            label.clutter_text.ellipsize = Pango.EllipsizeMode.END;
            label.x_align = Clutter.ActorAlign.CENTER;
            btn.set_child(label);
            btn.connect('clicked', () => {
                const line = this._rowLine(i);
                if (line) this._ctx.mpris.seekTo(line.t);
                return Clutter.EVENT_STOP;
            });
            this._list.add_child(btn);
            this._rows.push({btn, label});
        }
        this._current = -1;
        this.refresh();
    }

    _rowLine(row) {
        const base = Math.max(this._current, 0) - MID + row;
        return this._current < 0 ? this._lines[row] : this._lines[base];
    }

    refresh() {
        const m = this._ctx.mpris;
        const t = m.track;
        if (!t) {
            this._setEmpty('No song playing', '');
            return;
        }
        if (!this._ctx.settings.get_boolean('lyrics-enabled')) {
            this._setEmpty('Lyrics are turned off', 'Enable them in settings');
            return;
        }
        const key = m.trackKey();
        if (key !== this._key) {
            this._key = key;
            this._lines = [];
            this._status = 'idle';
            if (this._shown) this._load(t, key);
        } else if (this._shown && this._status === 'idle') {
            this._load(t, key);
        }
        if (this._status === 'loading') this._setEmpty('Looking for lyrics…', `${t.title} · ${t.artist}`);
        else if (this._status === 'none') this._setEmpty('No synced lyrics for this song', `${t.title} · ${t.artist}`);
        else if (this._status === 'ok') {
            this.actor.set_child(this._list);
            this._render();
        } else {
            this._setEmpty('Lyrics', '');
        }
    }

    _setEmpty(title, sub) {
        this._empty.destroy();
        this._empty = emptyState(this._ctx, 'format-justify-left-symbolic', title, sub);
        this.actor.set_child(this._empty);
    }

    async _load(track, key) {
        this._status = 'loading';
        this.refresh();
        const lines = await this._ctx.lyrics.fetch(track);
        if (key !== this._key) return;
        if (lines && lines.length) {
            this._lines = lines;
            this._status = 'ok';
        } else {
            this._status = lines === null ? 'idle' : 'none';
        }
        this.refresh();
    }

    _render() {
        const idx = lineIndexAt(this._lines, this._ctx.mpris.positionMs + 150);
        this._current = idx;
        for (let r = 0; r < ROWS; r++) {
            const {btn, label} = this._rows[r];
            const line = this._rowLine(r);
            label.text = line ? (line.text || '♪') : '';
            btn.reactive = !!line;
            const isCur = idx >= 0 && r === MID;
            btn.style_class = `nusa-lyric${isCur ? ' current' : (Math.abs(r - MID) === 1 ? ' near' : '')}`;
        }
    }

    onShow() {
        this._shown = true;
        this._ctx.mpris.refreshPosition();
        this.refresh();
        this._tick = this._ctx.timers.add(250, () => {
            if (this._status === 'ok' && this._ctx.mpris.playing) this._render();
            return true;
        });
    }

    onHide() {
        this._shown = false;
        this._ctx.timers.clear(this._tick);
        this._tick = 0;
    }

    destroy() {
        this.onHide();
        this.actor.destroy();
    }
}

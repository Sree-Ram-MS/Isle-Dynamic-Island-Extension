import Clutter from 'gi://Clutter';
import Pango from 'gi://Pango';
import St from 'gi://St';

import {lineIndexAt} from '../lib/lyrics.js';
import {emptyState, vbox} from '../lib/ui.js';

const ROWS = 4; // context lines around the pill (2 before, 2 after the current line)

export class LyricsPage {
    constructor(ctx) {
        this.id = 'lyrics';
        this.icon = 'format-justify-left-symbolic';
        this.name = 'Lyrics';
        this.hidden = true; // reached from the music page chat button, not in the dots
        this._ctx = ctx;
        this._lines = [];
        this._key = '';
        this._status = 'idle'; // idle | loading | none | ok
        this._tick = 0;
        this._shown = false;
        this._mini = false;
        this._miniText = '';
        this._build();
    }

    _build() {
        const {px} = this._ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});
        this._empty = emptyState(this._ctx, 'format-justify-left-symbolic', 'Lyrics', '');

        // the current line, shown big in a tinted pill (♪ + line), like the reference
        this._pillText = new St.Label({text: '', style_class: 'nusa-lyric-pill-text'});
        this._pillText.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        this._pillText.x_align = Clutter.ActorAlign.CENTER;
        const pill = new St.BoxLayout({
            orientation: Clutter.Orientation.HORIZONTAL, style_class: 'nusa-lyric-pill',
            x_align: Clutter.ActorAlign.CENTER, style: `spacing: ${px(8)}px;`,
        });
        const note = new St.Label({text: '♪', style_class: 'nusa-lyric-pill-note'});
        note.y_align = Clutter.ActorAlign.CENTER;
        this._pillText.y_align = Clutter.ActorAlign.CENTER;
        pill.add_child(note);
        pill.add_child(this._pillText);
        const pillRow = new St.BoxLayout({orientation: Clutter.Orientation.HORIZONTAL, x_expand: true, x_align: Clutter.ActorAlign.CENTER});
        pillRow.add_child(pill);

        this._list = vbox(px(2), {x_expand: true, y_align: Clutter.ActorAlign.CENTER});
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

        this._main = vbox(px(6), {x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this._main.add_child(pillRow);
        this._main.add_child(this._list);
        this._current = -1;
        this.refresh();
    }

    // context rows skip the current line (it is in the pill): -2, -1, +1, +2
    _rowLine(row) {
        if (this._current < 0) return this._lines[row + 1];
        const off = [-2, -1, 1, 2][row];
        return this._lines[this._current + off];
    }

    refresh() {
        const m = this._ctx.mpris;
        const t = m.track;
        if (!t) {
            this._setEmpty('No song playing', '');
            this._publishMini('');
            return;
        }
        if (!this._ctx.settings.get_boolean('lyrics-enabled')) {
            this._setEmpty('Lyrics are turned off', 'Enable them in settings');
            this._publishMini('');
            return;
        }
        const key = m.trackKey();
        const active = this._shown || this._mini;
        if (key !== this._key) {
            this._key = key;
            this._lines = [];
            this._status = 'idle';
            if (active) this._load(t, key);
        } else if (active && this._status === 'idle') {
            this._load(t, key);
        }
        if (this._status === 'loading') this._setEmpty('Looking for lyrics…', `${t.title} · ${t.artist}`);
        else if (this._status === 'none') this._setEmpty('No synced lyrics for this song', `${t.title} · ${t.artist}`);
        else if (this._status === 'ok') {
            this.actor.set_child(this._main);
            this._render();
        } else {
            this._setEmpty('Lyrics', '');
        }
        this._publishMini();
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
        const cur = idx >= 0 ? this._lines[idx] : null;
        const t = this._ctx.mpris.track;
        this._pillText.text = cur ? (cur.text || '♪') : (t ? t.title : '');
        for (let r = 0; r < ROWS; r++) {
            const {btn, label} = this._rows[r];
            const line = this._rowLine(r);
            label.text = line ? (line.text || '♪') : '';
            btn.reactive = !!line;
            btn.style_class = `nusa-lyric${r === 1 || r === 2 ? ' near' : ''}`;
        }
        this._publishMini();
    }

    // ---- minimized-notch lyrics -------------------------------------------
    // After the user closes the notch from this page, the collapsed notch
    // can replace its clock with the current line. The notch owns the
    // display; this page only publishes the text when it changes.
    canMini() {
        return !!this._ctx.mpris.track &&
            this._ctx.settings.get_boolean('lyrics-enabled') &&
            this._status !== 'none';
    }

    _currentMiniText() {
        const t = this._ctx.mpris.track;
        if (!t || !this._ctx.settings.get_boolean('lyrics-enabled')) return '';
        if (this._status === 'loading') return t.title ?? '';
        if (this._status !== 'ok') return '';
        const idx = lineIndexAt(this._lines, this._ctx.mpris.positionMs + 150);
        return idx >= 0 ? (this._lines[idx].text || '♪') : (t.title ?? '');
    }

    _publishMini(text = null) {
        if (!this._mini) return;
        const next = text ?? this._currentMiniText();
        if (next === this._miniText) return;
        this._miniText = next;
        this._ctx.notch.setMiniLyric?.(next);
    }

    setMiniActive(active) {
        this._mini = active;
        if (active) {
            this._ctx.mpris.refreshPosition();
            this.refresh();
            this._publishMini();
            this._startTick();
        } else {
            if (this._miniText !== '') {
                this._miniText = '';
                this._ctx.notch.setMiniLyric?.('');
            }
            if (!this._shown) this._stopTick();
        }
    }

    _startTick() {
        if (this._tick) return;
        this._tick = this._ctx.timers.add(250, () => {
            if (this._status === 'ok') {
                if (this._shown) {
                    if (this._ctx.mpris.playing) this._render();
                } else if (this._mini) {
                    // While paused the position cannot move, so the line
                    // published on arm/pause is already correct.
                    if (this._ctx.mpris.playing) this._publishMini();
                }
            }
            return true;
        });
    }

    _stopTick() {
        this._ctx.timers.clear(this._tick);
        this._tick = 0;
    }

    onShow() {
        this._shown = true;
        this._ctx.mpris.refreshPosition();
        this.refresh();
        this._startTick();
    }

    onHide() {
        this._shown = false;
        if (this._mini) {
            this._startTick();
            this._publishMini();
        } else {
            this._stopTick();
        }
    }

    destroy() {
        this._mini = false;
        this._shown = false;
        this._stopTick();
        this.actor.destroy();
    }
}

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import St from 'gi://St';

import {fmtTime} from '../lib/util.js';
import {hbox, makeIcon, makeLabel, vbox} from '../lib/ui.js';

const COLORS = {
    focus: {rgb: [0.91, 0.38, 0.42], css: '#e8606a'},
    break: {rgb: [0.31, 0.72, 0.63], css: '#4fb7a0'},
};

export class FocusPage {
    constructor(ctx) {
        this.id = 'focus';
        this.icon = 'alarm-symbolic';
        this.name = 'Focus';
        this.title = 'Pomodoro'; // shown in the notch header
        this._ctx = ctx;
        this._build();
        this._unsub = ctx.focus.changed.connect(() => this.refresh());
        this.refresh();
    }

    _fileIcon(name) {
        return new Gio.FileIcon({file: Gio.File.new_for_path(`${this._ctx.path}/icons/${name}-symbolic.svg`)});
    }

    _fileIconWidget(name, size, cls = '') {
        const i = makeIcon('image-missing-symbolic', size, cls);
        i.gicon = this._fileIcon(name);
        return i;
    }

    _build() {
        const {px, focus} = this._ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});

        // ---- left: title + progress ring with stopwatch
        const ringSize = px(88);
        this._ring = new St.DrawingArea({width: ringSize, height: ringSize});
        this._ring.connect('repaint', a => this._paintRing(a));
        const ringBox = new St.Widget({width: ringSize, height: ringSize, layout_manager: new Clutter.BinLayout()});
        ringBox.add_child(this._ring);
        const sw = this._fileIconWidget('stopwatch', px(38), 'nusa-white');
        sw.x_expand = sw.y_expand = true;
        sw.x_align = sw.y_align = Clutter.ActorAlign.CENTER;
        ringBox.add_child(sw);

        const left = vbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        left.add_child(ringBox);

        // ---- right: time + reset, label, mode pills + play
        this._time = makeLabel('25:00', 'nusa-fbig', {ellipsize: false});
        const reset = new St.Button({style_class: 'nusa-round', reactive: true, can_focus: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER, child: makeIcon('view-refresh-symbolic', px(15))});
        reset.connect('clicked', () => {
            focus.reset();
            return Clutter.EVENT_STOP;
        });
        const timeRow = hbox(px(14), {x_align: Clutter.ActorAlign.CENTER});
        timeRow.add_child(this._time);
        timeRow.add_child(reset);

        this._label = makeLabel('Focus time', 'nusa-fsub', {ellipsize: false});
        this._label.x_align = Clutter.ActorAlign.CENTER;

        this._breakBtn = this._modePill('cup', 'Short Break', () => focus.setMode('break'));
        this._focusBtn = this._modePill('target', 'Focus Time', () => focus.setMode('focus'));
        this._playBtn = new St.Button({style_class: 'nusa-fplay', reactive: true, can_focus: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER, child: makeIcon('media-playback-start-symbolic', px(16))});
        this._playBtn.connect('clicked', () => {
            focus.toggle();
            return Clutter.EVENT_STOP;
        });
        const btnRow = hbox(px(10), {x_align: Clutter.ActorAlign.CENTER});
        btnRow.add_child(this._breakBtn);
        btnRow.add_child(this._playBtn);
        btnRow.add_child(this._focusBtn);

        const right = vbox(px(3), {x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.CENTER});
        right.add_child(timeRow);
        right.add_child(this._label);
        right.add_child(new St.Widget({height: px(6)}));
        right.add_child(btnRow);

        const row = hbox(px(10), {x_expand: true, y_expand: true});
        row.add_child(left);
        row.add_child(right);
        this.actor.set_child(row);
    }

    _modePill(icon, text, onClick) {
        const {px} = this._ctx;
        const box = hbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        box.add_child(this._fileIconWidget(icon, px(15), 'nusa-white'));
        box.add_child(makeLabel(text, 'nusa-fpill-text', {ellipsize: false}));
        const b = new St.Button({style_class: 'nusa-fpill', reactive: true, can_focus: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER, child: box});
        b.connect('clicked', () => {
            onClick();
            return Clutter.EVENT_STOP;
        });
        return b;
    }

    _paintRing(area) {
        const f = this._ctx.focus;
        const [w, h] = area.get_surface_size();
        const lw = Math.max(5, Math.round(w * 0.085));
        const r = (Math.min(w, h) - lw) / 2;
        const cx = w / 2;
        const cy = h / 2;
        const [cr_, cg, cb] = COLORS[f.mode].rgb;
        const cr = area.get_context();
        cr.setLineWidth(lw);
        cr.setSourceRGBA(cr_, cg, cb, 0.22);
        cr.arc(cx, cy, r, 0, 2 * Math.PI);
        cr.stroke();
        const frac = Math.max(0, Math.min(1, f.total > 0 ? f.remaining / f.total : 1));
        if (frac > 0) {
            cr.setLineCap(1); // round
            cr.setSourceRGBA(cr_, cg, cb, 1);
            const start = -Math.PI / 2;
            if (frac >= 0.999) cr.arc(cx, cy, r, 0, 2 * Math.PI);
            else cr.arc(cx, cy, r, start, start + 2 * Math.PI * frac);
            cr.stroke();
        }
        cr.$dispose();
    }

    refresh() {
        const f = this._ctx.focus;
        const isFocus = f.mode === 'focus';
        this._time.text = fmtTime(f.remaining * 1000);
        this._label.text = (isFocus ? 'Focus time' : 'Break time') + (f.sessions ? `  ·  ${f.sessions} done` : '');
        this._playBtn.child.icon_name = f.running ? 'media-playback-pause-symbolic' : 'media-playback-start-symbolic';
        this._playBtn.set_style(`background-color: ${COLORS[f.mode].css};`);
        for (const [btn, on] of [[this._focusBtn, isFocus], [this._breakBtn, !isFocus]]) {
            if (on) btn.add_style_class_name('active');
            else btn.remove_style_class_name('active');
        }
        this._ring.queue_repaint();
    }

    onShow() { this.refresh(); }
    onHide() {}

    destroy() {
        this._unsub();
        this.actor.destroy();
    }
}

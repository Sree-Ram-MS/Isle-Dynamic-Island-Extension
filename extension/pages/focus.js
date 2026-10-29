import Clutter from 'gi://Clutter';
import St from 'gi://St';

import {fmtTime} from '../lib/util.js';
import {hbox, iconButton, makeLabel, pill, vbox} from '../lib/ui.js';

export class FocusPage {
    constructor(ctx) {
        this.id = 'focus';
        this.icon = 'alarm-symbolic';
        this.name = 'Focus';
        this._ctx = ctx;
        const {px} = ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});

        this._mode = makeLabel('', 'nusa-sub', {ellipsize: false});
        this._time = makeLabel('25:00', 'nusa-big', {ellipsize: false});
        this._sessions = makeLabel('', 'nusa-dim', {ellipsize: false});
        this._startBtn = pill('Start', () => ctx.focus.toggle(), true);

        const row = hbox(px(8), {x_align: Clutter.ActorAlign.CENTER});
        row.add_child(this._startBtn);
        row.add_child(iconButton(ctx, 'view-refresh-symbolic', () => ctx.focus.reset(), {size: 16}));
        row.add_child(iconButton(ctx, 'media-skip-forward-symbolic', () => ctx.focus.skip(), {size: 16}));

        const col = vbox(px(2), {x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        col.add_child(this._mode);
        col.add_child(this._time);
        col.add_child(row);
        col.add_child(this._sessions);
        this.actor.set_child(col);

        this._unsub = ctx.focus.changed.connect(() => this.refresh());
        this.refresh();
    }

    refresh() {
        const f = this._ctx.focus;
        this._mode.text = f.mode === 'focus' ? 'Focus session' : 'Break time';
        this._time.text = fmtTime(f.remaining * 1000);
        this._startBtn.label = f.running ? 'Pause' : (f.remaining < this._len() ? 'Resume' : 'Start');
        this._sessions.text = f.sessions ? `${f.sessions} session${f.sessions > 1 ? 's' : ''} completed` : 'Pomodoro';
    }

    _len() {
        const s = this._ctx.settings;
        return s.get_int(this._ctx.focus.mode === 'focus' ? 'focus-minutes' : 'break-minutes') * 60;
    }

    onShow() { this.refresh(); }
    onHide() {}

    destroy() {
        this._unsub();
        this.actor.destroy();
    }
}

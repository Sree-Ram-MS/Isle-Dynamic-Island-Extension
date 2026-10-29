import Clutter from 'gi://Clutter';
import St from 'gi://St';

import {BATTERY_STATE} from '../lib/services.js';
import {emptyState, hbox, makeIcon, makeLabel, progressBar, vbox} from '../lib/ui.js';

function eta(seconds) {
    if (!seconds) return '';
    const h = Math.floor(seconds / 3600);
    const m = Math.round((seconds % 3600) / 60);
    return h ? `${h} h ${m} min` : `${m} min`;
}

export class ChargingPage {
    constructor(ctx) {
        this.id = 'charging';
        this.icon = 'battery-full-charging-symbolic';
        this.name = 'Charging';
        this._ctx = ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});
        this._unsub = ctx.sys.changed.connect(() => this.refresh());
        this.refresh();
    }

    refresh() {
        const {px} = this._ctx;
        const b = this._ctx.sys.battery;
        if (!b.present) {
            this.actor.set_child(emptyState(this._ctx, 'battery-missing-symbolic', 'No battery', 'This computer runs on mains power'));
            return;
        }
        const charging = b.state === 1;
        const bar = progressBar(px(220), '');
        bar.set(b.percent / 100);
        const col = vbox(px(4), {x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        const top = hbox(px(10), {x_align: Clutter.ActorAlign.CENTER});
        top.add_child(makeIcon(charging ? 'battery-full-charging-symbolic' : 'battery-good-symbolic', px(30)));
        top.add_child(makeLabel(`${b.percent}%`, 'nusa-big', {ellipsize: false}));
        col.add_child(top);
        col.add_child(bar.actor);
        let sub = BATTERY_STATE[b.state] ?? '';
        if (charging && b.timeToFull) sub += ` · ${eta(b.timeToFull)} until full`;
        else if (b.state === 2 && b.timeToEmpty) sub += ` · ${eta(b.timeToEmpty)} left`;
        const subLabel = makeLabel(sub, 'nusa-sub', {ellipsize: false});
        subLabel.x_align = Clutter.ActorAlign.CENTER;
        col.add_child(subLabel);
        this.actor.set_child(col);
    }

    onShow() { this.refresh(); }
    onHide() {}

    destroy() {
        this._unsub();
        this.actor.destroy();
    }
}

export class HeadphonesPage {
    constructor(ctx) {
        this.id = 'headphones';
        this.icon = 'audio-headphones-symbolic';
        this.name = 'Headphones';
        this._ctx = ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});
        this._unsub = ctx.sys.changed.connect(() => {
            if (this._visible) this.refresh();
        });
        this._visible = false;
        this.refresh();
    }

    async refresh() {
        const {px} = this._ctx;
        const devices = await this._ctx.sys.connectedHeadphones();
        if (!devices.length) {
            this.actor.set_child(emptyState(this._ctx, 'audio-headphones-symbolic', 'No headphones connected', 'Pair Bluetooth headphones to see them here'));
            return;
        }
        const col = vbox(px(8), {x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        for (const d of devices) {
            const row = hbox(px(12), {x_align: Clutter.ActorAlign.CENTER});
            row.add_child(makeIcon('audio-headphones-symbolic', px(30)));
            const info = vbox(px(2));
            info.add_child(makeLabel(d.name, 'nusa-title'));
            info.add_child(makeLabel(d.battery !== null ? `Battery ${d.battery}%` : 'Connected', 'nusa-sub'));
            row.add_child(info);
            col.add_child(row);
        }
        this.actor.set_child(col);
    }

    onShow() {
        this._visible = true;
        this.refresh();
    }

    onHide() { this._visible = false; }

    destroy() {
        this._unsub();
        this.actor.destroy();
    }
}

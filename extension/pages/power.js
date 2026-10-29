import Clutter from 'gi://Clutter';
import St from 'gi://St';

import {BATTERY_STATE} from '../lib/services.js';
import {hbox, makeIcon, makeLabel, thinBar, vbox} from '../lib/ui.js';

function eta(seconds) {
    if (!seconds) return '';
    const h = Math.floor(seconds / 3600);
    const m = Math.round((seconds % 3600) / 60);
    return h ? `${h} h ${m} min` : `${m} min`;
}

// Battery / charging and connected Bluetooth headphones, side by side on one page.
export class PowerPage {
    constructor(ctx) {
        this.id = 'power';
        this.icon = 'battery-full-charging-symbolic';
        this.name = 'Power';
        this.title = 'Battery & Headphones';
        this._ctx = ctx;
        this._visible = false;
        this.actor = new St.Bin({x_expand: true, y_expand: true});
        this._root = hbox(ctx.px(8), {x_expand: true, y_expand: true});
        this.actor.set_child(this._root);
        this._batt = vbox(ctx.px(4), {style_class: 'nusa-pcard', x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.FILL});
        this._phones = vbox(ctx.px(4), {style_class: 'nusa-pcard', x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.FILL});
        this._root.add_child(this._batt);
        this._root.add_child(this._phones);
        this._unsub = ctx.sys.changed.connect(() => {
            this._drawBattery();
            if (this._visible) this._drawHeadphones();
        });
        this._drawBattery();
        this._drawHeadphones();
    }

    _bar(frac, color) {
        const bar = thinBar(this._ctx.px(5), color);
        bar.set(frac);
        return bar.actor;
    }

    // a card: big icon + value, caption line, optional bar
    _fill(card, {icon, big, caption, frac, color, dim}) {
        const {px} = this._ctx;
        card.destroy_all_children();
        const col = vbox(px(5), {x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.CENTER});
        const top = hbox(px(8), {y_align: Clutter.ActorAlign.CENTER});
        top.add_child(makeIcon(icon, px(26), dim ? 'nusa-dim' : ''));
        top.add_child(makeLabel(big, dim ? 'nusa-ptitle dim' : 'nusa-ptitle', {ellipsize: true, xExpand: true}));
        col.add_child(top);
        col.add_child(makeLabel(caption, 'nusa-sub', {ellipsize: true}));
        if (frac !== null && frac !== undefined) col.add_child(this._bar(frac, color));
        card.add_child(col);
    }

    _drawBattery() {
        const b = this._ctx.sys.battery;
        if (!b.present) {
            this._fill(this._batt, {icon: 'battery-missing-symbolic', big: 'No battery', caption: 'Running on mains power', dim: true});
            return;
        }
        const charging = b.state === 1;
        let sub = BATTERY_STATE[b.state] ?? '';
        if (charging && b.timeToFull) sub += ` · ${eta(b.timeToFull)} to full`;
        else if (b.state === 2 && b.timeToEmpty) sub += ` · ${eta(b.timeToEmpty)} left`;
        const color = charging || b.percent > 20 ? '#30d158' : (b.percent > 10 ? '#ff9f0a' : '#ff453a');
        this._fill(this._batt, {
            icon: charging ? 'battery-full-charging-symbolic' : 'battery-good-symbolic',
            big: `${b.percent}%`, caption: sub, frac: b.percent / 100, color,
        });
    }

    async _drawHeadphones() {
        const devices = await this._ctx.sys.connectedHeadphones();
        if (!this._phones || !this.actor) return;
        if (!devices.length) {
            this._fill(this._phones, {icon: 'audio-headphones-symbolic', big: 'Headphones', caption: 'Not connected', dim: true});
            return;
        }
        const d = devices[0];
        const more = devices.length > 1 ? ` (+${devices.length - 1})` : '';
        this._fill(this._phones, {
            icon: 'audio-headphones-symbolic', big: d.name, caption: (d.battery !== null ? `Battery ${d.battery}%` : 'Connected') + more,
            frac: d.battery !== null ? d.battery / 100 : null, color: '#3cc6f4',
        });
    }

    onShow() {
        this._visible = true;
        this._drawBattery();
        this._drawHeadphones();
    }

    onHide() { this._visible = false; }

    destroy() {
        this._unsub();
        this._phones = null;
        this.actor.destroy();
    }
}

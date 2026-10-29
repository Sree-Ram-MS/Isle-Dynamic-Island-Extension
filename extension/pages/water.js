import Clutter from 'gi://Clutter';
import St from 'gi://St';

import {weekdayLetter} from '../lib/util.js';
import {hbox, makeLabel, pill, progressBar, spacer, vbox} from '../lib/ui.js';

export class WaterPage {
    constructor(ctx) {
        this.id = 'water';
        this.icon = 'weather-showers-symbolic';
        this.name = 'Water';
        this._ctx = ctx;
        const {px} = ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});

        this._amount = makeLabel('', 'nusa-big', {ellipsize: false});
        this._goal = makeLabel('', 'nusa-sub', {ellipsize: false});
        this._bar = progressBar(px(190), 'water');
        this._drink = pill('', () => ctx.water.drink(), true);
        this._undo = pill('Undo', () => ctx.water.undo());

        const btns = hbox(px(8));
        btns.add_child(this._drink);
        btns.add_child(this._undo);

        const left = vbox(px(4), {y_align: Clutter.ActorAlign.CENTER});
        left.add_child(this._amount);
        left.add_child(this._goal);
        left.add_child(this._bar.actor);
        left.add_child(btns);

        this._chart = hbox(px(8), {y_align: Clutter.ActorAlign.END, x_align: Clutter.ActorAlign.END});
        const root = hbox(px(20), {x_expand: true, y_expand: true});
        root.add_child(left);
        root.add_child(spacer());
        root.add_child(this._chart);
        this.actor.set_child(root);

        this._unsub = ctx.water.changed.connect(() => this.refresh());
        this.refresh();
    }

    refresh() {
        const {px, water, settings} = this._ctx;
        const total = water.total();
        const goal = water.goal;
        this._amount.text = total >= 1000 ? `${(total / 1000).toFixed(2)} L` : `${total} ml`;
        this._goal.text = `of ${(goal / 1000).toFixed(1)} L daily goal${settings.get_boolean('water-reminders') ? '' : ' · reminders off'}`;
        this._bar.set(total / goal);
        this._drink.label = `Drink +${water.bottle} ml`;

        this._chart.destroy_all_children();
        const week = water.week();
        const max = Math.max(goal, ...week.map(d => d.ml));
        const chartH = px(70);
        for (const d of week) {
            const col = vbox(px(3), {y_align: Clutter.ActorAlign.END});
            const h = Math.max(px(3), Math.round(chartH * d.ml / max));
            const wrap = new St.Widget({height: chartH, width: px(14), layout_manager: new Clutter.BinLayout()});
            const bar = new St.Widget({style_class: `nusa-bar-col${d.daysAgo === 0 ? ' today' : ''}`, height: h, width: px(14), y_align: Clutter.ActorAlign.END, y_expand: true});
            bar.set_style(`width: ${px(14)}px;`);
            wrap.add_child(bar);
            col.add_child(wrap);
            col.add_child(makeLabel(weekdayLetter(d.daysAgo), 'nusa-dim', {ellipsize: false}));
            this._chart.add_child(col);
        }
    }

    onShow() { this.refresh(); }
    onHide() {}

    destroy() {
        this._unsub();
        this.actor.destroy();
    }
}

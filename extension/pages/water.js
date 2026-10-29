import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

import {fmtHM, todayKey} from '../lib/util.js';
import {hbox, makeIcon, makeLabel, vbox} from '../lib/ui.js';

const BLUE = [0.29, 0.66, 1.0];
const YLABEL_W = 36;

function roundRect(cr, x, y, w, h, r) {
    r = Math.min(r, w / 2, h / 2);
    cr.newSubPath();
    cr.arc(x + w - r, y + r, r, -Math.PI / 2, 0);
    cr.arc(x + w - r, y + h - r, r, 0, Math.PI / 2);
    cr.arc(x + r, y + h - r, r, Math.PI / 2, Math.PI);
    cr.arc(x + r, y + r, r, Math.PI, 1.5 * Math.PI);
    cr.closePath();
}

export class WaterPage {
    constructor(ctx) {
        this.id = 'water';
        this.icon = 'weather-showers-symbolic';
        this.name = 'Water';
        this.titleMarkup = 'Water <span foreground="#4fc3f7">Reminder</span>';
        this._ctx = ctx;
        this._graph = 0; // 0 = hourly bars, 1 = ring gauge
        this._build();
        this._unsub = ctx.water.changed.connect(() => this.refresh());
        this._setSig = ctx.settings.connect('changed', () => this.refresh());
        this.refresh();
    }

    _fileIconWidget(name, size, cls = '') {
        const i = makeIcon('image-missing-symbolic', size, cls);
        i.gicon = new Gio.FileIcon({file: Gio.File.new_for_path(`${this._ctx.path}/icons/${name}-symbolic.svg`)});
        return i;
    }

    _build() {
        const {px, water, settings} = this._ctx;
        this.actor = new St.Widget({x_expand: true, y_expand: true, layout_manager: new Clutter.BinLayout()});

        // ------------------------------------------------ left column
        this._amount = makeLabel('0.0', 'nusa-wbig', {ellipsize: false});
        this._goal = makeLabel('/ 1.5 L', 'nusa-wgoal', {ellipsize: false});
        this._goal.y_align = Clutter.ActorAlign.END;
        const amountBox = hbox(px(5), {y_align: Clutter.ActorAlign.CENTER});
        amountBox.add_child(this._amount);
        amountBox.add_child(this._goal);

        // reminders switch
        this._knob = new St.Widget({style_class: 'nusa-knob', y_align: Clutter.ActorAlign.CENTER});
        this._knob.set_size(px(18), px(18));
        const track = new St.Widget({layout_manager: new Clutter.BinLayout(), width: px(40), height: px(20)});
        this._knob.x_expand = true;
        track.add_child(this._knob);
        this._switch = new St.Button({style_class: 'nusa-switch', reactive: true, can_focus: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER, child: track});
        this._switch.connect('clicked', () => {
            settings.set_boolean('water-reminders', !settings.get_boolean('water-reminders'));
            return Clutter.EVENT_STOP;
        });

        const topRow = hbox(px(8), {x_expand: true});
        topRow.add_child(amountBox);
        const sp = new St.Widget({x_expand: true});
        topRow.add_child(sp);
        topRow.add_child(this._switch);

        this._sub = makeLabel('', 'nusa-wsub', {ellipsize: false});

        // thin progress bar with a knob at the end
        this._bar = new St.DrawingArea({x_expand: true, height: px(10)});
        this._bar.connect('repaint', a => this._paintBarProgress(a));

        const drink = new St.Button({
            style_class: 'nusa-wdrink', reactive: true, can_focus: true, track_hover: true,
            button_mask: St.ButtonMask.ONE | St.ButtonMask.THREE, y_align: Clutter.ActorAlign.CENTER,
        });
        const dbox = hbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        dbox.add_child(this._fileIconWidget('drop', px(15), 'nusa-wdrop'));
        dbox.add_child(makeLabel('Drink', 'nusa-fpill-text', {ellipsize: false}));
        drink.child = dbox;
        // left click: drink, right click: undo the last one
        drink.connect('clicked', (_b, button) => {
            if (button === 3) water.undo();
            else water.drink();
            return Clutter.EVENT_STOP;
        });

        const set = new St.Button({style_class: 'nusa-wset', reactive: true, can_focus: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER});
        const sbox = hbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        sbox.add_child(this._fileIconWidget('sliders', px(15), 'nusa-white'));
        sbox.add_child(makeLabel('Settings', 'nusa-fpill-text', {ellipsize: false}));
        set.child = sbox;
        set.connect('clicked', () => {
            this._showSettings(true);
            return Clutter.EVENT_STOP;
        });

        const btns = hbox(px(8));
        btns.add_child(drink);
        btns.add_child(set);

        const left = vbox(px(5), {y_align: Clutter.ActorAlign.CENTER, width: px(214)});
        left.add_child(topRow);
        left.add_child(this._sub);
        left.add_child(this._bar);
        left.add_child(new St.Widget({height: px(3)}));
        left.add_child(btns);

        const divider = new St.Widget({style_class: 'nusa-vdiv', width: 1, y_expand: true});

        // ------------------------------------------------ right column (two graphs)
        this._dotA = new St.Widget({style_class: 'nusa-gdot', reactive: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER});
        this._dotB = new St.Widget({style_class: 'nusa-gdot', reactive: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER});
        this._dotA.connect('button-press-event', () => { this._setGraph(0); return Clutter.EVENT_STOP; });
        this._dotB.connect('button-press-event', () => { this._setGraph(1); return Clutter.EVENT_STOP; });
        const pager = hbox(px(5), {x_align: Clutter.ActorAlign.END});
        pager.add_child(this._dotA);
        pager.add_child(this._dotB);

        this._barsView = this._buildBars();
        this._ringView = this._buildRing();
        const stack = new St.Widget({x_expand: true, y_expand: true, reactive: true, layout_manager: new Clutter.BinLayout()});
        stack.add_child(this._barsView);
        stack.add_child(this._ringView);
        stack.connect('button-press-event', () => {
            this._setGraph(this._graph ? 0 : 1);
            return Clutter.EVENT_STOP;
        });

        const right = vbox(px(2), {x_expand: true, y_expand: true});
        right.add_child(pager);
        right.add_child(stack);

        const root = hbox(px(12), {x_expand: true, y_expand: true});
        root.add_child(left);
        root.add_child(divider);
        root.add_child(right);
        this._main = root;
        this._main.x_expand = this._main.y_expand = true;
        this.actor.add_child(this._main);
        this._settingsView = this._buildSettings();
        this._settingsView.visible = false;
        this.actor.add_child(this._settingsView);
        this._setGraph(0);
    }

    // ------------------------------------------------ settings sub-view
    _buildSettings() {
        const {px, water, settings} = this._ctx;
        const label = t => {
            const l = makeLabel(t, 'nusa-slabel', {ellipsize: false});
            l.width = px(56);
            return l;
        };
        const round = (icon, cb) => {
            const b = new St.Button({style_class: 'nusa-sround', reactive: true, can_focus: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER, child: makeIcon(icon, px(13))});
            b.connect('clicked', () => {
                cb();
                return Clutter.EVENT_STOP;
            });
            return b;
        };
        const value = (w, cls = 'nusa-sval') => {
            const l = makeLabel('', cls, {ellipsize: false});
            l.width = px(w);
            l.clutter_text.x_align = Clutter.ActorAlign.CENTER;
            l.x_align = Clutter.ActorAlign.CENTER;
            return l;
        };

        // Bottle  - 550 ml +   44 ml each · 13/bottle
        this._sBottle = value(68);
        this._sSip = makeLabel('', 'nusa-snote', {ellipsize: false});
        const bottleRow = hbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        bottleRow.add_child(label('Bottle'));
        bottleRow.add_child(round('list-remove-symbolic', () => settings.set_int('water-bottle-ml', Math.max(50, water.bottle - 50))));
        bottleRow.add_child(this._sBottle);
        bottleRow.add_child(round('list-add-symbolic', () => settings.set_int('water-bottle-ml', Math.min(2000, water.bottle + 50))));
        bottleRow.add_child(this._sSip);

        // Goal  [1.5 L] 2.0 L  2.5 L  3.0 L
        this._sGoals = [];
        const goalRow = hbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        goalRow.add_child(label('Goal'));
        for (const ml of [1500, 2000, 2500, 3000]) {
            const b = new St.Button({style_class: 'nusa-schip', reactive: true, can_focus: true, track_hover: true, label: `${(ml / 1000).toFixed(1)} L`, y_align: Clutter.ActorAlign.CENTER});
            b.connect('clicked', () => {
                settings.set_int('water-goal-ml', ml);
                return Clutter.EVENT_STOP;
            });
            goalRow.add_child(b);
            this._sGoals.push([ml, b]);
        }

        // Hours  - 09:00 +  to  - 17:30 +
        this._sFrom = value(48);
        this._sTo = value(48);
        const step = 30;
        const hoursRow = hbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        hoursRow.add_child(label('Hours'));
        hoursRow.add_child(round('list-remove-symbolic', () => settings.set_int('water-start-min', Math.max(0, water.startMin - step))));
        hoursRow.add_child(this._sFrom);
        hoursRow.add_child(round('list-add-symbolic', () => settings.set_int('water-start-min', Math.min(water.endMin - step, water.startMin + step))));
        const to = makeLabel('to', 'nusa-snote', {ellipsize: false});
        to.x_align = Clutter.ActorAlign.CENTER;
        hoursRow.add_child(to);
        hoursRow.add_child(round('list-remove-symbolic', () => settings.set_int('water-end-min', Math.max(water.startMin + step, water.endMin - step))));
        hoursRow.add_child(this._sTo);
        hoursRow.add_child(round('list-add-symbolic', () => settings.set_int('water-end-min', Math.min(1440, water.endMin + step))));

        const left = vbox(px(9), {y_align: Clutter.ActorAlign.CENTER, x_expand: true});
        left.add_child(bottleRow);
        left.add_child(goalRow);
        left.add_child(hoursRow);

        // right: reminders / refills card + Reset
        this._sRem = makeLabel('0', 'nusa-scount', {ellipsize: false});
        this._sRef = makeLabel('0', 'nusa-scount', {ellipsize: false});
        const cardLabel = t => {
            const l = makeLabel(t, 'nusa-scount-l', {ellipsize: false});
            l.x_align = Clutter.ActorAlign.CENTER;
            return l;
        };
        this._sRem.x_align = this._sRef.x_align = Clutter.ActorAlign.CENTER;
        const card = vbox(0, {style_class: 'nusa-scard', x_align: Clutter.ActorAlign.FILL});
        card.add_child(this._sRem);
        card.add_child(cardLabel('reminders'));
        card.add_child(new St.Widget({style_class: 'nusa-sline', height: 1, x_expand: true}));
        card.add_child(this._sRef);
        card.add_child(cardLabel('refills'));

        const reset = new St.Button({style_class: 'nusa-sreset', reactive: true, can_focus: true, track_hover: true, x_align: Clutter.ActorAlign.FILL});
        const rbox = hbox(px(6), {x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        rbox.add_child(makeIcon('user-trash-symbolic', px(13), 'nusa-white'));
        rbox.add_child(makeLabel('Reset', 'nusa-fpill-text', {ellipsize: false}));
        reset.child = rbox;
        reset.connect('clicked', () => {
            water.resetToday(); // clears today's drinks
            return Clutter.EVENT_STOP;
        });

        const right = vbox(px(8), {y_align: Clutter.ActorAlign.CENTER, width: px(120)});
        right.add_child(card);
        right.add_child(reset);

        const root = hbox(px(14), {x_expand: true, y_expand: true});
        root.add_child(left);
        root.add_child(right);
        return root;
    }

    _showSettings(on) {
        if (!this._settingsView || this._settingsOn === on) return;
        this._settingsOn = on;
        this._main.visible = !on;
        this._settingsView.visible = on;
        if (on) {
            this._refreshSettings();
            this._ctx.notch.setSubview('Settings', () => this._showSettings(false));
        } else {
            this._ctx.notch.clearSubview();
        }
    }

    _refreshSettings() {
        const {water} = this._ctx;
        this._sBottle.text = `${water.bottle} ml`;
        this._sSip.text = `${water.sipMl} ml each · ${water.sipsPerBottle}/bottle`;
        for (const [ml, b] of this._sGoals) {
            if (ml === water.goal) b.add_style_class_name('active');
            else b.remove_style_class_name('active');
        }
        this._sFrom.text = fmtHM(water.startMin);
        this._sTo.text = fmtHM(water.endMin);
        this._sRem.text = `${water.reminderCount}`;
        this._sRef.text = `${water.refills}`;
    }

    _buildBars() {
        const {px} = this._ctx;
        this._area = new St.DrawingArea({x_expand: true, y_expand: true});
        this._area.connect('repaint', a => this._paintBars(a));

        this._yl = [makeLabel('400', 'nusa-axis', {ellipsize: false}), makeLabel('200', 'nusa-axis', {ellipsize: false}), makeLabel('0 ml', 'nusa-axis', {ellipsize: false})];
        const yBox = new St.Widget({width: px(YLABEL_W), y_expand: true, layout_manager: new Clutter.BinLayout()});
        [Clutter.ActorAlign.START, Clutter.ActorAlign.CENTER, Clutter.ActorAlign.END].forEach((al, i) => {
            this._yl[i].y_align = al;
            this._yl[i].y_expand = true;
            this._yl[i].x_align = Clutter.ActorAlign.START;
            yBox.add_child(this._yl[i]);
        });

        const chartRow = hbox(px(4), {x_expand: true, y_expand: true});
        chartRow.add_child(this._area);
        chartRow.add_child(yBox);

        this._xRow = new St.Widget({x_expand: true, layout_manager: new Clutter.BoxLayout({homogeneous: true})});
        const xWrap = hbox(px(4), {x_expand: true});
        xWrap.add_child(this._xRow);
        xWrap.add_child(new St.Widget({width: px(YLABEL_W)}));

        const col = vbox(px(2), {x_expand: true, y_expand: true});
        col.add_child(chartRow);
        col.add_child(xWrap);
        return col;
    }

    _buildRing() {
        const {px} = this._ctx;
        const size = px(104);
        this._gauge = new St.DrawingArea({width: size, height: size});
        this._gauge.connect('repaint', a => this._paintGauge(a));
        this._pct = makeLabel('0', 'nusa-gpct', {ellipsize: false});
        const pctRow = hbox(px(2), {x_align: Clutter.ActorAlign.CENTER});
        pctRow.add_child(this._fileIconWidget('drop', px(12), 'nusa-wdrop'));
        pctRow.add_child(makeLabel('%', 'nusa-gpct-s', {ellipsize: false}));
        const center = vbox(0, {x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        center.add_child(this._pct);
        center.add_child(pctRow);
        const box = new St.Widget({width: size, height: size, x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER, layout_manager: new Clutter.BinLayout()});
        box.add_child(this._gauge);
        box.add_child(center);
        return box;
    }

    _setGraph(g) {
        const {px} = this._ctx;
        this._graph = g;
        this._barsView.visible = g === 0;
        this._ringView.visible = g === 1;
        [[this._dotA, g === 0], [this._dotB, g === 1]].forEach(([d, on]) => {
            d.set_size(on ? px(8) : px(5), on ? px(8) : px(5));
            if (on) d.add_style_class_name('active');
            else d.remove_style_class_name('active');
        });
    }

    // ml per hour for today
    _hourly() {
        const {water, settings} = this._ctx;
        const start = Math.floor(water.startMin / 60);
        const end = Math.max(start + 1, Math.ceil(water.endMin / 60));
        const buckets = new Array(end - start).fill(0);
        const day = water._day(todayKey());
        for (const e of day) {
            const h = GLib.DateTime.new_from_unix_local(Math.floor(e.t / 1000)).get_hour();
            const i = Math.max(0, Math.min(buckets.length - 1, h - start));
            buckets[i] += e.ml;
        }
        return {start, buckets};
    }

    _paintBars(area) {
        const [w, h] = area.get_surface_size();
        const {buckets, start} = this._hourly();
        const axisMax = this._axisMax(buckets);
        const n = buckets.length;
        const slot = w / n;
        const bw = Math.max(3, slot * 0.5);
        const nowIdx = GLib.DateTime.new_now_local().get_hour() - start;
        const cr = area.get_context();
        const [R, G, B] = BLUE;

        // grid
        cr.setLineWidth(1);
        cr.setSourceRGBA(1, 1, 1, 0.18);
        cr.setDash([3, 3], 0);
        for (const y of [0.5, h / 2, h - 0.5]) {
            cr.moveTo(0, y);
            cr.lineTo(w, y);
            cr.stroke();
        }
        cr.setDash([], 0);

        for (let i = 0; i < n; i++) {
            const x = i * slot + (slot - bw) / 2;
            cr.setSourceRGBA(1, 1, 1, 0.10);
            roundRect(cr, x, 0, bw, h, bw / 2);
            cr.fill();
            const ml = buckets[i];
            if (ml > 0) {
                const bh = Math.max(bw, h * Math.min(1, ml / axisMax));
                cr.setSourceRGBA(R, G, B, i === nowIdx ? 1 : 0.85);
                roundRect(cr, x, h - bh, bw, bh, bw / 2);
                cr.fill();
            }
        }
        // right axis line
        cr.setSourceRGBA(0.6, 0.7, 1, 0.45);
        cr.moveTo(w - 0.5, 0);
        cr.lineTo(w - 0.5, h);
        cr.stroke();
        cr.$dispose();
    }

    _axisMax(buckets) {
        const m = Math.max(...buckets, 0);
        return Math.max(400, Math.ceil(m / 200) * 200);
    }


    _paintGauge(area) {
        const [w, h] = area.get_surface_size();
        const {water} = this._ctx;
        const frac = Math.max(0, Math.min(1, water.total() / water.goal));
        const lw = Math.max(5, w * 0.075);
        const r = (Math.min(w, h) - lw * 2) / 2;
        const cx = w / 2;
        const cy = h / 2;
        const a0 = 0.75 * Math.PI;            // bottom-left
        const sweep = 1.5 * Math.PI;          // 270 degrees
        const [R, G, B] = BLUE;
        const cr = area.get_context();
        cr.setLineWidth(lw);
        cr.setLineCap(1);
        cr.setSourceRGBA(0.55, 0.65, 0.85, 0.30);
        cr.arc(cx, cy, r, a0, a0 + sweep);
        cr.stroke();
        if (frac > 0.002) {
            cr.setSourceRGBA(R, G, B, 1);
            cr.arc(cx, cy, r, a0, a0 + sweep * frac);
            cr.stroke();
        }
        // knob at the end of the arc (at the start when empty)
        const ang = a0 + sweep * frac;
        const kx = cx + r * Math.cos(ang);
        const ky = cy + r * Math.sin(ang);
        cr.setSourceRGBA(R, G, B, 1);
        cr.arc(kx, ky, lw * 0.95, 0, 2 * Math.PI);
        cr.fill();
        cr.setSourceRGBA(0.85, 0.93, 1, 1);
        cr.arc(kx, ky, lw * 0.45, 0, 2 * Math.PI);
        cr.fill();
        cr.$dispose();
    }

    _paintBarProgress(area) {
        const [w, h] = area.get_surface_size();
        const {water} = this._ctx;
        const frac = Math.max(0, Math.min(1, water.total() / water.goal));
        const th = Math.max(3, h * 0.4);
        const y = (h - th) / 2;
        const cr = area.get_context();
        cr.setSourceRGBA(1, 1, 1, 0.16);
        roundRect(cr, 0, y, w, th, th / 2);
        cr.fill();
        const r = h / 2;
        const fw = Math.max(0, (w - h) * frac);
        if (fw > 0) {
            cr.setSourceRGBA(...BLUE, 1);
            roundRect(cr, 0, y, fw + r, th, th / 2);
            cr.fill();
        }
        cr.setSourceRGBA(0.8, 0.9, 1, 1);
        cr.arc(r + fw, h / 2, r * 0.72, 0, 2 * Math.PI);
        cr.fill();
        cr.$dispose();
    }

    refresh() {
        const {px, water, settings} = this._ctx;
        const total = water.total();
        const goal = water.goal;
        const pct = Math.round((total / goal) * 100);
        const on = settings.get_boolean('water-reminders');

        this._amount.text = (total / 1000).toFixed(1);
        this._goal.text = `/ ${(goal / 1000).toFixed(1)} L`;
        this._sub.text = `Today · ${pct}% · Reminders ${on ? 'on' : 'off'}`;
        this._pct.text = `${pct}`;

        if (on) this._switch.add_style_class_name('on');
        else this._switch.remove_style_class_name('on');
        this._knob.x_align = on ? Clutter.ActorAlign.END : Clutter.ActorAlign.START;

        const {buckets, start} = this._hourly();
        const max = this._axisMax(buckets);
        this._yl[0].text = `${max}`;
        this._yl[1].text = `${max / 2}`;
        this._xRow.destroy_all_children();
        buckets.forEach((_b, i) => {
            const hr = start + i;
            const show = i % 3 === 0;
            const l = makeLabel(show ? String(hr).padStart(2, '0') : '', 'nusa-axis', {ellipsize: false});
            l.x_align = Clutter.ActorAlign.CENTER;
            this._xRow.add_child(l);
        });

        this._area.queue_repaint();
        this._gauge.queue_repaint();
        this._bar.queue_repaint();
        if (this._settingsOn) this._refreshSettings();
    }

    onShow() { this.refresh(); }
    onHide() { this._showSettings(false); }

    destroy() {
        this._unsub();
        this._ctx.settings.disconnect(this._setSig);
        this.actor.destroy();
    }
}

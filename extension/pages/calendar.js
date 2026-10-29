import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';

import {dateKey, fmtRange, parseMinutes, todayKey} from '../lib/util.js';
import {hbox, iconButton, makeLabel, vbox} from '../lib/ui.js';

const DOW = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];

// Left: month / weekday / big day number of the selected date.
// Middle: month grid (today in blue). Right: reminders for the selected date + add form.
export class CalendarPage {
    constructor(ctx) {
        this.id = 'calendar';
        this.icon = 'x-office-calendar-symbolic';
        this.name = 'Calendar';
        this.title = 'Calendar';
        this._ctx = ctx;
        const now = GLib.DateTime.new_now_local();
        this._year = now.get_year();
        this._month = now.get_month();
        this._sel = todayKey();
        this._form = false;
        this._build();
        this._unsub = ctx.cal.changed.connect(() => this.refresh());
        this.refresh();
    }

    _btn(cls, label, cb) {
        const b = new St.Button({style_class: cls, label, reactive: true, can_focus: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER});
        b.connect('clicked', () => {
            cb();
            return Clutter.EVENT_STOP;
        });
        return b;
    }

    _build() {
        const {px} = this._ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});

        // ---- left: selected date
        this._lMonth = makeLabel('', 'nusa-cmonth', {ellipsize: false});
        this._lWeekday = makeLabel('', 'nusa-cweekday', {ellipsize: false});
        this._lDay = makeLabel('', 'nusa-cbig', {ellipsize: false});
        const left = vbox(0, {y_align: Clutter.ActorAlign.CENTER, width: px(86)});
        left.add_child(this._lMonth);
        left.add_child(this._lWeekday);
        left.add_child(this._lDay);

        // ---- middle: month grid
        this._navLabel = makeLabel('', 'nusa-cnav', {ellipsize: false, xExpand: true});
        this._navLabel.x_align = Clutter.ActorAlign.CENTER;
        const nav = hbox(px(2), {x_expand: true});
        nav.add_child(iconButton(this._ctx, 'go-previous-symbolic', () => this._shift(-1), {size: 10, cls: 'nusa-cstep'}));
        nav.add_child(this._navLabel);
        nav.add_child(iconButton(this._ctx, 'go-next-symbolic', () => this._shift(1), {size: 10, cls: 'nusa-cstep'}));
        this._grid = vbox(px(1));
        const mid = vbox(px(2), {y_align: Clutter.ActorAlign.CENTER, width: px(170)});
        mid.add_child(nav);
        mid.add_child(this._grid);

        // ---- right: reminders
        this._rHead = makeLabel('Reminders', 'nusa-tcap', {ellipsize: false, xExpand: true});
        this._addBtn = this._btn('nusa-cadd', '+ Add', () => this._openForm());
        const head = hbox(px(4), {x_expand: true});
        head.add_child(this._rHead);
        head.add_child(this._addBtn);
        this._list = vbox(px(1), {x_expand: true});
        this._scroll = new St.ScrollView({
            x_expand: true, y_expand: true, hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC, overlay_scrollbars: true,
        });
        this._scroll.add_child(this._list);

        this._title = new St.Entry({style_class: 'nusa-tentry', hint_text: 'Reminder', x_expand: true, can_focus: true});
        this._title.set_primary_icon(new St.Icon({icon_name: 'media-record-symbolic', icon_size: px(10)}));
        this._time = new St.Entry({style_class: 'nusa-ttime', hint_text: '--:--', x_expand: true, can_focus: true});
        this._time.set_primary_icon(new St.Icon({icon_name: 'alarm-symbolic', icon_size: px(13)}));
        for (const e of [this._title, this._time]) {
            e.clutter_text.connect('key-focus-in', () => this._ctx.notch.beginModal());
            e.clutter_text.connect('activate', () => this._save());
            e.clutter_text.connect('text-changed', () => e.remove_style_pseudo_class('error'));
        }
        const cancel = this._btn('nusa-tcancel', 'Cancel', () => this._closeForm());
        const save = this._btn('nusa-tsave', 'Save', () => this._save());
        cancel.x_expand = save.x_expand = true;
        const actions = hbox(px(6), {x_expand: true});
        actions.add_child(cancel);
        actions.add_child(save);
        this._formView = vbox(px(5), {x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this._formView.add_child(this._title);
        this._formView.add_child(this._time);
        this._formView.add_child(actions);
        this._formView.visible = false;

        const body = new St.Widget({x_expand: true, y_expand: true, layout_manager: new Clutter.BinLayout()});
        body.add_child(this._scroll);
        body.add_child(this._formView);
        const right = vbox(px(4), {x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.FILL});
        right.add_child(head);
        right.add_child(body);

        const root = hbox(px(12), {x_expand: true, y_expand: true});
        root.add_child(left);
        root.add_child(mid);
        root.add_child(right);
        this.actor.set_child(root);
    }

    // ------------------------------------------------ month grid
    _shift(delta) {
        let m = this._month + delta;
        let y = this._year;
        if (m < 1) { m = 12; y -= 1; }
        if (m > 12) { m = 1; y += 1; }
        this._month = m;
        this._year = y;
        this._drawGrid();
    }

    _pick(key) {
        this._sel = key;
        const [y, m] = key.split('-').map(Number);
        this._year = y;
        this._month = m;
        if (this._form) this._closeForm();
        this.refresh();
    }

    _drawGrid() {
        const {px, cal} = this._ctx;
        const today = todayKey();
        const first = GLib.DateTime.new_local(this._year, this._month, 1, 0, 0, 0);
        this._navLabel.text = first.format('%B %Y');
        // Sunday-first grid starting on the Sunday on or before the 1st
        const start = first.add_days(-(first.get_day_of_week() % 7));

        this._grid.destroy_all_children();
        const dow = hbox(0);
        for (const d of DOW) {
            const l = makeLabel(d, 'nusa-cdow', {ellipsize: false});
            l.width = px(24);
            l.x_align = Clutter.ActorAlign.CENTER;
            l.clutter_text.x_align = Clutter.ActorAlign.CENTER;
            dow.add_child(l);
        }
        this._grid.add_child(dow);

        for (let w = 0; w < 6; w++) {
            const row = hbox(0);
            for (let d = 0; d < 7; d++) {
                const day = start.add_days(w * 7 + d);
                const key = dateKey(day.get_year(), day.get_month(), day.get_day_of_month());
                const cls = ['nusa-cday'];
                if (day.get_month() !== this._month) cls.push('other');
                if (key === today) cls.push('today');
                else if (key === this._sel) cls.push('sel');
                if (cal.on(key).length) cls.push('has');
                const b = new St.Button({style_class: cls.join(' '), label: String(day.get_day_of_month()), reactive: true, can_focus: true, track_hover: true});
                b.set_size(px(24), px(16));
                b.connect('clicked', () => {
                    this._pick(key);
                    return Clutter.EVENT_STOP;
                });
                row.add_child(b);
            }
            this._grid.add_child(row);
        }
    }

    // ------------------------------------------------ reminders
    _selDate() {
        const [y, m, d] = this._sel.split('-').map(Number);
        return GLib.DateTime.new_local(y, m, d, 0, 0, 0);
    }

    _openForm() {
        this._form = true;
        this._title.set_text('');
        this._time.set_text('');
        this._title.remove_style_pseudo_class('error');
        this._time.remove_style_pseudo_class('error');
        this._syncView();
        this._title.grab_key_focus();
    }

    _closeForm() {
        this._form = false;
        this._ctx.notch.endModal();
        this._syncView();
    }

    _save() {
        const title = this._title.get_text().trim();
        if (!title) {
            this._title.add_style_pseudo_class('error');
            return;
        }
        const text = this._time.get_text().trim();
        let min = null;
        if (text) {
            min = parseMinutes(text);
            if (min === null) {
                this._time.add_style_pseudo_class('error');
                return;
            }
        }
        this._ctx.cal.add(this._sel, title, min);
        this._closeForm();
    }

    _syncView() {
        this._formView.visible = this._form;
        this._scroll.visible = !this._form;
        this._addBtn.visible = !this._form;
    }

    refresh() {
        const {px, cal} = this._ctx;
        const d = this._selDate();
        this._lMonth.text = d.format('%B').toUpperCase();
        this._lWeekday.text = d.format('%A');
        this._lDay.text = String(d.get_day_of_month());
        this._rHead.text = this._sel === todayKey() ? 'Today' : d.format('%a %-d %b');
        this._drawGrid();
        this._syncView();

        this._list.destroy_all_children();
        const items = cal.on(this._sel);
        if (!items.length) {
            const hint = makeLabel('No reminders', 'nusa-dim', {ellipsize: false});
            hint.x_align = Clutter.ActorAlign.CENTER;
            hint.y_expand = true;
            hint.y_align = Clutter.ActorAlign.CENTER;
            this._list.add_child(hint);
            return;
        }
        for (const r of items) {
            const row = hbox(px(6), {x_expand: true, style_class: 'nusa-trow', reactive: true, track_hover: true});
            const bar = new St.Widget({style_class: 'nusa-cbar', y_align: Clutter.ActorAlign.FILL});
            bar.set_width(px(3));
            row.add_child(bar);
            const info = vbox(0, {x_expand: true, y_align: Clutter.ActorAlign.CENTER});
            info.add_child(makeLabel(r.title, ''));
            info.add_child(makeLabel(r.startMin === null ? 'All day' : fmtRange(r.startMin, null), 'nusa-ttime-dim', {ellipsize: false}));
            row.add_child(info);
            row.add_child(iconButton(this._ctx, 'window-close-symbolic', () => cal.remove(r.id), {size: 11}));
            this._list.add_child(row);
        }
    }

    onShow() {
        // jump back to today when the notch is opened
        const now = GLib.DateTime.new_now_local();
        this._year = now.get_year();
        this._month = now.get_month();
        this._sel = todayKey();
        this.refresh();
    }

    onHide() {
        if (this._form) this._closeForm();
    }

    destroy() {
        this._unsub();
        this.actor.destroy();
    }
}

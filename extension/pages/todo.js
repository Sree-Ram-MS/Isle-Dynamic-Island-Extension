import Clutter from 'gi://Clutter';
import St from 'gi://St';

import {clockLabel, parseClock} from '../lib/util.js';
import {hbox, iconButton, makeLabel, pill, vbox} from '../lib/ui.js';

export class TodoPage {
    constructor(ctx) {
        this.id = 'todo';
        this.icon = 'object-select-symbolic';
        this.name = 'To-do';
        this._ctx = ctx;
        const {px} = ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});

        this._list = vbox(px(2), {x_expand: true});
        this._scroll = new St.ScrollView({
            x_expand: true, y_expand: true, hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC, overlay_scrollbars: true,
        });
        this._scroll.add_child(this._list);

        this._title = new St.Entry({style_class: 'nusa-entry', hint_text: 'New task…', x_expand: true, can_focus: true});
        this._time = new St.Entry({style_class: 'nusa-entry', hint_text: 'HH:MM', width: px(70), can_focus: true});
        for (const e of [this._title, this._time]) {
            e.clutter_text.connect('key-focus-in', () => ctx.notch.beginModal());
            e.clutter_text.connect('activate', () => this._add());
        }

        const form = hbox(px(6), {x_expand: true});
        form.add_child(this._title);
        form.add_child(this._time);
        form.add_child(pill('Add', () => this._add(), true));

        const root = vbox(px(6), {x_expand: true, y_expand: true});
        root.add_child(this._scroll);
        root.add_child(form);
        this.actor.set_child(root);

        this._unsub = ctx.todo.changed.connect(() => this.refresh());
        this.refresh();
    }

    _add() {
        const title = this._title.get_text().trim();
        if (!title) return;
        const timeText = this._time.get_text().trim();
        const due = timeText ? parseClock(timeText) : null;
        if (timeText && due === null) {
            this._time.add_style_pseudo_class('error');
            this._ctx.notch.peek('dialog-warning-symbolic', 'Use 24h time like 14:30', 2200);
            return;
        }
        this._ctx.todo.add(title, due);
        this._title.set_text('');
        this._time.set_text('');
    }

    refresh() {
        const {px, todo, is24h} = this._ctx;
        this._list.destroy_all_children();
        const tasks = [...todo.tasks].sort((a, b) => (a.done - b.done) || ((a.due ?? Infinity) - (b.due ?? Infinity)));
        if (!tasks.length) {
            const hint = makeLabel('No tasks yet — add one below', 'nusa-dim', {ellipsize: false});
            hint.x_align = Clutter.ActorAlign.CENTER;
            hint.y_expand = true;
            this._list.add_child(hint);
            return;
        }
        for (const t of tasks) {
            const row = hbox(px(8), {x_expand: true, style_class: 'nusa-task', reactive: true, track_hover: true});
            const dot = new St.Button({style_class: 'nusa-dot', reactive: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER});
            dot.set_style(`background-color: ${t.done ? 'transparent' : t.color}; border: 2px solid ${t.color};`);
            dot.connect('clicked', () => {
                todo.toggle(t.id);
                return Clutter.EVENT_STOP;
            });
            const title = makeLabel(t.title, t.done ? 'nusa-dim' : '', {xExpand: true});
            if (t.done) title.set_style('text-decoration: line-through;');
            row.add_child(dot);
            row.add_child(title);
            if (t.due) row.add_child(makeLabel(clockLabel(t.due, is24h()), 'nusa-dim', {ellipsize: false}));
            row.add_child(iconButton(this._ctx, 'window-close-symbolic', () => todo.remove(t.id), {size: 12}));
            this._list.add_child(row);
        }
    }

    onShow() { this.refresh(); }
    onHide() {}

    destroy() {
        this._unsub();
        this.actor.destroy();
    }
}

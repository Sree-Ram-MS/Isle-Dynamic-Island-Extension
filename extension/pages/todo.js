import Clutter from 'gi://Clutter';
import St from 'gi://St';

import {fmtHM, fmtRange, parseRange} from '../lib/util.js';
import {hbox, makeIcon, makeLabel, vbox} from '../lib/ui.js';

// Left: "0/0 Task Done Today" + Add Task / Edit Task.
// Right: the task list, or the add/edit form.
export class TodoPage {
    constructor(ctx) {
        this.id = 'todo';
        this.icon = 'object-select-symbolic';
        this.name = 'To-do';
        this.titleMarkup = 'Todo<span foreground="#30d158">List</span>';
        this._ctx = ctx;
        this._form = false;
        this._edit = false;
        this._editId = null;
        this._repeat = 'daily';
        this._build();
        this._unsub = ctx.todo.changed.connect(() => this.refresh());
        this.refresh();
    }

    _button(cls, child, cb) {
        const b = new St.Button({style_class: cls, reactive: true, can_focus: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER});
        if (child) b.child = child;
        b.connect('clicked', () => {
            cb();
            return Clutter.EVENT_STOP;
        });
        return b;
    }

    _build() {
        const {px} = this._ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});

        // ------------------------------------------------ left column
        this._count = makeLabel('0/0', 'nusa-tcount', {ellipsize: false});
        const cal = new St.Widget({layout_manager: new Clutter.BinLayout(), y_align: Clutter.ActorAlign.START});
        const calIcon = makeIcon('x-office-calendar-symbolic', px(16), 'nusa-dim');
        calIcon.x_align = Clutter.ActorAlign.CENTER;
        this._badge = new St.Widget({style_class: 'nusa-tbadge', x_align: Clutter.ActorAlign.END, y_align: Clutter.ActorAlign.START});
        this._badge.set_size(px(7), px(7));
        cal.add_child(calIcon);
        cal.add_child(this._badge);
        const countRow = hbox(px(10), {y_align: Clutter.ActorAlign.CENTER});
        countRow.add_child(this._count);
        countRow.add_child(cal);

        const caption = makeLabel('Task Done Today', 'nusa-tcap', {ellipsize: false});

        const addBox = hbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        addBox.add_child(makeIcon('list-add-symbolic', px(13), 'nusa-tplus'));
        addBox.add_child(makeLabel('Add Task', 'nusa-fpill-text', {ellipsize: false}));
        this._addBtn = this._button('nusa-tadd', addBox, () => this._openForm(null));
        const editBox = hbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        editBox.add_child(makeIcon('document-edit-symbolic', px(13)));
        editBox.add_child(makeLabel('Edit Task', 'nusa-fpill-text', {ellipsize: false}));
        this._editBtn = this._button('nusa-tedit', editBox, () => this._toggleEdit());
        const btns = hbox(px(6));
        btns.add_child(this._addBtn);
        btns.add_child(this._editBtn);

        const left = vbox(px(3), {y_align: Clutter.ActorAlign.CENTER});
        left.add_child(countRow);
        left.add_child(caption);
        left.add_child(new St.Widget({height: px(6)}));
        left.add_child(btns);

        // ------------------------------------------------ right: list
        this._list = vbox(px(2), {x_expand: true});
        this._scroll = new St.ScrollView({
            x_expand: true, y_expand: true, hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC, overlay_scrollbars: true,
        });
        this._scroll.add_child(this._list);

        // ------------------------------------------------ right: form
        this._title = new St.Entry({style_class: 'nusa-tentry', hint_text: 'Task name', x_expand: true, can_focus: true});
        this._title.set_primary_icon(new St.Icon({icon_name: 'media-record-symbolic', icon_size: px(10)}));
        const clear = new St.Icon({icon_name: 'edit-clear-symbolic', icon_size: px(13)});
        this._title.set_secondary_icon(clear);
        this._title.connect('secondary-icon-clicked', () => this._title.set_text(''));

        this._time = new St.Entry({style_class: 'nusa-ttime', hint_text: '--:-- - --:--', x_expand: true, can_focus: true});
        this._time.set_primary_icon(new St.Icon({icon_name: 'alarm-symbolic', icon_size: px(14)}));
        this._chip = this._button('nusa-tchip', null, () => {
            this._repeat = this._repeat === 'daily' ? 'once' : 'daily';
            this._chip.label = this._repeat === 'daily' ? 'Daily' : 'Once';
        });
        this._chip.label = 'Daily';

        for (const e of [this._title, this._time]) {
            e.clutter_text.connect('key-focus-in', () => this._ctx.notch.beginModal());
            e.clutter_text.connect('activate', () => this._save());
            e.clutter_text.connect('text-changed', () => e.remove_style_pseudo_class('error'));
        }

        const timeRow = hbox(px(4), {x_expand: true});
        timeRow.add_child(this._time);
        timeRow.add_child(this._chip);

        const cancel = this._button('nusa-tcancel', null, () => this._closeForm());
        cancel.label = 'Cancel';
        cancel.x_expand = true;
        const save = this._button('nusa-tsave', null, () => this._save());
        save.label = 'Save';
        save.x_expand = true;
        const actions = hbox(px(8), {x_expand: true});
        actions.add_child(cancel);
        actions.add_child(save);

        this._formView = vbox(px(6), {x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this._formView.add_child(this._title);
        this._formView.add_child(timeRow);
        this._formView.add_child(actions);
        this._formView.visible = false;

        const right = new St.Widget({x_expand: true, y_expand: true, layout_manager: new Clutter.BinLayout()});
        right.add_child(this._scroll);
        right.add_child(this._formView);

        const root = hbox(px(16), {x_expand: true, y_expand: true});
        root.add_child(left);
        root.add_child(right);
        this.actor.set_child(root);
    }

    // ------------------------------------------------ form
    _openForm(task) {
        const {todo} = this._ctx;
        this._form = true;
        this._editId = task?.id ?? null;
        this._title.set_text(task?.title ?? '');
        const start = task ? todo.startOf(task) : null;
        this._time.set_text(start === null ? '' : (task.endMin !== null && task.endMin !== undefined
            ? `${fmtHM(start)} - ${fmtHM(task.endMin)}` : fmtHM(start)));
        this._repeat = task?.repeat ?? 'daily';
        this._chip.label = this._repeat === 'daily' ? 'Daily' : 'Once';
        this._title.remove_style_pseudo_class('error');
        this._time.remove_style_pseudo_class('error');
        this._syncView();
        this._title.grab_key_focus();
    }

    _closeForm() {
        this._form = false;
        this._editId = null;
        this._ctx.notch.endModal();
        this._syncView();
    }

    _save() {
        const {todo} = this._ctx;
        const title = this._title.get_text().trim();
        if (!title) {
            this._title.add_style_pseudo_class('error');
            return;
        }
        const timeText = this._time.get_text().trim();
        let opts = {repeat: this._repeat, startMin: null, endMin: null};
        if (timeText) {
            const r = parseRange(timeText);
            if (!r) {
                this._time.add_style_pseudo_class('error');
                return;
            }
            opts = {repeat: this._repeat, startMin: r.start, endMin: r.end};
        }
        if (this._editId !== null) todo.update(this._editId, title, opts);
        else todo.add(title, opts);
        this._closeForm();
    }

    _toggleEdit() {
        if (this._form) this._closeForm();
        this._edit = !this._edit;
        this.refresh();
    }

    _syncView() {
        this._formView.visible = this._form;
        this._scroll.visible = !this._form;
        if (this._form) this._addBtn.add_style_class_name('active');
        else this._addBtn.remove_style_class_name('active');
        if (this._edit) this._editBtn.add_style_class_name('active');
        else this._editBtn.remove_style_class_name('active');
    }

    // ------------------------------------------------ list
    refresh() {
        const {px, todo} = this._ctx;
        const {done, total} = todo.stats();
        this._count.text = `${done}/${total}`;
        this._syncView();

        const tasks = todo.visible().sort((a, b) =>
            (todo.isDone(a) - todo.isDone(b)) || ((todo.startOf(a) ?? 1e9) - (todo.startOf(b) ?? 1e9)));
        this._badge.visible = tasks.some(t => !todo.isDone(t) && todo.startOf(t) !== null);

        this._list.destroy_all_children();
        if (!tasks.length) {
            const hint = makeLabel('No tasks yet. Tap Add Task', 'nusa-dim', {ellipsize: false});
            hint.x_align = Clutter.ActorAlign.CENTER;
            hint.y_align = Clutter.ActorAlign.CENTER;
            hint.y_expand = true;
            this._list.add_child(hint);
            return;
        }
        for (const t of tasks) {
            const isDone = todo.isDone(t);
            const row = hbox(px(8), {x_expand: true, style_class: 'nusa-trow', reactive: true, track_hover: true});
            const dot = new St.Button({style_class: 'nusa-dot', reactive: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER});
            dot.set_style(`background-color: ${isDone ? 'transparent' : t.color}; border: 2px solid ${t.color};`);
            dot.connect('clicked', () => {
                todo.toggle(t.id);
                return Clutter.EVENT_STOP;
            });
            const title = makeLabel(t.title, isDone ? 'nusa-dim' : '', {xExpand: true});
            if (isDone) title.set_style('text-decoration: line-through;');
            row.add_child(dot);
            row.add_child(title);
            const start = todo.startOf(t);
            if (start !== null)
                row.add_child(makeLabel(fmtRange(start, t.endMin ?? null), 'nusa-ttime-dim', {ellipsize: false}));
            if (this._edit) {
                const btn = (icon, cb) => this._button('nusa-ctl', makeIcon(icon, px(12)), cb);
                row.add_child(btn('document-edit-symbolic', () => this._openForm(t)));
                row.add_child(btn('window-close-symbolic', () => todo.remove(t.id)));
            }
            this._list.add_child(row);
        }
    }

    onShow() { this.refresh(); }

    onHide() {
        if (this._form) this._closeForm();
        if (this._edit) {
            this._edit = false;
            this.refresh();
        }
    }

    destroy() {
        this._unsub();
        this.actor.destroy();
    }
}

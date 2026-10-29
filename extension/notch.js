import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {Store} from './lib/store.js';
import {MprisWatcher, destroyArtSession} from './lib/mpris.js';
import {LyricsClient} from './lib/lyrics.js';
import {FocusService, IdleWatcher, SystemWatcher, TodoService, WaterService, playChime} from './lib/services.js';
import {Timers, fmtTime} from './lib/util.js';
import {hbox, iconButton, makeIcon, makeLabel, pill, vbox} from './lib/ui.js';

import {MusicPage} from './pages/music.js';
import {LyricsPage} from './pages/lyrics.js';
import {FocusPage} from './pages/focus.js';
import {WaterPage} from './pages/water.js';
import {ShelfPage} from './pages/shelf.js';
import {TodoPage} from './pages/todo.js';
import {ChargingPage, HeadphonesPage} from './pages/power.js';

const BASE_W = 190;
const BASE_H = 32;
const OPEN_W = 480;
const OPEN_H = 200;

export class Notch {
    constructor(ext) {
        this._ext = ext;
        this._state = 'collapsed';
        this._pageIndex = 0;
        this._pinned = false;
        this._grab = null;
        this._capSig = 0;
        this._peek = null;
        this._mascot = 'awake';
        this._notice = null;
        this._scrollAcc = 0;
        this._scrollAt = 0;
        this._signals = [];
    }

    // ------------------------------------------------------------ lifecycle
    enable() {
        this._settings = this._ext.getSettings();
        this._iface = new Gio.Settings({schema_id: 'org.gnome.desktop.interface'});
        this._timers = new Timers();
        this._store = new Store();
        this._mpris = new MprisWatcher();
        this._lyrics = new LyricsClient();
        this._focus = new FocusService(this._settings, this._timers);
        this._water = new WaterService(this._settings, this._store, this._timers);
        this._todo = new TodoService(this._store, this._timers);
        this._sys = new SystemWatcher();

        this._ctx = {
            settings: this._settings, store: this._store, timers: this._timers,
            mpris: this._mpris, lyrics: this._lyrics, focus: this._focus,
            water: this._water, todo: this._todo, sys: this._sys, notch: this,
            is24h: () => this._iface.get_string('clock-format') === '24h',
            px: n => Math.round(n * this._scale()),
        };

        this._mpris.changed.connect(() => this._onMedia());
        this._focus.changed.connect(() => this._updateCollapsed());
        this._focus.onFinished = (wasFocus) => this._notify({
            icon: 'alarm-symbolic',
            title: wasFocus ? 'Focus session complete' : 'Break is over',
            sub: wasFocus ? 'Time for a break' : 'Ready for the next session?',
            actions: [
                {label: wasFocus ? 'Start break' : 'Start focus', accent: true, cb: () => this._focus.start()},
                {label: 'Dismiss', cb: () => {}},
            ],
        });
        this._water.onReminder = () => {
            this.peek('weather-showers-symbolic', 'Time to drink water', 5000);
            playChime();
        };
        this._todo.onDue = (t) => this._notify({
            icon: 'alarm-symbolic', title: "Time's up!", sub: t.title,
            actions: [
                {label: 'Done', accent: true, cb: () => this._todo.toggle(t.id)},
                {label: 'Snooze 5 min', cb: () => this._todo.snooze(t.id, 5)},
            ],
        });
        this._sys.onPeek = (icon, text, ms) => {
            if (this._settings.get_boolean('status-peeks')) this.peek(icon, text, ms);
        };

        this._connect(this._settings, 'changed', (_s, key) => this._onSetting(key));
        this._connect(this._iface, 'changed::clock-format', () => this._updateCollapsed());
        this._connect(Main.layoutManager, 'monitors-changed', () => this._reposition(false));

        this._buildUi();
        this._mpris.start();
        this._sys.start();
        this._idle = new IdleWatcher(state => this._setMascot(state));

        this._timers.add(10 * 1000, () => {
            this._updateCollapsed();
            return true;
        });
    }

    destroy() {
        this._endModal();
        this._idle?.destroy();
        this._idle = null;
        this._timers?.destroy();
        for (const [obj, id] of this._signals) obj.disconnect(id);
        this._signals = [];
        this._teardownUi();
        this._focus?.destroy();
        this._water?.destroy();
        this._sys?.destroy();
        this._mpris?.destroy();
        this._lyrics?.destroy();
        destroyArtSession();
        this._store?.destroy();
        this._ctx = null;
    }

    _connect(obj, name, cb) {
        this._signals.push([obj, obj.connect(name, cb)]);
    }

    _scale() {
        return this._settings.get_double('notch-scale');
    }

    _minimal() {
        return this._settings.get_boolean('minimal-notch');
    }

    // ------------------------------------------------------------ UI
    _buildUi() {
        const px = this._ctx.px;

        this.actor = new St.Widget({
            style_class: `nusa-notch${this._minimal() ? ' minimal' : ''}`,
            reactive: true, track_hover: true, can_focus: true, clip_to_allocation: true,
            layout_manager: new Clutter.BinLayout(),
        });
        this.actor.set_style(`font-size: ${Math.round(13 * this._scale())}px;`);

        // collapsed content: [art/play icon] [clock | timer | peek | eyes] [equalizer]
        this._collapsedView = hbox(px(8), {x_expand: true, y_expand: true, style_class: 'nusa-collapsed'});
        this._leftIcon = makeIcon('audio-x-generic-symbolic', px(14), 'nusa-dim');
        this._leftIcon.visible = false;
        this._centerLabel = makeLabel('', 'nusa-clock', {ellipsize: false});
        this._centerBox = hbox(px(6), {x_expand: true, x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        this._peekIcon = makeIcon('dialog-information-symbolic', px(14));
        this._peekIcon.visible = false;
        this._eyes = hbox(px(10), {y_align: Clutter.ActorAlign.CENTER});
        this._eyeL = new St.Widget({style_class: 'nusa-eye'});
        this._eyeR = new St.Widget({style_class: 'nusa-eye'});
        this._eyes.add_child(this._eyeL);
        this._eyes.add_child(this._eyeR);
        this._eyes.visible = false;
        this._centerBox.add_child(this._peekIcon);
        this._centerBox.add_child(this._centerLabel);
        this._centerBox.add_child(this._eyes);
        this._eq = hbox(px(2), {y_align: Clutter.ActorAlign.CENTER});
        for (const h of [6, 11, 8]) {
            const bar = new St.Widget({style_class: 'nusa-eq', height: px(h), y_align: Clutter.ActorAlign.CENTER});
            this._eq.add_child(bar);
        }
        this._eq.visible = false;
        this._collapsedView.add_child(this._leftIcon);
        this._collapsedView.add_child(this._centerBox);
        this._collapsedView.add_child(this._eq);

        // expanded content
        this._expandedView = vbox(0, {x_expand: true, y_expand: true, style_class: 'nusa-expanded', visible: false, opacity: 0});
        this._pageHolder = new St.Bin({x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.FILL, y_align: Clutter.ActorAlign.FILL});
        this._bar = hbox(px(2), {x_align: Clutter.ActorAlign.CENTER, style_class: 'nusa-bar'});
        this._expandedView.add_child(this._pageHolder);
        this._expandedView.add_child(this._bar);

        this.actor.add_child(this._collapsedView);
        this.actor.add_child(this._expandedView);

        this._pages = [
            new MusicPage(this._ctx), new LyricsPage(this._ctx), new FocusPage(this._ctx),
            new WaterPage(this._ctx), new ShelfPage(this._ctx), new TodoPage(this._ctx),
            new HeadphonesPage(this._ctx), new ChargingPage(this._ctx),
        ];
        this._barButtons = this._pages.map((p, i) => {
            const b = iconButton(this._ctx, p.icon, () => this._goto(i), {size: 15, cls: 'nusa-bar-btn'});
            this._bar.add_child(b);
            return b;
        });
        this._bar.add_child(iconButton(this._ctx, 'emblem-system-symbolic', () => {
            this._collapse(true);
            this._ext.openPreferences();
        }, {size: 15, cls: 'nusa-bar-btn'}));

        this._uiSignals = [
            this.actor.connect('notify::hover', () => this._onHover()),
            this.actor.connect('scroll-event', (_a, ev) => this._onScroll(ev)),
            this.actor.connect('key-press-event', (_a, ev) => this._onKey(ev)),
            this.actor.connect('button-press-event', () => {
                if (this._state === 'expanded' && !this._grab) this.actor.grab_key_focus();
                return Clutter.EVENT_PROPAGATE;
            }),
        ];

        Main.layoutManager.addChrome(this.actor, {
            trackFullscreen: this._settings.get_boolean('hide-in-fullscreen'),
        });

        this._state = 'collapsed';
        this._setPage(this._pageIndex, false);
        this._applyCollapsed(false);
        this._updateCollapsed();
        this._setMascot(this._mascot);
    }

    _teardownUi() {
        if (!this.actor) return;
        this._timers?.clear(this._hoverTimer);
        this._timers?.clear(this._collapseTimer);
        this._timers?.clear(this._peekTimer);
        for (const id of this._uiSignals ?? []) this.actor.disconnect(id);
        for (const p of this._pages ?? []) p.destroy();
        this._pages = null;
        Main.layoutManager.removeChrome(this.actor);
        this.actor.destroy();
        this.actor = null;
    }

    _rebuildUi() {
        const keep = this._pageIndex;
        this._endModal();
        this._teardownUi();
        this._pageIndex = keep;
        this._buildUi();
    }

    _onSetting(key) {
        switch (key) {
        case 'notch-scale':
        case 'minimal-notch':
        case 'hide-in-fullscreen':
            this._rebuildUi();
            break;
        case 'mascot':
            this._setMascot(this._mascot);
            break;
        case 'focus-minutes':
        case 'break-minutes':
            this._focus.refreshLengths();
            break;
        case 'water-goal-ml':
        case 'water-bottle-ml':
        case 'water-reminders':
            this._water.changed.emit();
            break;
        case 'lyrics-enabled':
            this._pages?.[1]?.refresh();
            break;
        default:
            break;
        }
    }

    // ------------------------------------------------------------ sizes & position
    _collapsedSize() {
        const s = this._scale();
        if (this._minimal() && !this._peek && !this._notice)
            return [Math.round(140 * s), Math.max(3, Math.round(4 * s))];
        let w = BASE_W;
        if (this._peek) w = Math.min(430, Math.max(BASE_W, 76 + this._peek.text.length * 8.6));
        else if (this._leftIcon.visible || this._eq.visible) w = BASE_W + 30;
        return [Math.round(w * s), Math.round(BASE_H * s)];
    }

    _expandedSize() {
        const s = this._scale();
        return [Math.round(OPEN_W * s), Math.round(OPEN_H * s)];
    }

    _targetX(w) {
        const m = Main.layoutManager.primaryMonitor;
        return m.x + Math.round((m.width - w) / 2);
    }

    _reposition(animate) {
        const m = Main.layoutManager.primaryMonitor;
        if (!m || !this.actor) return;
        const [w, h] = this._state === 'expanded' ? this._expandedSize() : this._collapsedSize();
        this.actor.remove_all_transitions();
        if (animate) {
            this.actor.ease({
                x: this._targetX(w), y: m.y, width: w, height: h,
                duration: 220, mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            });
        } else {
            this.actor.set_position(this._targetX(w), m.y);
            this.actor.set_size(w, h);
        }
    }

    _applyCollapsed(animate) {
        const hidden = this._minimal() && !this._peek && !this._notice;
        this._collapsedView.visible = !hidden;
        this._reposition(animate);
    }

    // ------------------------------------------------------------ expand / collapse
    _onHover() {
        if (this.actor.hover) {
            this._timers.clear(this._collapseTimer);
            if (this._state === 'collapsed') {
                this._timers.clear(this._hoverTimer);
                this._hoverTimer = this._timers.add(70, () => {
                    this._hoverTimer = 0;
                    if (this.actor?.hover) this._expand();
                    return false;
                });
            }
        } else {
            this._timers.clear(this._hoverTimer);
            this._hoverTimer = 0;
            if (this._state === 'expanded') {
                this._timers.clear(this._collapseTimer);
                this._collapseTimer = this._timers.add(350, () => {
                    this._collapseTimer = 0;
                    if (!this.actor?.hover) this._collapse(false);
                    return false;
                });
            }
        }
    }

    _expand() {
        if (this._state === 'expanded') return;
        this._state = 'expanded';
        this._timers.clear(this._peekTimer);
        this._peek = null;
        this._collapsedView.visible = false;
        const [w, h] = this._expandedSize();
        const m = Main.layoutManager.primaryMonitor;
        this.actor.remove_all_transitions();
        this.actor.ease({
            x: this._targetX(w), y: m.y, width: w, height: h, duration: 230,
            mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            onComplete: () => {
                if (this._state !== 'expanded' || !this.actor) return;
                this._expandedView.show();
                this._expandedView.ease({opacity: 255, duration: 120});
                this._pages[this._pageIndex].onShow?.();
            },
        });
    }

    _collapse(force) {
        if (this._state === 'collapsed') return;
        if (!force && (this._pinned || this._grab || this._notice)) return;
        this._endModal();
        this._state = 'collapsed';
        this._expandedView.remove_all_transitions();
        this._expandedView.opacity = 0;
        this._expandedView.hide();
        this._pages[this._pageIndex].onHide?.();
        const key = global.stage.get_key_focus();
        if (key && this.actor.contains(key)) global.stage.set_key_focus(null);
        this._applyCollapsed(true);
        this._updateCollapsed();
    }

    setPinned(v) {
        this._pinned = v;
        if (!v && this._state === 'expanded' && !this.actor.hover) this._onHover();
    }

    // ------------------------------------------------------------ modal keyboard (for text entries)
    beginModal() {
        if (this._grab || !this.actor) return;
        this._grab = Main.pushModal(this.actor, {actionMode: Shell.ActionMode.POPUP});
        if (!this._grab) return;
        this._capSig = global.stage.connect('captured-event', (_s, ev) => {
            if (ev.type() === Clutter.EventType.BUTTON_PRESS) {
                const src = ev.get_source();
                if (!(src && this.actor && this.actor.contains(src))) this._endModal(true);
            }
            return Clutter.EVENT_PROPAGATE;
        });
    }

    _endModal(collapse = false) {
        if (this._capSig) {
            global.stage.disconnect(this._capSig);
            this._capSig = 0;
        }
        if (this._grab) {
            Main.popModal(this._grab);
            this._grab = null;
        }
        if (collapse && this.actor && !this.actor.hover) this._collapse(false);
    }

    // ------------------------------------------------------------ pages
    gotoPage(id) {
        const i = this._pages.findIndex(p => p.id === id);
        if (i >= 0) this._goto(i);
    }

    _goto(i) {
        if (i < 0 || i >= this._pages.length || i === this._pageIndex) return;
        this._endModal();
        if (this._state === 'expanded') this._pages[this._pageIndex].onHide?.();
        this._pageIndex = i;
        this._setPage(i, this._state === 'expanded');
    }

    _setPage(i, show) {
        const page = this._pages[i];
        this._pageHolder.set_child(page.actor);
        this._barButtons.forEach((b, n) => (n === i ? b.add_style_class_name('active') : b.remove_style_class_name('active')));
        if (show) page.onShow?.();
    }

    _onScroll(ev) {
        if (this._state !== 'expanded' || this._notice) return Clutter.EVENT_PROPAGATE;
        const dir = ev.get_scroll_direction();
        let step = 0;
        if (dir === Clutter.ScrollDirection.UP || dir === Clutter.ScrollDirection.LEFT) step = -1;
        else if (dir === Clutter.ScrollDirection.DOWN || dir === Clutter.ScrollDirection.RIGHT) step = 1;
        else if (dir === Clutter.ScrollDirection.SMOOTH) {
            const [dx, dy] = ev.get_scroll_delta();
            this._scrollAcc += Math.abs(dx) > Math.abs(dy) ? dx : dy;
            if (Math.abs(this._scrollAcc) < 1.5) return Clutter.EVENT_STOP;
            step = this._scrollAcc > 0 ? 1 : -1;
            this._scrollAcc = 0;
        }
        const now = GLib.get_monotonic_time() / 1000;
        if (!step || now - this._scrollAt < 220) return Clutter.EVENT_STOP;
        this._scrollAt = now;
        this._goto(this._pageIndex + step);
        return Clutter.EVENT_STOP;
    }

    _onKey(ev) {
        const sym = ev.get_key_symbol();
        if (sym === Clutter.KEY_Escape) {
            if (this._grab) this._endModal(true);
            else this._collapse(true);
            return Clutter.EVENT_STOP;
        }
        const focus = global.stage.get_key_focus();
        if (focus instanceof Clutter.Text) return Clutter.EVENT_PROPAGATE;
        if (sym === Clutter.KEY_Left) {
            this._goto(this._pageIndex - 1);
            return Clutter.EVENT_STOP;
        }
        if (sym === Clutter.KEY_Right) {
            this._goto(this._pageIndex + 1);
            return Clutter.EVENT_STOP;
        }
        return Clutter.EVENT_PROPAGATE;
    }

    // ------------------------------------------------------------ collapsed view
    _onMedia() {
        this._updateCollapsed();
        if (this._state === 'expanded') {
            this._pages[0].refresh();
            this._pages[1].refresh();
        }
    }

    _clockText() {
        const dt = GLib.DateTime.new_now_local();
        return dt.format(this._ctx.is24h() ? '%H:%M' : '%-I:%M');
    }

    _updateCollapsed() {
        if (!this.actor) return;
        const m = this._mpris;
        const playing = m.playing;
        this._leftIcon.visible = playing;
        this._eq.visible = playing;

        const peek = this._peek;
        const eyes = !peek && this._settings.get_boolean('mascot') && this._mascot !== 'awake';
        this._eyes.visible = eyes;
        this._centerLabel.visible = !eyes;
        this._peekIcon.visible = !!peek;
        if (peek) {
            this._peekIcon.icon_name = peek.icon;
            this._centerLabel.text = peek.text;
            this._centerLabel.style_class = 'nusa-peek-text';
        } else {
            this._centerLabel.style_class = 'nusa-clock';
            this._centerLabel.text = this._focus.running
                ? `${this._focus.mode === 'focus' ? '●' : '☕'} ${fmtTime(this._focus.remaining * 1000)}`
                : this._clockText();
        }
        if (this._state === 'collapsed') this._applyCollapsed(true);
    }

    _setMascot(state) {
        this._mascot = state;
        if (!this.actor) return;
        const s = this._scale();
        const doze = state === 'doze';
        this._eyeL.style_class = this._eyeR.style_class = `nusa-eye${doze ? ' doze' : ''}`;
        const w = Math.round(9 * s);
        const h = Math.round((doze ? 2 : 13) * s);
        for (const e of [this._eyeL, this._eyeR]) {
            e.set_size(w, h);
            e.y_align = Clutter.ActorAlign.CENTER;
        }
        this._updateCollapsed();
    }

    // Short status message in the closed notch
    peek(icon, text, ms = 2500) {
        if (!this.actor || this._state === 'expanded') return;
        this._peek = {icon, text};
        this._timers.clear(this._peekTimer);
        this._peekTimer = this._timers.add(ms, () => {
            this._peekTimer = 0;
            this._peek = null;
            this._updateCollapsed();
            this._applyCollapsed(true);
            return false;
        });
        this._updateCollapsed();
    }

    // ------------------------------------------------------------ notices ("Time's up!", etc.)
    _notify({icon, title, sub, actions}) {
        if (!this.actor) return;
        const {px} = this._ctx;
        this._notice = {icon, title, sub, actions};
        const col = vbox(px(6), {x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER, style_class: 'nusa-alert'});
        const top = hbox(px(10), {x_align: Clutter.ActorAlign.CENTER});
        top.add_child(makeIcon(icon, px(28)));
        top.add_child(makeLabel(title, 'nusa-big', {ellipsize: false}));
        col.add_child(top);
        const subLabel = makeLabel(sub, 'nusa-sub');
        subLabel.x_align = Clutter.ActorAlign.CENTER;
        col.add_child(subLabel);
        const row = hbox(px(8), {x_align: Clutter.ActorAlign.CENTER});
        for (const a of actions) {
            row.add_child(pill(a.label, () => {
                a.cb();
                this._dismissNotice();
            }, !!a.accent));
        }
        col.add_child(row);
        this._noticeActor = col;
        this._pageHolder.set_child(col);
        this._expand();
        if (this._state === 'expanded' && !this._expandedView.visible) {
            this._expandedView.show();
            this._expandedView.opacity = 255;
        }
    }

    _dismissNotice() {
        this._notice = null;
        this._noticeActor = null;
        this._pageHolder.set_child(this._pages[this._pageIndex].actor);
        if (!this.actor.hover) this._onHover();
    }
}

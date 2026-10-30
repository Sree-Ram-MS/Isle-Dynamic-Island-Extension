import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Pango from 'gi://Pango';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {Store} from './lib/store.js';
import {MprisWatcher, destroyArtSession} from './lib/mpris.js';
import {LyricsClient} from './lib/lyrics.js';
import {CalendarService, FocusService, IdleWatcher, SystemWatcher, TodoService, WaterService, playChime} from './lib/services.js';
import {Timers, fmtTime} from './lib/util.js';
import {hbox, iconButton, makeIcon, makeLabel, pill, vbox} from './lib/ui.js';
import {makeRing} from './lib/ring.js';

import {MusicPage} from './pages/music.js';
import {LyricsPage} from './pages/lyrics.js';
import {FocusPage} from './pages/focus.js';
import {WaterPage} from './pages/water.js';
import {ShelfPage} from './pages/shelf.js';
import {TodoPage} from './pages/todo.js';
import {CalendarPage} from './pages/calendar.js';
import {WallpaperPage} from './pages/wallpaper.js';
import {PowerPage} from './pages/power.js';
import {HardwarePage} from './pages/hardware.js';

const BASE_W = 190;
const BASE_H = 32;
const OPEN_W = 620;
const OPEN_H = 208;

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
        this._subview = null;
        this._dotsTimer = 0;
        this._pageBarOn = false;
        this._eqTimer = 0;
        this._miniLyrics = false;
        this._miniLyricText = '';
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
        this._cal = new CalendarService(this._store, this._timers);
        this._sys = new SystemWatcher();

        this._ctx = {
            settings: this._settings, store: this._store, timers: this._timers,
            mpris: this._mpris, lyrics: this._lyrics, focus: this._focus,
            water: this._water, todo: this._todo, cal: this._cal, sys: this._sys, notch: this,
            path: this._ext.path,
            is24h: () => false, // always AM/PM
            px: n => Math.round(n * this._scale()),
        };

        this._mpris.changed.connect(() => this._onMedia());
        this._focus.changed.connect(() => {
            this._updateCollapsed();
            this._updateHeader();
        });
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
        this._cal.onDue = (r) => this._notify({
            icon: 'x-office-calendar-symbolic', title: 'Reminder', sub: r.title,
            actions: [
                {label: 'Snooze 10 min', cb: () => this._cal.snooze(r.id, 10)},
                {label: 'Dismiss', accent: true, cb: () => {}},
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
        this._sys.changed.connect(() => this._updateRings());
        this._sys.start();
        this._idle = new IdleWatcher(state => this._setMascot(state));

        this._timers.add(10 * 1000, () => {
            this._updateCollapsed();
            this._updateHeader();
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
        this._ring = makeRing(px, 13);
        this._miniNote = makeLabel('♪', 'nusa-mini-note', {ellipsize: false});
        this._miniNote.visible = false;
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
        this._centerBox.add_child(this._ring.actor);
        this._centerBox.add_child(this._peekIcon);
        this._centerBox.add_child(this._miniNote);
        this._centerBox.add_child(this._centerLabel);
        this._centerBox.add_child(this._eyes);
        this._eq = hbox(px(2), {y_align: Clutter.ActorAlign.CENTER});
        this._eqBase = [6, 11, 8, 12, 5];
        this._eqBars = [];
        for (const h of this._eqBase) {
            const bar = new St.Widget({style_class: 'nusa-eq', height: px(h), y_align: Clutter.ActorAlign.CENTER});
            this._eq.add_child(bar);
            this._eqBars.push(bar);
        }
        this._eq.visible = false;
        this._collapsedView.add_child(this._centerBox);
        this._collapsedView.add_child(this._eq);

        // expanded content: header / page area with < > arrows / page dots
        this._expandedView = vbox(px(2), {x_expand: true, y_expand: true, style_class: 'nusa-expanded', visible: false, opacity: 0});
        this._pageHolder = new St.Bin({
            x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.FILL, y_align: Clutter.ActorAlign.FILL,
            style: `padding: 0 ${px(30)}px;`,
        });
        this._arrowL = iconButton(this._ctx, 'go-previous-symbolic', () => this._step(-1), {size: 14, cls: 'nusa-arrow'});
        this._arrowR = iconButton(this._ctx, 'go-next-symbolic', () => this._step(1), {size: 14, cls: 'nusa-arrow'});
        for (const [b, al] of [[this._arrowL, Clutter.ActorAlign.START], [this._arrowR, Clutter.ActorAlign.END]]) {
            b.x_expand = true;
            b.y_expand = true;
            b.x_align = al;
            b.y_align = Clutter.ActorAlign.CENTER;
        }
        this._pageArea = new St.Widget({x_expand: true, y_expand: true, layout_manager: new Clutter.BinLayout()});
        this._pageArea.add_child(this._pageHolder);
        this._pageArea.add_child(this._arrowL);
        this._pageArea.add_child(this._arrowR);

        this._buildHeader();
        // footer: the page dots. Hovering the footer swaps the dots for a page
        // icon bar (one icon per page, click to jump); moving away restores them.
        // The footer has a fixed height so the swap never resizes the notch.
        this._dotsBox = hbox(px(5), {x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER, style_class: 'nusa-dots'});
        this._pageBar = hbox(px(2), {x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER, style_class: 'nusa-pagebar', visible: false, opacity: 0});
        this._footer = new St.Widget({
            x_expand: true, reactive: true, track_hover: true,
            layout_manager: new Clutter.BinLayout(), style_class: 'nusa-footer',
        });
        this._footer.set_height(px(24));
        this._footer.add_child(this._dotsBox);
        this._footer.add_child(this._pageBar);
        this._footer.connect('notify::hover', () => this._onFooterHover());
        this._expandedView.add_child(this._header);
        this._expandedView.add_child(this._pageArea);
        this._expandedView.add_child(this._footer);

        this.actor.add_child(this._collapsedView);
        this.actor.add_child(this._expandedView);

        this._pages = [
            new MusicPage(this._ctx), new LyricsPage(this._ctx), new FocusPage(this._ctx),
            new WaterPage(this._ctx), new ShelfPage(this._ctx), new TodoPage(this._ctx),
            new CalendarPage(this._ctx), new WallpaperPage(this._ctx), new PowerPage(this._ctx), new HardwarePage(this._ctx),
        ];
        // Settings > Pages decides which pages appear in the dots / icon bar.
        // Lyrics is hidden from the list by default and stays reachable from
        // the Music page's lyrics button either way.
        for (const p of this._pages)
            p.hidden = !this._settings.get_boolean(`show-page-${p.id}`);
        if (!this._pages.some(p => !p.hidden)) this._pages[0].hidden = false;
        if (this._pages[this._pageIndex]?.hidden) {
            const first = this._pages.findIndex(p => !p.hidden);
            if (first >= 0) this._pageIndex = first;
        }
        this._dots = [];
        this._pageIcons = [];
        this._pages.forEach((p, i) => {
            if (p.hidden) return;
            const d = new St.Widget({style_class: 'nusa-pdot', y_align: Clutter.ActorAlign.CENTER, reactive: true, track_hover: true});
            d.set_size(px(6), px(6));
            d.connect('button-press-event', () => {
                this._goto(i);
                return Clutter.EVENT_STOP;
            });
            this._dotsBox.add_child(d);
            this._dots.push([i, d]);
            const b = iconButton(this._ctx, p.icon, () => this._goto(i), {size: 15, cls: 'nusa-picon'});
            this._pageBar.add_child(b);
            this._pageIcons.push([i, b]);
        });

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
        this._timers?.clear(this._dotsTimer);
        this._dotsTimer = 0;
        this._pageBarOn = false;
        this._pageIcons = null;
        this._timers?.clear(this._eqTimer);
        this._eqTimer = 0;
        this._eqBars = null;
        this._trans = null;
        this._miniLyrics = false;
        this._miniLyricText = '';
        for (const id of this._uiSignals ?? []) this.actor.disconnect(id);
        for (const p of this._pages ?? []) p.destroy();
        this._pages = null;
        this._ring = this._hdrRing = this._hdrFocus = null;
        this._hdrBack = this._hdrGear = null;
        this._subview = null;
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
        if (key.startsWith('show-page-')) {
            this._rebuildUi();
            return;
        }
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
            if (!this._settings.get_boolean('lyrics-enabled')) this._setMiniLyrics(false);
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
        else if (this._miniLyrics && this._miniLyricText) {
            // Only the lyric mode changes the normal collapsed width: fit the
            // ♪ + current line, leaving room for the equalizer when playing.
            const chars = Array.from(this._miniLyricText).length;
            w = Math.min(560, Math.max(BASE_W, 76 + chars * 8.6 + (this._eq.visible ? 38 : 0)));
        }
        else if (this._eq.visible) w = BASE_W + 36;
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
        this._updateHeader();
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
        this._finishTransition();
        this._timers.clear(this._dotsTimer);
        this._dotsTimer = 0;
        this._showPageBar(false);
        this._expandedView.remove_all_transitions();
        this._expandedView.opacity = 0;
        this._expandedView.hide();
        this._subview?.onBack?.();
        this.clearSubview();
        const closingPage = this._pages[this._pageIndex];
        closingPage.onHide?.();
        // Closing from the Lyrics page arms the minimized lyric display;
        // closing from any other page turns it off again.
        this._setMiniLyrics(closingPage.id === 'lyrics' && closingPage.canMini?.());
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

    // Next/previous visible page, wrapping around
    _step(dir) {
        const n = this._pages.length;
        let i = this._pageIndex;
        for (let k = 0; k < n; k++) {
            i = (i + dir + n) % n;
            if (!this._pages[i].hidden) break;
        }
        this._goto(i, dir);
    }

    _goto(i, dirHint = 0) {
        if (i < 0 || i >= this._pages.length || i === this._pageIndex) return;
        this._endModal();
        this._subview?.onBack?.();
        this.clearSubview();
        const dir = dirHint || Math.sign(i - this._pageIndex) || 1;
        const oldPage = this._pages[this._pageIndex];
        if (this._state === 'expanded') oldPage.onHide?.();
        this._pageIndex = i;
        if (this._state === 'expanded' && !this._notice) this._transitionTo(i, oldPage, dir);
        else this._setPage(i, this._state === 'expanded');
    }

    // Smooth slide: the new page slides in from the swipe direction while the
    // old one slides out, instead of the page swapping instantly.
    _transitionTo(i, oldPage, dir) {
        const page = this._pages[i];
        this._finishTransition();
        this._setTitle(page);
        this._updatePageChrome(i);
        const w = Math.max(120, this._pageHolder.width);
        const oldActor = oldPage.actor;
        const newActor = page.actor;
        newActor.translation_x = dir * w;
        oldActor.translation_x = 0;
        this._pageHolder.add_child(newActor);
        this._trans = {oldActor, newActor};
        page.onShow?.();
        newActor.ease({
            translation_x: 0, duration: 230, mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            onComplete: () => this._finishTransition(),
        });
        oldActor.ease({
            translation_x: -dir * w, duration: 230, mode: Clutter.AnimationMode.EASE_OUT_QUAD,
        });
    }

    // Settle any in-flight slide instantly (rapid swipes, collapse, sub-views…)
    _finishTransition() {
        if (!this._trans) return;
        const {oldActor, newActor} = this._trans;
        this._trans = null;
        for (const a of [oldActor, newActor]) {
            a.remove_all_transitions();
            a.translation_x = 0;
        }
        if (oldActor.get_parent() === this._pageHolder && oldActor !== newActor)
            this._pageHolder.remove_child(oldActor);
    }

    _updatePageChrome(i) {
        const {px} = this._ctx;
        for (const [idx, d] of this._dots) {
            const on = idx === i;
            d.set_size(on ? px(18) : px(6), px(6));
            if (on) d.add_style_class_name('active');
            else d.remove_style_class_name('active');
        }
        for (const [idx, b] of this._pageIcons ?? []) {
            if (idx === i) b.add_style_class_name('active');
            else b.remove_style_class_name('active');
        }
    }

    _setPage(i, show) {
        const page = this._pages[i];
        this._finishTransition();
        page.actor.translation_x = 0;
        this._pageHolder.set_child(page.actor);
        this._setTitle(page);
        this._updatePageChrome(i);
        if (show) page.onShow?.();
    }

    // Hovering the footer for a beat replaces the dots with the page icon bar;
    // leaving the footer brings the dots back after a short delay.
    _onFooterHover() {
        if (!this._footer || this._subview) return;
        this._timers.clear(this._dotsTimer);
        if (this._footer.hover) {
            this._dotsTimer = this._timers.add(250, () => {
                this._dotsTimer = 0;
                if (this._footer?.hover && !this._subview) this._showPageBar(true);
                return false;
            });
        } else {
            this._dotsTimer = this._timers.add(300, () => {
                this._dotsTimer = 0;
                if (!this._footer?.hover) this._showPageBar(false);
                return false;
            });
        }
    }

    _showPageBar(on) {
        if (!this._pageBar || on === this._pageBarOn) return;
        this._pageBarOn = on;
        this._pageBar.remove_all_transitions();
        this._dotsBox.remove_all_transitions();
        if (on) {
            this._pageBar.show();
            this._pageBar.ease({opacity: 255, duration: 150});
            this._dotsBox.ease({opacity: 0, duration: 120, onComplete: () => {
                if (this._pageBarOn) this._dotsBox.hide();
            }});
        } else {
            this._dotsBox.show();
            this._dotsBox.ease({opacity: 255, duration: 150});
            this._pageBar.ease({opacity: 0, duration: 120, onComplete: () => {
                if (!this._pageBarOn) this._pageBar.hide();
            }});
        }
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
        this._step(step);
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
            this._step(-1);
            return Clutter.EVENT_STOP;
        }
        if (sym === Clutter.KEY_Right) {
            this._step(1);
            return Clutter.EVENT_STOP;
        }
        return Clutter.EVENT_PROPAGATE;
    }

    // ------------------------------------------------------------ collapsed view
    _onMedia() {
        if (this._miniLyrics && !this._mpris.track) this._setMiniLyrics(false);
        if (this._state === 'expanded') {
            this._pages[0].refresh();
            this._pages[1].refresh();
        } else if (this._miniLyrics) {
            this._pages[1].refresh();
        }
        this._updateCollapsed();
    }

    // ---- minimized lyrics ---------------------------------------------------
    _lyricsPage() {
        return this._pages?.find(p => p.id === 'lyrics') ?? null;
    }

    _setMiniLyrics(active) {
        const page = this._lyricsPage();
        if (!page) {
            this._miniLyrics = false;
            this._miniLyricText = '';
            return;
        }
        if (active === this._miniLyrics) {
            if (active) page.setMiniActive(true);
            return;
        }
        this._miniLyrics = !!active;
        if (!this._miniLyrics) this._miniLyricText = '';
        page.setMiniActive(this._miniLyrics);
        if (this._state === 'collapsed') this._updateCollapsed();
    }

    setMiniLyric(text) {
        const next = (text ?? '').trim();
        if (next === this._miniLyricText) return;
        this._miniLyricText = next;
        if (this._state === 'collapsed') this._updateCollapsed();
    }

    _clockText() {
        return GLib.DateTime.new_now_local().format('%-I:%M %p');
    }

    _buildHeader() {
        const px = this._ctx.px;
        // [page title]        [battery ring · date · time]        [focus level · gear]
        this._header = new St.Widget({x_expand: true, style_class: 'nusa-header', layout_manager: new Clutter.BinLayout()});

        // back chevron (only inside a sub-view such as Water > Settings) + page title
        this._hdrBack = iconButton(this._ctx, 'go-previous-symbolic', () => this._subview?.onBack?.(), {size: 14, cls: 'nusa-arrow'});
        this._hdrBack.visible = false;
        this._hdrTitle = makeLabel('', 'nusa-hdr-title', {ellipsize: false});
        const titleBox = hbox(px(8), {x_expand: true, x_align: Clutter.ActorAlign.START, y_align: Clutter.ActorAlign.CENTER});
        titleBox.add_child(this._hdrBack);
        titleBox.add_child(this._hdrTitle);
        this._header.add_child(titleBox);

        const mid = hbox(px(8), {x_expand: true, x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        this._hdrRing = makeRing(px, 16);
        mid.add_child(this._hdrRing.actor);
        this._hdrDate = makeLabel('', 'nusa-hdr-date', {ellipsize: false});
        this._hdrTime = makeLabel('', 'nusa-hdr-time', {ellipsize: false});
        this._hdrAmPm = makeLabel('', 'nusa-hdr-ampm', {ellipsize: false});
        this._hdrAmPm.y_align = Clutter.ActorAlign.END;
        const clock = hbox(px(3), {y_align: Clutter.ActorAlign.CENTER});
        clock.add_child(this._hdrTime);
        clock.add_child(this._hdrAmPm);
        mid.add_child(this._hdrDate);
        mid.add_child(clock);
        this._header.add_child(mid);

        const right = hbox(px(8), {x_expand: true, x_align: Clutter.ActorAlign.END, y_align: Clutter.ActorAlign.CENTER});
        this._hdrFocus = makeRing(px, 20);
        this._hdrFocus.actor.visible = false;
        right.add_child(this._hdrFocus.actor);
        this._hdrGear = iconButton(this._ctx, 'preferences-system-symbolic', () => this.openSettings(), {size: 15, cls: 'nusa-hdr-btn'});
        right.add_child(this._hdrGear);
        this._header.add_child(right);
        this._updateHeader();
    }

    openSettings() {
        this._collapse(true);
        this._ext.openPreferences();
    }

    // A page can open a sub-view (e.g. Water > Settings): back chevron + its title in the header,
    // no side arrows or page dots. onBack() must close the sub-view (and call clearSubview()).
    setSubview(title, onBack) {
        this._subview = {title, onBack};
        this._hdrBack.visible = true;
        this._hdrGear.visible = false;
        this._arrowL.hide();
        this._arrowR.hide();
        this._timers.clear(this._dotsTimer);
        this._dotsTimer = 0;
        this._showPageBar(false);
        this._footer.hide();
        this._hdrTitle.clutter_text.use_markup = false;
        this._hdrTitle.text = title;
    }

    clearSubview() {
        if (!this._subview) return;
        this._subview = null;
        if (!this.actor) return;
        this._hdrBack.visible = false;
        this._hdrGear.visible = true;
        this._arrowL.show();
        this._arrowR.show();
        this._footer.show();
        this._setTitle(this._pages[this._pageIndex]);
    }

    endModal() {
        this._endModal();
    }

    _setTitle(page) {
        if (page.titleMarkup) this._hdrTitle.clutter_text.set_markup(page.titleMarkup);
        else {
            this._hdrTitle.clutter_text.use_markup = false;
            this._hdrTitle.text = page.title ?? page.name ?? '';
        }
    }

    // battery ring (header + closed notch) and the red/teal focus "water" circle
    _updateRings() {
        const f = this._focus;
        const frac = f.total > 0 ? f.remaining / f.total : 1;
        const active = f.running || f.remaining < f.total;
        if (this._ring) {
            if (f.running) this._ring.setFocus(frac, f.mode);
            else this._ring.setBattery(this._sys.battery);
        }
        if (this._hdrRing) this._hdrRing.setBattery(this._sys.battery);
        if (this._hdrFocus) {
            this._hdrFocus.actor.visible = active;
            if (active) this._hdrFocus.setFocus(frac, f.mode);
        }
    }

    _updateHeader() {
        if (!this._hdrDate) return;
        const dt = GLib.DateTime.new_now_local();
        this._hdrDate.text = dt.format('%a %-d %b %Y');
        this._hdrTime.text = dt.format('%-I:%M');
        this._hdrAmPm.text = dt.format('%p');
        this._updateRings();
    }

    _updateCollapsed() {
        if (!this.actor) return;
        const m = this._mpris;
        const playing = m.playing;
        this._eq.visible = playing;
        this._syncEqAnim(playing);

        const peek = this._peek;
        const mini = !peek && this._miniLyrics && !!this._miniLyricText;
        const eyes = !peek && !mini && this._settings.get_boolean('mascot') && this._mascot !== 'awake';
        this._eyes.visible = eyes;
        this._ring.actor.visible = !eyes && !peek && !mini;
        this._updateRings();
        this._centerLabel.visible = !eyes;
        this._peekIcon.visible = !!peek;
        this._miniNote.visible = mini;
        this._centerLabel.clutter_text.ellipsize = mini ? Pango.EllipsizeMode.END : Pango.EllipsizeMode.NONE;
        if (peek) {
            this._peekIcon.icon_name = peek.icon;
            this._centerLabel.text = peek.text;
            this._centerLabel.style_class = 'nusa-peek-text';
        } else if (mini) {
            this._centerLabel.style_class = 'nusa-mini-lyric';
            this._centerLabel.text = this._miniLyricText;
        } else {
            this._centerLabel.style_class = 'nusa-clock';
            this._centerLabel.text = this._focus.running
                ? fmtTime(this._focus.remaining * 1000)
                : this._clockText();
        }
        if (this._state === 'collapsed') this._applyCollapsed(true);
    }

    // The sound-wave bars dance while music is playing; they rest at their
    // base heights when playback stops.
    _syncEqAnim(playing) {
        if (playing && !this._eqTimer) {
            this._eqTimer = this._timers.add(170, () => {
                if (!this._eqBars || !this._ctx) return false;
                const px = this._ctx.px;
                for (const bar of this._eqBars) {
                    bar.remove_all_transitions();
                    bar.ease({
                        height: px(3 + Math.round(Math.random() * 10)),
                        duration: 160, mode: Clutter.AnimationMode.EASE_OUT_QUAD,
                    });
                }
                return true;
            });
        } else if (!playing && this._eqTimer) {
            this._timers.clear(this._eqTimer);
            this._eqTimer = 0;
            const px = this._ctx.px;
            (this._eqBars ?? []).forEach((bar, i) => {
                bar.remove_all_transitions();
                bar.set_height(px(this._eqBase[i]));
            });
        }
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
        this._arrowL.hide();
        this._arrowR.hide();
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
        this._arrowL.show();
        this._arrowR.show();
        this._finishTransition();
        const a = this._pages[this._pageIndex].actor;
        a.translation_x = 0;
        this._pageHolder.set_child(a);
        if (!this.actor.hover) this._onHover();
    }
}
